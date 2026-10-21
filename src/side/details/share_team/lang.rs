//! Language and texts of the public registration page.

use chrono::NaiveDateTime;

use crate::time::to_local;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    #[default]
    De,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::De, Lang::En];

    pub fn label(self) -> &'static str {
        match self {
            Lang::De => "DE",
            Lang::En => "EN",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|lang| *lang == self).unwrap_or(0)
    }

    pub fn txt(self) -> &'static Txt {
        match self {
            Lang::De => &DE,
            Lang::En => &EN,
        }
    }

    /// Deadline in the visitor's local time (storage timestamps are UTC).
    pub fn format_deadline(self, utc: NaiveDateTime) -> String {
        let local = to_local(utc);
        match self {
            Lang::De => local.format("%d.%m.%Y, %H:%M Uhr").to_string(),
            Lang::En => local.format("%d.%m.%Y, %H:%M").to_string(),
        }
    }
}

pub struct Txt {
    pub page_title: &'static str,
    pub form_title: &'static str,
    pub lbl_name: &'static str,
    pub name_ph: &'static str,
    pub lbl_email: &'static str,
    pub email_ph: &'static str,
    pub lbl_phone: &'static str,
    pub phone_ph: &'static str,
    pub lbl_members: &'static str,
    pub members_ph: &'static str,
    pub lbl_diets: &'static str,
    pub diets_ph: &'static str,
    pub deadline_until: &'static str,
    pub btn_submit: &'static str,
    pub err_name_empty: &'static str,
    pub err_email_invalid: &'static str,
    pub err_phone_empty: &'static str,
    pub err_phone_invalid: &'static str,
    pub err_members_invalid: &'static str,
    pub err_members_zero: &'static str,
    pub err_diets_invalid: &'static str,
    pub submit_failed_title: &'static str,
    pub success_title: &'static str,
    pub success_body: &'static str,
    pub success_verify: &'static str,
    pub success_review: &'static str,
    pub deadline_passed_title: &'static str,
    pub deadline_passed_body: &'static str,
    pub inactive_title: &'static str,
    pub inactive_body: &'static str,
}

const DE: Txt = Txt {
    page_title: "Team registrieren",
    form_title: "Team-Daten",
    lbl_name: "Teamname",
    name_ph: "z.B. Die Chili-Chasers",
    lbl_email: "E-Mail",
    email_ph: "z.B. chili@chasers.de",
    lbl_phone: "Telefon",
    phone_ph: "z.B. +49 1234 56789",
    lbl_members: "Anzahl Mitglieder",
    members_ph: "z.B. 2",
    lbl_diets: "Ernährungsbesonderheiten",
    diets_ph: "z.B. vegetarisch, Nussallergie, halal …",
    deadline_until: "Anmeldung möglich bis",
    btn_submit: "Team erstellen",
    err_name_empty: "Teamname darf nicht leer sein!",
    err_email_invalid: "Bitte eine gültige E-Mail-Adresse eingeben!",
    err_phone_empty: "Telefonnummer darf nicht leer sein!",
    err_phone_invalid: "Bitte eine gültige Telefonnummer eingeben!",
    err_members_invalid: "Bitte eine gültige Zahl eingeben!",
    err_members_zero: "Anzahl Mitglieder muss größer als 0 sein!",
    err_diets_invalid: "Die Eingabe ist ungültig oder zu lang!",
    submit_failed_title: "Die Anmeldung war nicht möglich",
    success_title: "Team erfolgreich registriert!",
    success_body: "Dein Team wurde angelegt. Wir freuen uns auf dich!",
    success_verify: "Bitte bestätige deine E-Mail-Adresse über den Link in der E-Mail, die wir dir geschickt haben.",
    success_review: "Deine Anmeldung wird noch vom Veranstalter geprüft.",
    deadline_passed_title: "Anmeldung geschlossen",
    deadline_passed_body: "Der Anmeldeschluss ist abgelaufen. Es können keine weiteren Teams registriert werden.",
    inactive_title: "Link nicht aktiv",
    inactive_body: "Dieser Registrierungslink ist aktuell nicht aktiv.",
};

const EN: Txt = Txt {
    page_title: "Register team",
    form_title: "Team data",
    lbl_name: "Team name",
    name_ph: "e.g. The Chili Chasers",
    lbl_email: "Email",
    email_ph: "e.g. chili@chasers.de",
    lbl_phone: "Phone number",
    phone_ph: "e.g. +49 1234 56789",
    lbl_members: "Number of members",
    members_ph: "e.g. 2",
    lbl_diets: "Dietary requirements",
    diets_ph: "e.g. vegetarian, nut allergy, halal …",
    deadline_until: "Registration open until",
    btn_submit: "Create team",
    err_name_empty: "Team name cannot be empty!",
    err_email_invalid: "Please enter a valid email address!",
    err_phone_empty: "Phone number cannot be empty!",
    err_phone_invalid: "Please enter a valid phone number!",
    err_members_invalid: "Please enter a valid number!",
    err_members_zero: "Number of members must be greater than 0!",
    err_diets_invalid: "The input is invalid or too long!",
    submit_failed_title: "The registration was not possible",
    success_title: "Team successfully registered!",
    success_body: "Your team has been created. We look forward to seeing you!",
    success_verify: "Please confirm your email address using the link in the email we sent you.",
    success_review: "Your registration is still being reviewed by the organizer.",
    deadline_passed_title: "Registration closed",
    deadline_passed_body: "The registration deadline has passed. No more teams can be registered.",
    inactive_title: "Link not active",
    inactive_body: "This registration link is currently not active.",
};
