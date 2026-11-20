//! How the change log of a team is shown (Team dialog -> History).
//!
//! Every entry has an actor, an action and, for `updated`, the changed fields as
//! `[field, NEW value, OLD value]` triples (the order of the backend; its code comments say
//! otherwise). All values are strings; an empty optional value is `–`. The address is the debug
//! output of an object: it is never shown, only that it changed. Unknown fields, actions and actors
//! are shown as they came.

use super::{AuditAction, AuditActorType, AuditChangedField, AuditLogEntry};

/// "Organizer" / "Team". The label of an admin is the opaque `sub` of the JWT, not a name.
pub fn actor_text(entry: &AuditLogEntry) -> String {
    match &entry.actor_type {
        AuditActorType::Admin => "Organizer".to_string(),
        AuditActorType::Participant => "Team".to_string(),
        AuditActorType::Other(raw) => raw.clone(),
    }
}

pub fn action_text(action: &AuditAction) -> String {
    match action {
        AuditAction::Created => "Created".to_string(),
        AuditAction::Updated => "Updated".to_string(),
        AuditAction::Canceled => "Canceled".to_string(),
        AuditAction::ResendVerificationMail => "Verification email sent again".to_string(),
        AuditAction::EmailVerified => "Email address confirmed".to_string(),
        AuditAction::PlanInvalidated => "Plan invalidated".to_string(),
        AuditAction::Other(raw) => raw.clone(),
    }
}

/// One changed field, ready to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeView {
    /// `label: old -> new`
    Value { label: String, old: String, new: String },
    /// Only the fact that the field changed (the address).
    Changed { label: String },
}

impl ChangeView {
    pub fn text(&self) -> String {
        match self {
            ChangeView::Value { label, old, new } => format!("{label}: {old} → {new}"),
            ChangeView::Changed { label } => format!("{label} changed"),
        }
    }
}

fn field_label(field: &str) -> Option<&'static str> {
    Some(match field {
        "name" => "Team name",
        "mail" => "Email",
        "phone" => "Phone",
        "members" => "Members",
        "diets" => "Dietary notes",
        "status" => "Status",
        "address" => "Address",
        _ => return None,
    })
}

/// The status in words (the backend sends the Rust debug form).
fn status_text(value: &str) -> String {
    match value {
        "Active" => "Active".to_string(),
        "Review" => "Needs review".to_string(),
        "Canceled" => "Canceled".to_string(),
        other => other.to_string(),
    }
}

pub fn describe_change(change: &AuditChangedField) -> ChangeView {
    let label = field_label(&change.field).map_or_else(|| change.field.clone(), str::to_string);
    match change.field.as_str() {
        "address" => ChangeView::Changed { label },
        "status" => ChangeView::Value {
            label,
            old: status_text(&change.old_value),
            new: status_text(&change.new_value),
        },
        _ => ChangeView::Value {
            label,
            old: change.old_value.clone(),
            new: change.new_value.clone(),
        },
    }
}

pub fn describe_changes(changes: &[AuditChangedField]) -> Vec<ChangeView> {
    changes.iter().map(describe_change).collect()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use uuid::Uuid;

    use super::*;

    fn field(field: &str, new: &str, old: &str) -> AuditChangedField {
        AuditChangedField { field: field.into(), new_value: new.into(), old_value: old.into() }
    }

    fn entry(actor_type: AuditActorType, label: Option<&str>) -> AuditLogEntry {
        AuditLogEntry {
            id: Uuid::new_v4(),
            actor_type,
            actor_label: label.map(str::to_string),
            action: AuditAction::Created,
            changed_fields: Vec::new(),
            created_at: NaiveDate::from_ymd_opt(2026, 10, 30).unwrap().and_hms_opt(9, 0, 0).unwrap(),
        }
    }

    #[test]
    fn the_new_value_comes_before_the_old_one() {
        let view = describe_change(&field("mail", "neu@run.de", "alt@run.de"));
        assert_eq!(view.text(), "Email: alt@run.de → neu@run.de");
    }

    #[test]
    fn an_empty_value_stays_a_dash() {
        let view = describe_change(&field("phone", "–", "0170 1"));
        assert_eq!(view.text(), "Phone: 0170 1 → –");
    }

    #[test]
    fn the_status_is_put_into_words() {
        let view = describe_change(&field("status", "Review", "Active"));
        assert_eq!(view.text(), "Status: Active → Needs review");
        assert_eq!(describe_change(&field("status", "Canceled", "Review")).text(), "Status: Needs review → Canceled");
    }

    #[test]
    fn the_address_is_never_shown() {
        let view = describe_change(&field("address", "AddressData { street: \"Secret\" }", "AddressData { .. }"));
        assert_eq!(view.text(), "Address changed");
        assert!(!view.text().contains("Secret"));
    }

    #[test]
    fn unknown_fields_are_shown_as_they_are() {
        let view = describe_change(&field("shoe_size", "43", "42"));
        assert_eq!(view.text(), "shoe_size: 42 → 43");
        assert_eq!(describe_change(&field("status", "Weird", "Active")).text(), "Status: Active → Weird");
    }

    #[test]
    fn actors_are_roles_not_names() {
        assert_eq!(actor_text(&entry(AuditActorType::Admin, Some("8f3c-sub"))), "Organizer");
        assert_eq!(actor_text(&entry(AuditActorType::Participant, None)), "Team");
        assert_eq!(actor_text(&entry(AuditActorType::Other("robot".into()), None)), "robot");
    }

    #[test]
    fn unknown_actions_are_kept() {
        assert_eq!(AuditAction::from("future_thing".to_string()), AuditAction::Other("future_thing".into()));
        assert_eq!(action_text(&AuditAction::from("future_thing".to_string())), "future_thing");
        assert_eq!(action_text(&AuditAction::from("resend_verification_mail".to_string())), "Verification email sent again");
        assert_eq!(action_text(&AuditAction::PlanInvalidated), "Plan invalidated");
    }

    #[test]
    fn entries_of_the_backend_are_read() {
        let json = r#"{"actor_type":"participant","action":"updated"}"#;
        #[derive(serde::Deserialize)]
        struct Probe { actor_type: AuditActorType, action: AuditAction }
        let probe: Probe = serde_json::from_str(json).unwrap();
        assert_eq!(probe.actor_type, AuditActorType::Participant);
        assert_eq!(probe.action, AuditAction::Updated);
    }
}
