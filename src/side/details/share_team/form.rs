//! Form model of the public registration: which fields are shown, how they are validated
//! (rules from the storage layer, texts in the visitor's language) and how they become a
//! `TeamRegistration`. No UI code in here.

use uuid::Uuid;

use super::lang::Txt;
use crate::storage::{AddressData, RequiredField, TeamRegistration};

/// Fields the organizer asks for. Name and address are always part of the form.
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
    };
    change(&mut team);
    team.validate().is_ok()
}

impl RegisterDraft {
    pub fn errors(&self, visible: VisibleFields, t: &Txt) -> RegisterErrors {
        let mut errors = RegisterErrors::default();
        let say = |message: &str| message.to_string();

        if self.name.trim().is_empty() || !accepted(&|team| team.name = self.name.trim().to_string()) {
            errors.name = say(t.err_name_empty);
        }

        if visible.mail
            && (self.mail.trim().is_empty() || !accepted(&|team| team.mail = non_empty(&self.mail)))
        {
            errors.mail = say(t.err_email_invalid);
        }

        if visible.phone {
            if self.phone.trim().is_empty() {
                errors.phone = say(t.err_phone_empty);
            } else if !accepted(&|team| team.phone = non_empty(&self.phone)) {
                errors.phone = say(t.err_phone_invalid);
            }
        }

        if visible.members {
            errors.members = match parse_members(&self.members) {
                Ok(None) | Err(()) => say(t.err_members_invalid),
                Ok(Some(0)) => say(t.err_members_zero),
                Ok(Some(count)) if !accepted(&|team| team.members = Some(count)) => say(t.err_members_invalid),
                Ok(Some(_)) => String::new(),
            };
        }

        // Optional, but must be acceptable if filled in.
        if visible.diets && !accepted(&|team| team.diets = non_empty(&self.diets)) {
            errors.diets = say(t.err_diets_invalid);
        }
        errors
    }

    /// Hidden fields are never sent, even if something was typed into them earlier.
    pub fn to_registration(
        &self,
        visible: VisibleFields,
        address: AddressData,
        access_token: Uuid,
    ) -> Result<TeamRegistration, String> {
        Ok(TeamRegistration {
            name: self.name.trim().to_string(),
            address,
            mail: if visible.mail { non_empty(&self.mail) } else { None },
            phone: if visible.phone { non_empty(&self.phone) } else { None },
            members: if visible.members {
                parse_members(&self.members).map_err(|()| "Invalid number of members".to_string())?
            } else {
                None
            },
            diets: if visible.diets { non_empty(&self.diets) } else { None },
            access_token,
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
    fn complete_form_is_valid() {
        assert!(filled().errors(all(), Lang::En.txt()).is_empty());
    }

    #[test]
    fn name_is_always_required() {
        let errors = RegisterDraft { name: " ".into(), ..filled() }.errors(VisibleFields::default(), Lang::En.txt());
        assert_eq!(errors.name, Lang::En.txt().err_name_empty);
    }

    #[test]
    fn hidden_fields_are_not_validated_and_not_sent() {
        let draft = RegisterDraft { name: "Team".into(), mail: "garbage".into(), members: "x".into(), ..Default::default() };
        let hidden = VisibleFields::default();
        assert!(draft.errors(hidden, Lang::De.txt()).is_empty());

        let registration = draft.to_registration(hidden, address(), Uuid::nil()).unwrap();
        assert_eq!((registration.mail, registration.phone, registration.members, registration.diets), (None, None, None, None));
    }

    #[test]
    fn shown_mail_phone_and_members_are_required() {
        let errors = RegisterDraft { name: "Team".into(), ..Default::default() }
            .errors(VisibleFields { mail: true, phone: true, members: true, diets: false }, Lang::De.txt());
        let t = Lang::De.txt();
        assert_eq!(errors.mail, t.err_email_invalid);
        assert_eq!(errors.phone, t.err_phone_empty);
        assert_eq!(errors.members, t.err_members_invalid);
    }

    #[test]
    fn diets_stay_optional() {
        let draft = RegisterDraft { diets: "".into(), ..filled() };
        assert!(draft.errors(all(), Lang::En.txt()).is_empty());
    }

    #[test]
    fn invalid_mail_uses_the_storage_rules() {
        for bad in ["no-at-sign", "a@b", "@x.de", "x@.de"] {
            let errors = RegisterDraft { mail: bad.into(), ..filled() }.errors(all(), Lang::En.txt());
            assert_eq!(errors.mail, Lang::En.txt().err_email_invalid, "'{bad}'");
        }
    }

    #[test]
    fn members_zero_and_garbage_get_their_own_messages() {
        let t = Lang::En.txt();
        assert_eq!(RegisterDraft { members: "0".into(), ..filled() }.errors(all(), t).members, t.err_members_zero);
        for bad in ["abc", "-3", "2.5", "70000"] {
            assert_eq!(RegisterDraft { members: bad.into(), ..filled() }.errors(all(), t).members, t.err_members_invalid, "'{bad}'");
        }
    }

    #[test]
    fn messages_follow_the_language() {
        let draft = RegisterDraft { name: "".into(), ..filled() };
        assert_ne!(draft.errors(all(), Lang::De.txt()).name, draft.errors(all(), Lang::En.txt()).name);
    }

    #[test]
    fn registration_carries_the_access_token_and_trimmed_values() {
        let token = Uuid::from_u128(42);
        let draft = RegisterDraft { name: "  Chili  ".into(), ..filled() };
        let registration = draft.to_registration(all(), address(), token).unwrap();
        assert_eq!(registration.name, "Chili");
        assert_eq!(registration.access_token, token);
        assert_eq!(registration.members, Some(2));
        assert!(registration.validate().is_ok());
    }
}
