//! Form model of the public registration and the team page: which fields the organiser lets a
//! guest enter, which are required, how they are validated (rules from the storage layer, texts in
//! the visitor's language) and how they become a `TeamRegistration` / `TeamUpdate`. No UI code in
//! here. Everything a request needs is checked here first, so the page never sends a request the
//! server would refuse for data it could have checked itself.

use uuid::Uuid;

use super::lang::Txt;
use crate::storage::{AddressData, RequiredField, ShareTeamConfig, TeamData, TeamRegistration, TeamUpdate};

/// A set of the optional details. Name and address are always part of the form.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VisibleFields {
    pub mail: bool,
    pub phone: bool,
    pub members: bool,
    pub diets: bool,
}

impl VisibleFields {
    pub fn from_config(fields: &[RequiredField]) -> Self {
        Self {
            mail: fields.contains(&RequiredField::Mail),
            phone: fields.contains(&RequiredField::Phone),
            members: fields.contains(&RequiredField::Members),
            diets: fields.contains(&RequiredField::Diets),
        }
    }
}

/// The organiser's settings for the form: what a guest may enter and what he has to enter.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FieldRules {
    pub allowed: VisibleFields,
    pub required: VisibleFields,
}

impl FieldRules {
    pub fn from_config(config: &ShareTeamConfig) -> Self {
        let allowed = VisibleFields::from_config(&config.allowed_fields);
        // A required detail that is not allowed cannot be entered, so it cannot be required.
        let required = VisibleFields::from_config(&config.required_fields);
        Self {
            allowed,
            required: VisibleFields {
                mail: required.mail && allowed.mail,
                phone: required.phone && allowed.phone,
                members: required.members && allowed.members,
                diets: required.diets && allowed.diets,
            },
        }
    }

    /// Without a configuration (could not be loaded): everything may be entered, nothing is
    /// demanded; the server has the last word.
    pub fn open() -> Self {
        Self {
            allowed: VisibleFields { mail: true, phone: true, members: true, diets: true },
            required: VisibleFields::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RegisterDraft {
    pub name: String,
    pub mail: String,
    pub phone: String,
    pub members: String,
    pub diets: String,
}

/// One message per field (already translated); empty string = valid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RegisterErrors {
    pub name: String,
    pub mail: String,
    pub phone: String,
    pub members: String,
    pub diets: String,
}

impl RegisterErrors {
    pub fn is_empty(&self) -> bool {
        [&self.name, &self.mail, &self.phone, &self.members, &self.diets]
            .iter()
            .all(|message| message.is_empty())
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn parse_members(value: &str) -> Result<Option<u16>, ()> {
    match non_empty(value) {
        None => Ok(None),
        Some(text) => text.parse::<u16>().map(Some).map_err(|_| ()),
    }
}

/// `true` if the storage layer accepts a team that is valid except for what `change` modifies.
fn accepted(change: &dyn Fn(&mut TeamRegistration)) -> bool {
    let mut team = TeamRegistration {
        name: "Team".to_string(),
        address: AddressData {
            address: "Street 1, 12345 City".to_string(),
            latitude: 50.0,
            longitude: 8.0,
        },
        mail: None,
        phone: None,
        members: None,
        diets: None,
        access_token: Uuid::nil(),
        honeypot: None,
    };
    change(&mut team);
    team.validate().is_ok()
}

impl RegisterDraft {
    pub fn from_team(team: &TeamData) -> Self {
        Self {
            name: team.name.clone(),
            mail: team.mail.clone().unwrap_or_default(),
            phone: team.phone.clone().unwrap_or_default(),
            members: team.members.map(|members| members.to_string()).unwrap_or_default(),
            diets: team.diets.clone().unwrap_or_default(),
        }
    }

    /// What is wrong with the form, as translated messages. Used for the registration and for
    /// editing alike. Only the details the organiser allows are looked at: a required one must
    /// be filled in, an optional one has to be valid when it is filled in.
    pub fn errors(&self, rules: FieldRules, t: &Txt) -> RegisterErrors {
        let mut errors = RegisterErrors::default();
        let say = |message: &str| message.to_string();
        let FieldRules { allowed, required } = rules;

        if self.name.trim().is_empty() || !accepted(&|team| team.name = self.name.trim().to_string()) {
            errors.name = say(t.err_name_empty);
        }

        if allowed.mail {
            let empty = self.mail.trim().is_empty();
            if (empty && required.mail) || (!empty && !accepted(&|team| team.mail = non_empty(&self.mail))) {
                errors.mail = say(t.err_email_invalid);
            }
        }

        if allowed.phone {
            if self.phone.trim().is_empty() {
                if required.phone {
                    errors.phone = say(t.err_phone_empty);
                }
            } else if !accepted(&|team| team.phone = non_empty(&self.phone)) {
                errors.phone = say(t.err_phone_invalid);
            }
        }

        if allowed.members {
            errors.members = match parse_members(&self.members) {
                Ok(None) if required.members => say(t.err_members_invalid),
                Ok(None) => String::new(),
                Err(()) => say(t.err_members_invalid),
                Ok(Some(0)) => say(t.err_members_zero),
                Ok(Some(count)) if !accepted(&|team| team.members = Some(count)) => say(t.err_members_invalid),
                Ok(Some(_)) => String::new(),
            };
        }

        if allowed.diets {
            let empty = self.diets.trim().is_empty();
            if (empty && required.diets) || (!empty && !accepted(&|team| team.diets = non_empty(&self.diets))) {
                errors.diets = say(t.err_diets_invalid);
            }
        }
        errors
    }

    /// Full overwrite of the master data (`TeamUpdate` semantics): empty optional fields become
    /// `None`. A detail the organiser does not allow is not taken from the form but stays as the
    /// team has it (`existing`), so the request never changes what the guest may not touch.
    /// `needs_check` keeps the current status; whether a change triggers a new review is up to the
    /// organizer's `review_trigger_fields`, which the backend applies.
    pub fn to_update(
        &self,
        address: AddressData,
        needs_check: bool,
        allowed: VisibleFields,
        existing: &TeamData,
    ) -> Result<TeamUpdate, String> {
        Ok(TeamUpdate {
            name: self.name.trim().to_string(),
            address,
            mail: if allowed.mail { non_empty(&self.mail) } else { existing.mail.clone() },
            phone: if allowed.phone { non_empty(&self.phone) } else { existing.phone.clone() },
            members: if allowed.members {
                parse_members(&self.members).map_err(|()| "Invalid number of members".to_string())?
            } else {
                existing.members
            },
            diets: if allowed.diets { non_empty(&self.diets) } else { existing.diets.clone() },
            needs_check,
        })
    }

    /// Details that are not allowed are never sent, even if something was typed into them
    /// earlier. `honeypot` is the content of the invisible bot trap (empty for people).
    pub fn to_registration(
        &self,
        allowed: VisibleFields,
        address: AddressData,
        access_token: Uuid,
        honeypot: &str,
    ) -> Result<TeamRegistration, String> {
        Ok(TeamRegistration {
            name: self.name.trim().to_string(),
            address,
            mail: if allowed.mail { non_empty(&self.mail) } else { None },
            phone: if allowed.phone { non_empty(&self.phone) } else { None },
            members: if allowed.members {
                parse_members(&self.members).map_err(|()| "Invalid number of members".to_string())?
            } else {
                None
            },
            diets: if allowed.diets { non_empty(&self.diets) } else { None },
            access_token,
            honeypot: non_empty(honeypot),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::lang::Lang;
    use super::*;

    fn all() -> VisibleFields {
        VisibleFields { mail: true, phone: true, members: true, diets: true }
    }

    fn rules(allowed: VisibleFields, required: VisibleFields) -> FieldRules {
        FieldRules { allowed, required }
    }

    fn filled() -> RegisterDraft {
        RegisterDraft {
            name: "Chili Chasers".into(),
            mail: "chili@chasers.de".into(),
            phone: "+49 1234 56789".into(),
            members: "2".into(),
            diets: "vegan".into(),
        }
    }

    fn address() -> AddressData {
        AddressData { address: "Street 1, 12345 City".into(), latitude: 50.0, longitude: 8.0 }
    }

    #[test]
    fn rules_follow_the_three_settings_of_the_organiser() {
        let mut config = ShareTeamConfig::default();
        config.allowed_fields = vec![RequiredField::Mail, RequiredField::Diets];
        config.required_fields = vec![RequiredField::Mail];
        let rules = FieldRules::from_config(&config);
        assert!(rules.allowed.mail && rules.allowed.diets && !rules.allowed.phone && !rules.allowed.members);
        // diets: allowed but optional
        assert!(rules.required.mail && !rules.required.diets);
    }

    #[test]
    fn a_detail_that_is_not_allowed_cannot_be_required() {
        let mut config = ShareTeamConfig::default();
        config.allowed_fields = vec![RequiredField::Mail];
        config.required_fields = vec![RequiredField::Mail, RequiredField::Phone];
        assert!(!FieldRules::from_config(&config).required.phone);
    }

    #[test]
    fn nothing_is_demanded_without_a_configuration() {
        let t = Lang::En.txt();
        let draft = RegisterDraft { name: "Team".into(), ..Default::default() };
        assert!(draft.errors(FieldRules::open(), t).is_empty());
    }

    #[test]
    fn only_what_is_required_has_to_be_filled_in() {
        let t = Lang::En.txt();
        let draft = RegisterDraft { name: "Team".into(), ..Default::default() };
        let required = VisibleFields { mail: true, phone: true, members: true, diets: false };
        let errors = draft.errors(rules(all(), required), t);
        assert_eq!(errors.mail, t.err_email_invalid);
        assert_eq!(errors.phone, t.err_phone_empty);
        assert_eq!(errors.members, t.err_members_invalid);
        assert!(errors.diets.is_empty(), "optional and empty is fine");
    }

    #[test]
    fn optional_details_are_checked_when_filled_in() {
        let t = Lang::De.txt();
        let draft = RegisterDraft { name: "Team".into(), mail: "nonsense".into(), members: "0".into(), ..Default::default() };
        let errors = draft.errors(rules(all(), VisibleFields::default()), t);
        assert_eq!(errors.mail, t.err_email_invalid);
        assert_eq!(errors.members, t.err_members_zero);
    }

    #[test]
    fn details_that_are_not_allowed_are_not_checked_and_not_sent() {
        let draft = RegisterDraft { name: "Team".into(), mail: "garbage".into(), members: "x".into(), ..Default::default() };
        let none = VisibleFields::default();
        // not even "required" matters for what is not allowed
        assert!(draft.errors(rules(none, all()), Lang::De.txt()).is_empty());

        let registration = draft.to_registration(none, address(), Uuid::nil(), "").unwrap();
        assert_eq!((registration.mail, registration.phone, registration.members, registration.diets), (None, None, None, None));
    }

    #[test]
    fn a_required_diet_has_to_be_filled_in() {
        let t = Lang::En.txt();
        let draft = RegisterDraft { diets: "".into(), ..filled() };
        let required = VisibleFields { diets: true, ..VisibleFields::default() };
        assert_eq!(draft.errors(rules(all(), required), t).diets, t.err_diets_invalid);
        assert!(draft.errors(rules(all(), VisibleFields::default()), t).is_empty());
    }

    #[test]
    fn name_is_always_required() {
        let errors = RegisterDraft { name: " ".into(), ..filled() }.errors(FieldRules::default(), Lang::En.txt());
        assert_eq!(errors.name, Lang::En.txt().err_name_empty);
    }

    #[test]
    fn invalid_mail_uses_the_storage_rules() {
        for bad in ["no-at-sign", "a@b", "@x.de", "x@.de"] {
            let errors = RegisterDraft { mail: bad.into(), ..filled() }.errors(rules(all(), all()), Lang::En.txt());
            assert_eq!(errors.mail, Lang::En.txt().err_email_invalid, "'{bad}'");
        }
    }

    #[test]
    fn members_zero_and_garbage_get_their_own_messages() {
        let t = Lang::En.txt();
        let r = rules(all(), all());
        assert_eq!(RegisterDraft { members: "0".into(), ..filled() }.errors(r, t).members, t.err_members_zero);
        // 255 is what the server's database holds
        for bad in ["abc", "-3", "2.5", "70000", "256"] {
            assert_eq!(RegisterDraft { members: bad.into(), ..filled() }.errors(r, t).members, t.err_members_invalid, "'{bad}'");
        }
        assert!(RegisterDraft { members: "255".into(), ..filled() }.errors(r, t).is_empty());
    }

    #[test]
    fn messages_follow_the_language() {
        let draft = RegisterDraft { name: "".into(), ..filled() };
        let r = rules(all(), all());
        assert_ne!(draft.errors(r, Lang::De.txt()).name, draft.errors(r, Lang::En.txt()).name);
    }

    #[test]
    fn registration_carries_the_access_token_and_trimmed_values() {
        let token = Uuid::from_u128(42);
        let draft = RegisterDraft { name: "  Chili  ".into(), ..filled() };
        let registration = draft.to_registration(all(), address(), token, "").unwrap();
        assert_eq!(registration.name, "Chili");
        assert_eq!(registration.access_token, token);
        assert_eq!(registration.members, Some(2));
        assert_eq!(registration.honeypot, None);
        assert!(registration.validate().is_ok());
    }

    #[test]
    fn a_filled_honeypot_travels_with_the_registration() {
        let registration = filled().to_registration(all(), address(), Uuid::nil(), " http://spam ").unwrap();
        assert_eq!(registration.honeypot.as_deref(), Some("http://spam"));
    }

    #[test]
    fn a_loaded_team_round_trips_into_a_valid_update() {
        let mut team = TeamData::default();
        team.name = "Chili Chasers".into();
        team.mail = Some("chili@chasers.de".into());
        team.phone = None;
        team.members = Some(3);
        team.diets = None;

        let draft = RegisterDraft::from_team(&team);
        assert_eq!(draft.members, "3");
        assert_eq!(draft.phone, "");
        assert!(draft.errors(rules(all(), VisibleFields::default()), Lang::En.txt()).is_empty());

        let update = draft.to_update(address(), true, all(), &team).unwrap();
        assert!(update.validate().is_ok());
        assert!(update.needs_check);
        assert_eq!((update.mail.as_deref(), update.phone, update.members, update.diets), (Some("chili@chasers.de"), None, Some(3), None));
    }

    #[test]
    fn an_update_keeps_what_the_guest_may_not_touch() {
        let mut team = TeamData::default();
        team.name = "T".into();
        team.mail = Some("old@x.de".into());
        team.phone = Some("0151".into());
        team.diets = Some("vegan".into());
        team.members = None;
        // the organiser only lets guests change the mail now; whatever sits in the form for the rest
        let allowed = VisibleFields { mail: true, ..VisibleFields::default() };
        let draft = RegisterDraft { name: "T".into(), mail: "new@x.de".into(), phone: "999".into(), members: "9".into(), diets: "x".into() };
        let update = draft.to_update(address(), false, allowed, &team).unwrap();
        assert_eq!(update.mail.as_deref(), Some("new@x.de"));
        assert_eq!(update.phone.as_deref(), Some("0151"));
        assert_eq!(update.diets.as_deref(), Some("vegan"));
        assert_eq!(update.members, None);
    }
}
