//! The team fields shared by the registration form and the team page (localized).

use dioxus::prelude::*;

use super::form::{RegisterDraft, RegisterErrors, VisibleFields};
use super::lang::Lang;
use crate::ui::*;

/// `shown` decides which optional fields appear (registration: only what the organizer asks
/// for; team page: everything). `errors` are already filtered for display by the caller.
#[component]
pub fn TeamFormFields(
    draft: Signal<RegisterDraft>,
    errors: RegisterErrors,
    shown: VisibleFields,
    lang: Lang,
    #[props(default)] disabled: bool,
) -> Element {
    let t = lang.txt();
    let current = draft.read().clone();

    rsx! {
        div { class: "space-y-4",
            Field { label: t.lbl_name, error: errors.name.clone(),
                TextInput {
                    value: current.name.clone(),
                    placeholder: t.name_ph,
                    error: !errors.name.is_empty(),
                    disabled,
                    oninput: move |e: Event<FormData>| draft.write().name = e.value(),
                }
            }
            if shown.mail {
                Field { label: t.lbl_email, error: errors.mail.clone(),
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
            if shown.phone {
                Field { label: t.lbl_phone, error: errors.phone.clone(),
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
            if shown.members {
                Field { label: t.lbl_members, error: errors.members.clone(),
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
            if shown.diets {
                Field { label: t.lbl_diets, error: errors.diets.clone(),
                    TextInput {
                        value: current.diets.clone(),
                        placeholder: t.diets_ph,
                        error: !errors.diets.is_empty(),
                        disabled,
                        oninput: move |e: Event<FormData>| draft.write().diets = e.value(),
                    }
                }
            }
        }
    }
}
