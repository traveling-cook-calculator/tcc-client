//! Form model of a team: what the user types, how it is validated and how it becomes
//! a `TeamCreate` / `TeamUpdate`. No UI code in here.
//!
//! Validation rules live in the storage layer (`TeamUpdate::validate`). The form only needs
//! them *per field*, so each field is checked against an otherwise valid team ("probe").

use crate::storage::{AddressData, TeamCreate, TeamData, TeamUpdate};

/// Raw text of the form fields (the address has its own component and state).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TeamDraft {
    pub name: String,
    pub mail: String,
    pub phone: String,
    pub members: String,
    pub diets: String,
}

/// One message per field; empty string = valid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TeamErrors {
    pub name: String,
    pub mail: String,
    pub phone: String,
    pub members: String,
    pub diets: String,
}

impl TeamErrors {
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

/// `Ok(None)` for an empty field, `Err` for something that is not a number.
fn parse_members(value: &str) -> Result<Option<u16>, String> {
    match non_empty(value) {
        None => Ok(None),
        Some(text) => text
            .parse::<u16>()
            .map(Some)
            .map_err(|_| "Please enter a whole number.".to_string()),
    }
}

impl TeamDraft {
    pub fn from_team(team: &TeamData) -> Self {
        Self {
            name: team.name.clone(),
            mail: team.mail.clone().unwrap_or_default(),
            phone: team.phone.clone().unwrap_or_default(),
            members: team.members.map(|m| m.to_string()).unwrap_or_default(),
            diets: team.diets.clone().unwrap_or_default(),
        }
    }

    /// Validates every field on its own.
    pub fn errors(&self) -> TeamErrors {
        let probe = |change: &dyn Fn(&mut TeamUpdate)| -> String {
            let mut team = TeamUpdate {
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
                needs_check: false,
            };
            change(&mut team);
            team.validate().err().unwrap_or_default()
        };

        let members = match parse_members(&self.members) {
            Ok(parsed) => probe(&|team| team.members = parsed),
            Err(message) => message,
        };

        TeamErrors {
            name: probe(&|team| team.name = self.name.trim().to_string()),
            mail: probe(&|team| team.mail = non_empty(&self.mail)),
            phone: probe(&|team| team.phone = non_empty(&self.phone)),
            members,
            diets: probe(&|team| team.diets = non_empty(&self.diets)),
        }
    }

    pub fn to_create(&self, address: AddressData, needs_check: bool) -> Result<TeamCreate, String> {
        Ok(TeamCreate {
            name: self.name.trim().to_string(),
            address,
            mail: non_empty(&self.mail),
            phone: non_empty(&self.phone),
            members: parse_members(&self.members)?,
            diets: non_empty(&self.diets),
            needs_check,
        })
    }

    pub fn to_update(&self, address: AddressData, needs_check: bool) -> Result<TeamUpdate, String> {
        Ok(TeamUpdate {
            name: self.name.trim().to_string(),
            address,
            mail: non_empty(&self.mail),
            phone: non_empty(&self.phone),
            members: parse_members(&self.members)?,
            diets: non_empty(&self.diets),
            needs_check,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> TeamDraft {
        TeamDraft { name: "The Chili Chasers".into(), ..TeamDraft::default() }
    }

    fn address() -> AddressData {
        AddressData { address: "Street 1, 12345 City".into(), latitude: 50.0, longitude: 8.0 }
    }

    #[test]
    fn minimal_team_is_valid() {
        assert!(valid().errors().is_empty());
        assert!(valid().to_create(address(), false).unwrap().validate().is_ok());
    }

    #[test]
    fn empty_optional_fields_become_none() {
        let update = valid().to_update(address(), true).unwrap();
        assert_eq!((update.mail, update.phone, update.members, update.diets), (None, None, None, None));
        assert!(update.needs_check);
    }

    #[test]
    fn name_is_required_and_reported_on_the_name_field_only() {
        let errors = TeamDraft { name: "  ".into(), ..valid() }.errors();
        assert!(!errors.name.is_empty());
        assert!(errors.mail.is_empty() && errors.phone.is_empty() && errors.members.is_empty());
    }

    #[test]
    fn invalid_mail_is_reported_on_the_mail_field_only() {
        let errors = TeamDraft { mail: "not-an-email".into(), ..valid() }.errors();
        assert!(!errors.mail.is_empty());
        assert!(errors.name.is_empty());
        assert!(TeamDraft { mail: "chili@chasers.de".into(), ..valid() }.errors().mail.is_empty());
    }

    #[test]
    fn members_must_be_a_positive_whole_number() {
        for bad in ["abc", "-1", "2.5", "0", "70000"] {
            let errors = TeamDraft { members: bad.into(), ..valid() }.errors();
            assert!(!errors.members.is_empty(), "'{bad}' should be rejected");
        }
        assert!(TeamDraft { members: "4".into(), ..valid() }.errors().members.is_empty());
        assert!(TeamDraft { members: "".into(), ..valid() }.errors().members.is_empty());
    }

    #[test]
    fn unparsable_members_cannot_become_a_request() {
        assert!(TeamDraft { members: "abc".into(), ..valid() }.to_create(address(), false).is_err());
    }

    #[test]
    fn values_are_trimmed() {
        let update = TeamDraft { name: "  Team  ".into(), diets: " vegan ".into(), ..valid() }
            .to_update(address(), false)
            .unwrap();
        assert_eq!(update.name, "Team");
        assert_eq!(update.diets.as_deref(), Some("vegan"));
    }
}
