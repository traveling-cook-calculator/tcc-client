//! The team form shared by "Add team" and "Edit team".

use dioxus::prelude::*;

use super::draft::TeamDraft;
use crate::side::details::address::{Address, AddressParam};
use crate::ui::*;

/// `show_all_errors`: an empty, untouched name does not complain until the user tries to submit.
#[component]
pub fn TeamFields(
    draft: Signal<TeamDraft>,
    address: AddressParam,
    #[props(default)] show_all_errors: bool,
    #[props(default)] disabled: bool,
) -> Element {
    let current = draft.read().clone();
    let errors = current.errors();
    let name_error = if current.name.is_empty() && !show_all_errors { String::new() } else { errors.name };
    let dim = if disabled { "pointer-events-none opacity-60" } else { "" };

    rsx! {
        div { class: "flex flex-col md:flex-row gap-6",
            div { class: "flex-1 space-y-4 md:border-r border-amber-100 md:pr-5",
                Field { label: "Team Name", error: name_error.clone(),
                    TextInput {
                        value: current.name.clone(),
                        placeholder: "e.g. The Chili Chasers",
                        error: !name_error.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().name = e.value(),
                    }
                }
                Field { label: "Email", error: errors.mail.clone(),
                    TextInput {
                        kind: InputKind::Email,
                        value: current.mail.clone(),
                        placeholder: "e.g. chili@chasers.de",
                        error: !errors.mail.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().mail = e.value(),
                    }
                }
                Field { label: "Phone Number", error: errors.phone.clone(),
                    TextInput {
                        kind: InputKind::Tel,
                        value: current.phone.clone(),
                        placeholder: "e.g. +49 1234 56789",
                        error: !errors.phone.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().phone = e.value(),
                    }
                }
                Field { label: "Number of Members", error: errors.members.clone(),
                    TextInput {
                        kind: InputKind::Number,
                        value: current.members.clone(),
                        placeholder: "e.g. 2",
                        error: !errors.members.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().members = e.value(),
                    }
                }
                Field { label: "Dietary Requirements", error: errors.diets.clone(),
                    TextInput {
                        value: current.diets.clone(),
                        placeholder: "e.g. vegetarian, nut allergy, halal ...",
                        error: !errors.diets.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().diets = e.value(),
                    }
                }
            }

            // `Address` renders its own heading.
            div { class: "flex-1 {dim}", Address { param: address } }
        }
    }
}
