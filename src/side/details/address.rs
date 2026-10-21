//! Address input: search via Nominatim ("Automatic") or enter coordinates ("Manual").
//!
//! `AddressParam` holds only what the user typed (3 text signals + "show errors").
//! Errors are *derived* from that text on every render and come from
//! `AddressData::validate` (the old code kept three extra error signals in sync by hand
//! and accepted `NaN`, `inf` and `500` as coordinates).

use dioxus::prelude::*;

use crate::address_connector::{get_address, Feature, NOT_FOUND};
use crate::storage::AddressData;
use crate::ui::*;

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

#[cfg(test)]
impl AddressErrors {
    fn is_empty(&self) -> bool {
        self.address.is_empty() && self.latitude.is_empty() && self.longitude.is_empty()
    }
}

fn parse_coordinate(text: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .map_err(|_| "Please enter a valid coordinate.".to_string())
}

/// Checks the fields one by one against an otherwise valid address.
fn field_errors(address: &str, latitude: &str, longitude: &str) -> AddressErrors {
    let probe = |change: &dyn Fn(&mut AddressData)| {
        let mut data = AddressData {
            address: "Street 1, 12345 City".to_string(),
            latitude: 50.0,
            longitude: 8.0,
        };
        change(&mut data);
        data.validate().err().unwrap_or_default()
    };
    AddressErrors {
        address: probe(&|data| data.address = address.trim().to_string()),
        latitude: match parse_coordinate(latitude) {
            Ok(value) => probe(&|data| data.latitude = value),
            Err(message) => message,
        },
        longitude: match parse_coordinate(longitude) {
            Ok(value) => probe(&|data| data.longitude = value),
            Err(message) => message,
        },
    }
}

/// The address from the three texts, or the first validation message.
fn to_address_data(address: &str, latitude: &str, longitude: &str) -> Result<AddressData, String> {
    let data = AddressData {
        address: address.trim().to_string(),
        latitude: parse_coordinate(latitude)?,
        longitude: parse_coordinate(longitude)?,
    };
    data.validate()?;
    Ok(data)
}

impl AddressParam {
    /// Hooks: call unconditionally in the component body. Pass `AddressData::default()`
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
    fn visible_errors(&self) -> AddressErrors {
        let (address, latitude, longitude) = self.snapshot();
        let all = field_errors(&address, &latitude, &longitude);
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
#[component]
pub(crate) fn Address(param: AddressParam) -> Element {
    let mut mode = use_signal(|| 0usize);

    rsx! {
        div { class: "space-y-3",
            div { class: "flex items-center gap-1.5",
                AddressSVG {}
                Headline3 { headline: "Address" }
            }
            Tabs {
                tabs: vec!["Automatic", "Manual"],
                selected: mode(),
                onselect: move |index| mode.set(index),
            }
            if mode() == 0 {
                AutoAddress { param }
            } else {
                ManualAddress { param }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Automatic: Nominatim search
// ─────────────────────────────────────────────

#[component]
fn AutoAddress(param: AddressParam) -> Element {
    let search_task = use_busy();
    let mut query = use_signal(String::new);
    let mut search_error = use_signal(|| None::<String>);

    let on_search = move |_| {
        let text = query.peek().trim().to_string();
        if text.is_empty() {
            search_error.set(Some("Please enter an address to search for.".to_string()));
            return;
        }
        search_error.set(None);
        search_task.run(async move {
            match get_address(&text).await {
                Ok(found) => param.fill(&found),
                Err(e) if e == NOT_FOUND => search_error.set(Some("No address found!".to_string())),
                Err(e) => search_error.set(Some(e)),
            }
        });
    };

    let current = param.address.read().clone();
    let stored_error = param.visible_errors().address;

    rsx! {
        div { class: "space-y-2",
            div { class: "flex items-center justify-between",
                span { class: "text-[11px] font-semibold tracking-[0.12em] uppercase text-amber-700/70",
                    "Search Address"
                }
                span { class: "relative group cursor-help",
                    InfoSVG {}
                    span { class: "absolute bottom-full right-0 mb-1.5 hidden group-hover:block \
                                   bg-zinc-800 text-white text-xs rounded-xl \
                                   px-3 py-2 w-64 z-10 shadow-lg leading-relaxed",
                        "The entered address will be forwarded to Nominatim (OpenStreetMap) for location determination."
                    }
                }
            }

            TextInput {
                value: query(),
                placeholder: "Street, City, ZIP code",
                error: search_error.read().is_some(),
                oninput: move |e: Event<FormData>| {
                    query.set(e.value());
                    search_error.set(None);
                },
            }
            Button {
                label: "Search",
                variant: ButtonVariant::Secondary,
                loading: search_task.is_busy(),
                onclick: on_search,
            }

            div { class: "flex items-start gap-2 text-sm min-h-6",
                if let Some(message) = search_error() {
                    FieldError { error: message }
                } else if !current.is_empty() {
                    CheckSVG {}
                    span { class: "text-zinc-600 leading-snug", "{current}" }
                } else if !stored_error.is_empty() {
                    FieldError { error: stored_error }
                } else {
                    span { class: "text-zinc-400", "No address set" }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Manual: coordinates
// ─────────────────────────────────────────────

#[component]
fn ManualAddress(param: AddressParam) -> Element {
    let (mut address, mut latitude, mut longitude) = (param.address, param.latitude, param.longitude);
    let errors = param.visible_errors();
    let (current_address, current_latitude, current_longitude) = param.snapshot();

    rsx! {
        div { class: "space-y-4",
            Field { label: "Latitude", error: errors.latitude.clone(),
                TextInput {
                    value: current_latitude,
                    placeholder: "e.g. 50.1127197",
                    error: !errors.latitude.is_empty(),
                    oninput: move |e: Event<FormData>| latitude.set(e.value()),
                }
            }
            Field { label: "Longitude", error: errors.longitude.clone(),
                TextInput {
                    value: current_longitude,
                    placeholder: "e.g. 8.682092",
                    error: !errors.longitude.is_empty(),
                    oninput: move |e: Event<FormData>| longitude.set(e.value()),
                }
            }
            Field { label: "Address", error: errors.address.clone(),
                TextInput {
                    value: current_address,
                    placeholder: "e.g. Main Street 1, 12345 City",
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
        assert!(field_errors("Main Street 1", "50.1", "8.6").is_empty());
        assert!(to_address_data("Main Street 1", "50.1", "8.6").is_ok());
    }

    #[test]
    fn each_field_reports_its_own_error() {
        let errors = field_errors("", "50.1", "8.6");
        assert!(!errors.address.is_empty() && errors.latitude.is_empty() && errors.longitude.is_empty());
        let errors = field_errors("Main Street 1", "abc", "8.6");
        assert!(errors.address.is_empty() && !errors.latitude.is_empty() && errors.longitude.is_empty());
        let errors = field_errors("Main Street 1", "50.1", "");
        assert!(errors.address.is_empty() && errors.latitude.is_empty() && !errors.longitude.is_empty());
    }

    #[test]
    fn out_of_range_and_non_finite_coordinates_are_rejected() {
        // The old check accepted everything `f64::parse` accepts.
        for bad in ["91", "-91", "NaN", "inf", "-inf", "1e308"] {
            assert!(!field_errors("Main Street 1", bad, "8.6").latitude.is_empty(), "latitude '{bad}'");
            assert!(to_address_data("Main Street 1", bad, "8.6").is_err(), "latitude '{bad}'");
        }
        for bad in ["181", "-181", "NaN"] {
            assert!(!field_errors("Main Street 1", "50.1", bad).longitude.is_empty(), "longitude '{bad}'");
        }
        assert!(to_address_data("Main Street 1", "90", "-180").is_ok());
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        let data = to_address_data("  Main Street 1 ", " 50.1 ", " 8.6 ").unwrap();
        assert_eq!(data.address, "Main Street 1");
        assert_eq!((data.latitude, data.longitude), (50.1, 8.6));
    }
}
