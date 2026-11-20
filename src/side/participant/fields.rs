//! The team fields shared by the registration form and the team page (localized).

use dioxus::prelude::*;

use super::form::{FieldRules, RegisterDraft, RegisterErrors};
use super::lang::Lang;
use crate::ui::*;

/// `rules.allowed` decides which optional fields appear: only what the organizer lets a guest
/// enter. What is not required is marked as optional. `errors` are already filtered for display
/// by the caller.
#[component]
pub fn TeamFormFields(
    draft: Signal<RegisterDraft>,
    errors: RegisterErrors,
    rules: FieldRules,
    lang: Lang,
    #[props(default)] disabled: bool,
    /// Consent to the processing of the dietary information (Art. 9 GDPR). Shown together with the
    /// field when there is something to consent to; the caller decides when that is.
    #[props(default)] consent: Option<Signal<bool>>,
) -> Element {
    let t = lang.txt();
    let current = draft.read().clone();
    let shown = rules.allowed;
    // "(optional)" behind the label of what may stay empty
    let label = |text: &str, required: bool| -> String {
        if required { text.to_string() } else { format!("{text} ({})", t.optional) }
    };

    // Phone and number of members are short: next to each other (3:2) when both are asked, which
    // saves a screen of scrolling on a phone.
    let pair = shown.phone && shown.members;
    let phone_class = if pair { "col-span-3" } else { "col-span-5" };
    let members_class = if pair { "col-span-2" } else { "col-span-5" };

    rsx! {
        div { class: "grid grid-cols-5 items-end gap-x-3 gap-y-4",
            div { class: "col-span-5",
            Field { label: t.lbl_name, error: errors.name.clone(),
                TextInput {
                    value: current.name.clone(),
                    placeholder: t.name_ph,
                    error: !errors.name.is_empty(),
                    disabled,
                    oninput: move |e: Event<FormData>| draft.write().name = e.value(),
                }
            }
            }
            if shown.mail {
                div { class: "col-span-5",
                Field { label: label(t.lbl_email, rules.required.mail), error: errors.mail.clone(),
                    TextInput {
                        kind: InputKind::Email,
                        value: current.mail.clone(),
                        placeholder: t.email_ph,
                        error: !errors.mail.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().mail = e.value(),
                    }
                }
                }
            }
            if shown.phone {
                div { class: "{phone_class}",
                Field { label: label(t.lbl_phone, rules.required.phone), error: errors.phone.clone(),
                    TextInput {
                        kind: InputKind::Tel,
                        value: current.phone.clone(),
                        placeholder: t.phone_ph,
                        error: !errors.phone.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().phone = e.value(),
                    }
                }
                }
            }
            if shown.members {
                div { class: "{members_class}",
                Field { label: label(t.lbl_members, rules.required.members), error: errors.members.clone(),
                    TextInput {
                        kind: InputKind::Number,
                        value: current.members.clone(),
                        placeholder: t.members_ph,
                        error: !errors.members.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().members = e.value(),
                    }
                }
                }
            }
            if shown.diets {
                div { class: "col-span-5 space-y-4",
                Field { label: label(t.lbl_diets, rules.required.diets), error: errors.diets.clone(),
                    TextInput {
                        value: current.diets.clone(),
                        placeholder: t.diets_ph,
                        error: !errors.diets.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().diets = e.value(),
                    }
                }
                Hint { class: "-mt-2", "{t.diets_hint}" }
                if let Some(mut consent) = consent {
                    if !current.diets.trim().is_empty() {
                        Checkbox {
                            boxed: true,
                            label: t.diets_consent,
                            checked: consent(),
                            disabled,
                            onchange: move |checked| consent.set(checked),
                        }
                    }
                }
                }
            }
        }
    }
}
