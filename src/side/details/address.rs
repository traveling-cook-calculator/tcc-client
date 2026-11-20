//! Address input: search via Nominatim ("Automatic") or enter coordinates ("Manual").
//!
//! `AddressParam` holds only what the user typed (3 text signals + "show errors").
//! Errors are *derived* from that text on every render and come from
//! `AddressData::validate` (the old code kept three extra error signals in sync by hand
//! and accepted `NaN`, `inf` and `500` as coordinates).

use dioxus::prelude::*;

use crate::address_connector::{get_address, Feature, NOT_FOUND};
use crate::storage::{AddressData, MAX_ADDRESS_LEN};
use crate::ui::*;

// ─────────────────────────────────────────────
//  Language
// ─────────────────────────────────────────────

/// The language of the component's texts. The organiser's pages are English; the pages of the
/// guests pass the language the visitor chose (`participant::lang::Lang::address`).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) enum AddressLang {
    #[default]
    En,
    De,
}

/// Every word the component shows, in one place. Messages that carry a number are functions.
pub(crate) struct AddressText {
    heading: &'static str,
    tab_automatic: &'static str,
    tab_manual: &'static str,
    search_label: &'static str,
    search_tip: &'static str,
    search_placeholder: &'static str,
    search_button: &'static str,
    nothing_set: &'static str,
    enter_query: &'static str,
    not_found: &'static str,
    needed: &'static str,
    latitude: &'static str,
    longitude: &'static str,
    address: &'static str,
    latitude_ph: &'static str,
    longitude_ph: &'static str,
    address_ph: &'static str,
    coordinate_invalid: &'static str,
    address_missing: &'static str,
    latitude_range: &'static str,
    longitude_range: &'static str,
    address_too_long: fn(usize) -> String,
}

const TEXT_EN: AddressText = AddressText {
    heading: "Address",
    tab_automatic: "Automatic",
    tab_manual: "Manual",
    search_label: "Search address",
    search_tip: "The entered address will be forwarded to Nominatim (OpenStreetMap) for location determination.",
    search_placeholder: "Street, City, ZIP code",
    search_button: "Search",
    nothing_set: "No address set",
    enter_query: "Please enter an address to search for.",
    not_found: "No address found!",
    needed: "An address is needed. Search for it (Automatic) or enter address and coordinates (Manual).",
    latitude: "Latitude",
    longitude: "Longitude",
    address: "Address",
    latitude_ph: "e.g. 50.1127197",
    longitude_ph: "e.g. 8.682092",
    address_ph: "e.g. Main Street 1, 12345 City",
    coordinate_invalid: "Please enter a valid coordinate.",
    address_missing: "Please enter an address.",
    latitude_range: "Latitude must be between -90 and 90.",
    longitude_range: "Longitude must be between -180 and 180.",
    address_too_long: |max| format!("The address may have at most {max} characters."),
};

const TEXT_DE: AddressText = AddressText {
    heading: "Adresse",
    tab_automatic: "Automatisch",
    tab_manual: "Manuell",
    search_label: "Adresse suchen",
    search_tip: "Die eingegebene Adresse wird zur Ortsbestimmung an Nominatim (OpenStreetMap) übermittelt.",
    search_placeholder: "Straße, Ort, PLZ",
    search_button: "Suchen",
    nothing_set: "Noch keine Adresse",
    enter_query: "Bitte gib eine Adresse zum Suchen ein.",
    not_found: "Keine Adresse gefunden!",
    needed: "Eine Adresse ist nötig. Suche sie (Automatisch) oder trage Adresse und Koordinaten ein (Manuell).",
    latitude: "Breitengrad",
    longitude: "Längengrad",
    address: "Adresse",
    latitude_ph: "z. B. 50.1127197",
    longitude_ph: "z. B. 8.682092",
    address_ph: "z. B. Hauptstraße 1, 12345 Stadt",
    coordinate_invalid: "Bitte gib eine gültige Koordinate ein.",
    address_missing: "Bitte gib eine Adresse ein.",
    latitude_range: "Der Breitengrad muss zwischen -90 und 90 liegen.",
    longitude_range: "Der Längengrad muss zwischen -180 und 180 liegen.",
    address_too_long: |max| format!("Die Adresse darf höchstens {max} Zeichen haben."),
};

impl AddressLang {
    fn text(self) -> &'static AddressText {
        match self {
            AddressLang::En => &TEXT_EN,
            AddressLang::De => &TEXT_DE,
        }
    }
}

// ─────────────────────────────────────────────
//  State
// ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AddressParam {
    address: Signal<String>,
    latitude: Signal<String>,
    longitude: Signal<String>,
    /// Set by the first submit attempt: from then on empty fields complain as well.
    show_errors: Signal<bool>,
}

/// One message per field; empty string = valid.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AddressErrors {
    pub address: String,
    pub latitude: String,
    pub longitude: String,
}

impl AddressErrors {
    fn is_empty(&self) -> bool {
        self.address.is_empty() && self.latitude.is_empty() && self.longitude.is_empty()
    }
}

fn parse_coordinate(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|value| value.is_finite())
}

/// What is wrong with each of the three texts, in the visitor's language. The rules are those of
/// `AddressData::validate` (so a form that shows no error always passes the final check).
fn field_errors(address: &str, latitude: &str, longitude: &str, t: &AddressText) -> AddressErrors {
    let address = address.trim();
    AddressErrors {
        address: if address.is_empty() {
            t.address_missing.to_string()
        } else if address.chars().count() > MAX_ADDRESS_LEN {
            (t.address_too_long)(MAX_ADDRESS_LEN)
        } else {
            String::new()
        },
        latitude: match parse_coordinate(latitude) {
            None => t.coordinate_invalid.to_string(),
            Some(value) if !(-90.0..=90.0).contains(&value) => t.latitude_range.to_string(),
            Some(_) => String::new(),
        },
        longitude: match parse_coordinate(longitude) {
            None => t.coordinate_invalid.to_string(),
            Some(value) if !(-180.0..=180.0).contains(&value) => t.longitude_range.to_string(),
            Some(_) => String::new(),
        },
    }
}

/// The address from the three texts, or the first validation message.
fn to_address_data(address: &str, latitude: &str, longitude: &str) -> Result<AddressData, String> {
    let invalid = || TEXT_EN.coordinate_invalid.to_string();
    let data = AddressData {
        address: address.trim().to_string(),
        latitude: parse_coordinate(latitude).ok_or_else(invalid)?,
        longitude: parse_coordinate(longitude).ok_or_else(invalid)?,
    };
    data.validate()?;
    Ok(data)
}

impl AddressParam {
    /// Hooks: call unconditionally in the component body. Pass `AddressData::empty()`
    /// for an empty form (0/0 counts as "not set").
    pub(crate) fn new(initial: &AddressData) -> Self {
        let coordinate = |value: f64| {
            if value.is_finite() && value != 0.0 { value.to_string() } else { String::new() }
        };
        Self {
            address: use_signal(|| initial.address.clone()),
            latitude: use_signal(|| coordinate(initial.latitude)),
            longitude: use_signal(|| coordinate(initial.longitude)),
            show_errors: use_signal(|| false),
        }
    }

    /// Raw input values. Read-only: safe to call while rendering.
    pub(crate) fn snapshot(&self) -> (String, String, String) {
        (self.address.read().clone(), self.latitude.read().clone(), self.longitude.read().clone())
    }

    /// Validation result without side effects (use while rendering).
    pub(crate) fn current(&self) -> Result<AddressData, String> {
        let (address, latitude, longitude) = self.snapshot();
        to_address_data(&address, &latitude, &longitude)
    }

    /// For submit handlers: reveals all errors in the form, then returns the address.
    pub(crate) fn get_address_data(&self) -> Result<AddressData, String> {
        let mut show_errors = self.show_errors;
        show_errors.set(true);
        self.current()
    }

    /// Errors that should be visible now: everything after a submit attempt,
    /// before that only fields the user has already filled in.
    fn visible_errors(&self, t: &AddressText) -> AddressErrors {
        let (address, latitude, longitude) = self.snapshot();
        let all = field_errors(&address, &latitude, &longitude, t);
        if *self.show_errors.read() {
            return all;
        }
        let quiet = |text: &str, error: String| if text.trim().is_empty() { String::new() } else { error };
        AddressErrors {
            address: quiet(&address, all.address),
            latitude: quiet(&latitude, all.latitude),
            longitude: quiet(&longitude, all.longitude),
        }
    }

    /// Something is wrong with the address and the user should see it, whichever way of entering
    /// it (automatic search or manual fields) is open.
    fn has_visible_errors(&self) -> bool {
        // Whether there is an error does not depend on the language.
        !self.visible_errors(&TEXT_EN).is_empty()
    }

    fn fill(&self, found: &Feature) {
        let (mut address, mut latitude, mut longitude) = (self.address, self.latitude, self.longitude);
        address.set(found.formatted());
        latitude.set(found.lat.to_string());
        longitude.set(found.lon.to_string());
    }
}

// ─────────────────────────────────────────────
//  Component
// ─────────────────────────────────────────────

/// Self-contained: renders its own "Address" heading, callers must not add another label.
/// `lang` is the language of all its texts (English for the organiser, the visitor's choice for guests).
#[component]
pub(crate) fn Address(param: AddressParam, #[props(default)] lang: AddressLang) -> Element {
    let mut mode = use_signal(|| 0usize);
    let t = lang.text();

    rsx! {
        div { class: "space-y-3",
            div { class: "flex items-center gap-1.5",
                AddressSVG {}
                Headline3 { headline: t.heading }
            }
            Tabs {
                tabs: vec![t.tab_automatic, t.tab_manual],
                selected: mode(),
                onselect: move |index| mode.set(index),
            }
            if mode() == 0 {
                AutoAddress { param, lang }
            } else {
                ManualAddress { param, lang }
            }
            // One message for both ways of entering the address: the error must not hide in the
            // tab that happens not to be open.
            if param.has_visible_errors() {
                Notice { tone: NoticeTone::Danger, small: true, "{t.needed}" }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Automatic: Nominatim search
// ─────────────────────────────────────────────

#[component]
fn AutoAddress(param: AddressParam, lang: AddressLang) -> Element {
    let t = lang.text();
    let search_task = use_busy();
    let mut query = use_signal(String::new);
    let mut search_error = use_signal(|| None::<String>);

    // Shared by the button and the Enter key.
    let mut on_search = move || {
        let text = query.peek().trim().to_string();
        if text.is_empty() {
            search_error.set(Some(t.enter_query.to_string()));
            return;
        }
        search_error.set(None);
        search_task.run(async move {
            match get_address(&text).await {
                Ok(found) => param.fill(&found),
                Err(e) if e == NOT_FOUND => search_error.set(Some(t.not_found.to_string())),
                Err(e) => search_error.set(Some(e)),
            }
        });
    };

    let current = param.address.read().clone();
    let stored_error = param.visible_errors(t).address;

    rsx! {
        div { class: "space-y-2",
            div { class: "flex items-center justify-between",
                Eyebrow { text: t.search_label }
                InfoTip { text: t.search_tip }
            }

            TextInput {
                value: query(),
                placeholder: t.search_placeholder,
                error: search_error.read().is_some() || !stored_error.is_empty(),
                oninput: move |e: Event<FormData>| {
                    query.set(e.value());
                    search_error.set(None);
                },
                // Enter in the address field starts the search (not the surrounding form's submit).
                onenter: move |_| on_search(),
            }
            Button {
                label: t.search_button,
                variant: ButtonVariant::Secondary,
                loading: search_task.is_busy(),
                onclick: move |_| on_search(),
            }

            div { class: "flex items-start gap-2 text-sm min-h-6",
                if let Some(message) = search_error() {
                    FieldError { error: message }
                } else if !current.is_empty() {
                    CheckSVG {}
                    span { class: "text-zinc-600 leading-snug", "{current}" }
                } else if !stored_error.is_empty() && !current.trim().is_empty() {
                    FieldError { error: stored_error }
                } else {
                    span { class: "text-zinc-400", "{t.nothing_set}" }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Manual: coordinates
// ─────────────────────────────────────────────

#[component]
fn ManualAddress(param: AddressParam, lang: AddressLang) -> Element {
    let t = lang.text();
    let (mut address, mut latitude, mut longitude) = (param.address, param.latitude, param.longitude);
    let errors = param.visible_errors(t);
    let (current_address, current_latitude, current_longitude) = param.snapshot();

    rsx! {
        div { class: "space-y-4",
            Field { label: t.latitude, error: errors.latitude.clone(),
                TextInput {
                    value: current_latitude,
                    placeholder: t.latitude_ph,
                    error: !errors.latitude.is_empty(),
                    oninput: move |e: Event<FormData>| latitude.set(e.value()),
                }
            }
            Field { label: t.longitude, error: errors.longitude.clone(),
                TextInput {
                    value: current_longitude,
                    placeholder: t.longitude_ph,
                    error: !errors.longitude.is_empty(),
                    oninput: move |e: Event<FormData>| longitude.set(e.value()),
                }
            }
            Field { label: t.address, error: errors.address.clone(),
                TextInput {
                    value: current_address,
                    placeholder: t.address_ph,
                    error: !errors.address.is_empty(),
                    oninput: move |e: Event<FormData>| address.set(e.value()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_input_has_no_errors() {
        assert!(field_errors("Main Street 1", "50.1", "8.6", &TEXT_EN).is_empty());
        assert!(to_address_data("Main Street 1", "50.1", "8.6").is_ok());
    }

    #[test]
    fn each_field_reports_its_own_error() {
        let errors = field_errors("", "50.1", "8.6", &TEXT_EN);
        assert!(!errors.address.is_empty() && errors.latitude.is_empty() && errors.longitude.is_empty());
        let errors = field_errors("Main Street 1", "abc", "8.6", &TEXT_EN);
        assert!(errors.address.is_empty() && !errors.latitude.is_empty() && errors.longitude.is_empty());
        let errors = field_errors("Main Street 1", "50.1", "", &TEXT_EN);
        assert!(errors.address.is_empty() && errors.latitude.is_empty() && !errors.longitude.is_empty());
    }

    #[test]
    fn out_of_range_and_non_finite_coordinates_are_rejected() {
        // The old check accepted everything `f64::parse` accepts.
        for bad in ["91", "-91", "NaN", "inf", "-inf", "1e308"] {
            assert!(!field_errors("Main Street 1", bad, "8.6", &TEXT_EN).latitude.is_empty(), "latitude '{bad}'");
            assert!(to_address_data("Main Street 1", bad, "8.6").is_err(), "latitude '{bad}'");
        }
        for bad in ["181", "-181", "NaN"] {
            assert!(!field_errors("Main Street 1", "50.1", bad, &TEXT_EN).longitude.is_empty(), "longitude '{bad}'");
        }
        assert!(to_address_data("Main Street 1", "90", "-180").is_ok());
    }

    #[test]
    fn the_messages_follow_the_language() {
        let en = field_errors("", "abc", "999", &TEXT_EN);
        let de = field_errors("", "abc", "999", &TEXT_DE);
        for (a, b) in [(&en.address, &de.address), (&en.latitude, &de.latitude), (&en.longitude, &de.longitude)] {
            assert!(!a.is_empty() && !b.is_empty() && a != b);
        }
        assert!(de.address.contains("Adresse") && en.address.contains("address"));
        assert!(field_errors(&"x".repeat(501), "1", "1", &TEXT_DE).address.contains("500"));
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        let data = to_address_data("  Main Street 1 ", " 50.1 ", " 8.6 ").unwrap();
        assert_eq!(data.address, "Main Street 1");
        assert_eq!((data.latitude, data.longitude), (50.1, 8.6));
    }
}
