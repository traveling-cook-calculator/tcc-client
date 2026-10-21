//! Public team registration, reachable through the share link `/cook-and-run/{id}/share`.
//! No login is needed (neither for viewing nor for registering).
//!
//! | file       | contents                                                  |
//! |------------|-----------------------------------------------------------|
//! | `mod.rs`   | page, loading, registration form                          |
//! | `form.rs`  | form model + validation (pure, tested)                    |
//! | `lang.rs`  | German/English texts                                      |
//! | `views.rs` | inactive / closed / success states                        |
//!
//! Participants use their **own** `Storage` instance with an anonymous cloud connection.
//! The app-wide connection requires a login (`CloudStorage::new` checks the session), and
//! sharing one connection would let the login state of the app overwrite the anonymous one.

mod form;
mod lang;
mod views;

use chrono::Utc;
use dioxus::prelude::*;
use uuid::Uuid;

use super::address::{Address, AddressParam};
use crate::config::Config;
use crate::data::Storage;
use crate::storage::{AddressData, ShareTeamConfig, StorageManager};
use crate::ui::*;
use form::{RegisterDraft, VisibleFields};
use lang::Lang;
use views::{ClockSVG, ClosedView, InactiveView, SuccessView};

#[component]
pub fn ShareRegisterPage(cook_and_run_id: ReadSignal<Uuid>) -> Element {
    let config = use_context::<Signal<Option<Config>>>();
    let participant = use_hook(|| StorageManager::new().map(Storage::new));
    let mut lang = use_signal(Lang::default);
    let handle = participant.clone().ok();

    let share_config = use_resource(move || {
        let id = cook_and_run_id();
        let participant = participant.clone();
        async move {
            let storage = participant?;
            let base_url = config
                .peek()
                .as_ref()
                .map(|config| config.auth.audience.clone())
                .ok_or_else(|| "The configuration is not loaded.".to_string())?;
            storage.connect_cloud_anonymous(base_url).await?;
            storage.read().await.select_cook_and_run_share_config(id).await
        }
    });

    let id = cook_and_run_id();
    let storage = handle;
    let current = lang();

    rsx! {
        div { class: "flex-1 min-w-0 w-full sm:max-w-2xl sm:mx-auto flex flex-col sm:py-10 sm:px-4 gap-4 sm:gap-5",
            div { class: "px-4 sm:px-0 pt-4 sm:pt-0",
                PageHeader {
                    title: current.txt().page_title,
                    actions: rsx! {
                        SegmentedTabs {
                            options: Lang::ALL.iter().map(|lang| lang.label()).collect::<Vec<_>>(),
                            selected: current.index(),
                            onselect: move |index: usize| lang.set(Lang::ALL[index]),
                        }
                    },
                }
            }

            {
                render_resource(share_config, "Could not load the registration page.", move |config| {
                    match (config, storage) {
                        (Some(config), Some(storage)) => rsx! {
                            RegisterContent { key: "{id}", cook_and_run_id: id, storage, config, lang: current }
                        },
                        _ => rsx! { InactiveView { lang: current } },
                    }
                })
            }
        }
    }
}

#[component]
fn RegisterContent(
    cook_and_run_id: Uuid,
    storage: Storage,
    config: ShareTeamConfig,
    lang: Lang,
) -> Element {
    // Captured once; the clock must not be read in the render body.
    let now = use_hook(|| Utc::now().naive_utc());
    let closed = config.registration_deadline.filter(|deadline| *deadline < now);

    rsx! {
        if !config.invite_text.is_empty() {
            div { class: "mx-4 sm:mx-0 rounded-2xl border border-amber-200 bg-amber-50/70 px-4 py-3.5 sm:px-5 sm:py-4",
                p { class: "text-sm text-amber-900 leading-relaxed whitespace-pre-wrap", "{config.invite_text}" }
            }
        }
        if let Some(deadline) = closed {
            ClosedView { lang, deadline }
        } else {
            RegisterForm { cook_and_run_id, storage, config, lang }
        }
    }
}

// ─────────────────────────────────────────────
//  Registration form
//
//  Mobile: one column, borderless card. From `md`: fields | address side by side.
// ─────────────────────────────────────────────

#[component]
fn RegisterForm(
    cook_and_run_id: Uuid,
    storage: Storage,
    config: ShareTeamConfig,
    lang: Lang,
) -> Element {
    let mut draft = use_signal(RegisterDraft::default);
    let address = AddressParam::new(&AddressData::default());
    let mut attempted = use_signal(|| false);
    let mut submitted = use_signal(|| false);
    let mut submit_error = use_signal(|| None::<String>);
    let submit_task = use_busy();

    let t = lang.txt();
    let visible = VisibleFields::from_config(&config.required_fields);
    let current = draft.read().clone();

    if submitted() {
        return rsx! {
            SuccessView {
                lang,
                verify_email: config.require_email_verification && visible.mail && !current.mail.trim().is_empty(),
                in_review: config.default_needs_check,
            }
        };
    }

    let errors = current.errors(visible, t);
    // An empty field stays quiet until the first submit attempt.
    let shown = |text: &str, error: &str| -> String {
        if text.trim().is_empty() && !attempted() { String::new() } else { error.to_string() }
    };
    let name_error = shown(&current.name, &errors.name);
    let mail_error = shown(&current.mail, &errors.mail);
    let phone_error = shown(&current.phone, &errors.phone);
    let members_error = shown(&current.members, &errors.members);
    let diets_error = errors.diets.clone();

    let on_submit = move |_| {
        attempted.set(true);
        submit_error.set(None);
        let current = draft.peek().clone();
        if !current.errors(visible, t).is_empty() {
            return;
        }
        // Also reveals what is wrong in the address form.
        let Ok(address_data) = address.get_address_data() else { return };
        let registration = match current.to_registration(visible, address_data, Uuid::new_v4()) {
            Ok(registration) => registration,
            Err(e) => return submit_error.set(Some(e)),
        };

        submit_task.run(async move {
            // Anonymous operation on the participant's own storage; a shared guard is enough.
            let result = {
                storage
                    .read()
                    .await
                    .register_team_as_participant(cook_and_run_id, Uuid::new_v4(), &registration)
                    .await
            };
            match result {
                Ok(()) => submitted.set(true),
                // The server decides about deadline, team limit and rules; it must not be silent.
                Err(e) => submit_error.set(Some(e)),
            }
        });
    };

    rsx! {
        Card { title: t.form_title, class: "max-sm:rounded-none max-sm:border-x-0",
            if let Some(deadline) = config.registration_deadline {
                div { class: "flex items-center gap-2.5 rounded-xl border border-amber-300 bg-amber-50 px-3.5 py-2.5",
                    ClockSVG { class: "w-4 h-4 text-amber-600 shrink-0" }
                    p { class: "text-xs font-semibold text-amber-700",
                        "{t.deadline_until}: {lang.format_deadline(deadline)}"
                    }
                }
            }

            div { class: "flex flex-col md:flex-row gap-6",
                div { class: "flex-1 space-y-4 md:border-r border-amber-100 md:pr-6",
                    Field { label: t.lbl_name, error: name_error.clone(),
                        TextInput {
                            value: current.name.clone(),
                            placeholder: t.name_ph,
                            error: !name_error.is_empty(),
                            oninput: move |e: Event<FormData>| draft.write().name = e.value(),
                        }
                    }
                    if visible.mail {
                        Field { label: t.lbl_email, error: mail_error.clone(),
                            TextInput {
                                kind: InputKind::Email,
                                value: current.mail.clone(),
                                placeholder: t.email_ph,
                                error: !mail_error.is_empty(),
                                oninput: move |e: Event<FormData>| draft.write().mail = e.value(),
                            }
                        }
                    }
                    if visible.phone {
                        Field { label: t.lbl_phone, error: phone_error.clone(),
                            TextInput {
                                kind: InputKind::Tel,
                                value: current.phone.clone(),
                                placeholder: t.phone_ph,
                                error: !phone_error.is_empty(),
                                oninput: move |e: Event<FormData>| draft.write().phone = e.value(),
                            }
                        }
                    }
                    if visible.members {
                        Field { label: t.lbl_members, error: members_error.clone(),
                            TextInput {
                                kind: InputKind::Number,
                                value: current.members.clone(),
                                placeholder: t.members_ph,
                                error: !members_error.is_empty(),
                                oninput: move |e: Event<FormData>| draft.write().members = e.value(),
                            }
                        }
                    }
                    if visible.diets {
                        Field { label: t.lbl_diets, error: diets_error.clone(),
                            TextInput {
                                value: current.diets.clone(),
                                placeholder: t.diets_ph,
                                error: !diets_error.is_empty(),
                                oninput: move |e: Event<FormData>| draft.write().diets = e.value(),
                            }
                        }
                    }
                }

                div { class: "flex-1", Address { param: address } }
            }

            if let Some(message) = submit_error() {
                div { role: "alert", class: "rounded-xl border border-red-200 bg-red-50 px-4 py-3 space-y-0.5",
                    p { class: "text-sm font-semibold text-red-700", "{t.submit_failed_title}" }
                    p { class: "text-xs text-red-600 break-words", "{message}" }
                }
            }

            div { class: "pt-1 border-t border-amber-100 flex",
                Button { label: t.btn_submit, loading: submit_task.is_busy(), onclick: on_submit }
            }
        }
    }
}
