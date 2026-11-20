//! Public team registration, reachable through the share link `/cook-and-run/{id}/share`.
//! No login is needed (neither for viewing nor for registering).

use chrono::Utc;
use dioxus::prelude::*;
use uuid::Uuid;

use super::fields::TeamFormFields;
use super::form::{FieldRules, RegisterDraft, RegisterErrors};
use super::lang::Lang;
use super::views::{ClockSVG, ClosedView, FullView, InactiveView, PrivacyNotice, SuccessView};
use super::{team_url, use_participant};
use crate::data::Storage;
use crate::side::details::address::{Address, AddressParam};
use crate::storage::{AddressData, ShareTeamConfig, TeamAccess};
use crate::ui::*;

#[component]
pub fn ShareRegisterPage(cook_and_run_id: ReadSignal<Uuid>) -> Element {
    let participant = use_participant();
    let mut lang = use_signal(Lang::default);
    let handle = participant.storage();

    let share_config = use_resource(move || {
        let id = cook_and_run_id();
        let participant = participant.clone();
        async move {
            let storage = participant.connect().await?;
            // Guests have no login: the public endpoint, not the organizer's one.
            let config = storage.read().await.select_public_share_config(id).await;
            config
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
        } else if config.registration_full {
            // The server tells (with the configuration) that every place is taken: no form, no
            // request that would only be refused.
            FullView { lang }
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
    let draft = use_signal(RegisterDraft::default);
    let address = AddressParam::new(&AddressData::empty());
    let mut attempted = use_signal(|| false);
    let consent = use_signal(|| false);
    let mut submitted = use_signal(|| false);
    // Closed while the page was open (the deadline is checked again when sending).
    let mut closed_now = use_signal(|| None::<chrono::NaiveDateTime>);
    // The team's id and its access token are chosen here, once. They stay the same for every
    // attempt: if the answer of the server gets lost, the team may exist anyway, and the link
    // built from these two is the way to it (see `on_submit`).
    let (team_id, access_token) = use_hook(|| (Uuid::new_v4(), Uuid::new_v4()));
    // Invisible bot trap: people never see it, bots fill it.
    let mut honeypot = use_signal(String::new);
    let mut submit_error = use_signal(|| None::<String>);
    let submit_task = use_busy();

    let t = lang.txt();
    let rules = FieldRules::from_config(&config);
    let visible = rules.allowed;
    let current = draft.read().clone();

    if let Some(deadline) = closed_now() {
        return rsx! { ClosedView { lang, deadline } };
    }
    if submitted() {
        let has_mail = visible.mail && !current.mail.trim().is_empty();
        return rsx! {
            SuccessView {
                lang,
                verify_email: has_mail,
                in_review: config.default_needs_check,
                has_mail,
                team_link: team_url(cook_and_run_id, team_id, access_token),
            }
        };
    }

    let errors = current.errors(rules, t);
    // An empty field stays quiet until the first submit attempt.
    let shown = |text: &str, error: &str| -> String {
        if text.trim().is_empty() && !attempted() { String::new() } else { error.to_string() }
    };
    let name_error = shown(&current.name, &errors.name);
    let mail_error = shown(&current.mail, &errors.mail);
    let phone_error = shown(&current.phone, &errors.phone);
    let members_error = shown(&current.members, &errors.members);
    // Dietary information may be special-category data: it is only stored with express consent.
    let needs_consent = visible.diets && !current.diets.trim().is_empty() && !consent();
    // Same rule as every other field: quiet while empty, loud after the first attempt.
    let diets_error = if !shown(&current.diets, &errors.diets).is_empty() {
        errors.diets.clone()
    } else if needs_consent && attempted() {
        t.err_diets_consent.to_string()
    } else {
        String::new()
    };

    let on_submit = move |_| {
        attempted.set(true);
        submit_error.set(None);
        let current = draft.peek().clone();
        // Reveal the errors of the address in every case (not only when the rest is fine).
        let address_result = address.get_address_data();
        if !current.errors(rules, t).is_empty() || address_result.is_err() {
            return;
        }
        // The deadline may have passed while the form was open: say so instead of sending.
        if let Some(deadline) = config.registration_deadline.filter(|deadline| *deadline < Utc::now().naive_utc()) {
            return closed_now.set(Some(deadline));
        }
        if visible.diets && !current.diets.trim().is_empty() && !*consent.peek() {
            return;
        }
        let Ok(address_data) = address_result else { return };
        let registration = match current.to_registration(visible, address_data, access_token, &honeypot.peek()) {
            Ok(registration) => registration,
            Err(e) => return submit_error.set(Some(e)),
        };

        submit_task.run(async move {
            // Anonymous operation on the participant's own storage; a shared guard is enough.
            let result = {
                storage
                    .read()
                    .await
                    .register_team_as_participant(cook_and_run_id, team_id, &registration)
                    .await
            };
            match result {
                Ok(()) => submitted.set(true),
                Err(e) => {
                    // A dropped connection can hide a success: if the team is there under the
                    // token chosen above, the registration worked.
                    let exists = {
                        storage
                            .read()
                            .await
                            .select_team_with_access(cook_and_run_id, team_id, &TeamAccess::Token(access_token))
                            .await
                            .is_ok()
                    };
                    if exists {
                        submitted.set(true);
                    } else {
                        // The server decides about deadline, team limit and rules; it must not be silent.
                        submit_error.set(Some(e));
                    }
                }
            }
        });
    };

    rsx! {
        Card { title: t.form_title, class: "max-sm:rounded-none max-sm:border-x-0",
            if let Some(deadline) = config.registration_deadline {
                Notice { tone: NoticeTone::Attention, small: true, class: "flex items-center gap-2.5",
                    ClockSVG { class: "w-4 h-4 shrink-0" }
                    span { class: "font-semibold", "{t.deadline_until}: {lang.format_deadline(deadline)}" }
                }
            }

            div { class: "flex flex-col md:flex-row gap-6",
                div { class: "flex-1 md:border-r border-amber-100 md:pr-6",
                    TeamFormFields {
                        draft,
                        errors: RegisterErrors {
                            name: name_error,
                            mail: mail_error,
                            phone: phone_error,
                            members: members_error,
                            diets: diets_error,
                        },
                        rules,
                        lang,
                        consent: Some(consent),
                    }
                    div {
                        class: "absolute -left-[10000px] top-auto w-px h-px overflow-hidden",
                        "aria-hidden": "true",
                        label { r#for: "website", "Website" }
                        input {
                            id: "website",
                            name: "website",
                            r#type: "text",
                            tabindex: "-1",
                            autocomplete: "off",
                            value: "{honeypot}",
                            oninput: move |e| honeypot.set(e.value()),
                        }
                    }
                }

                div { class: "flex-1", Address { param: address, lang: lang.address() } }
            }

            if let Some(message) = submit_error() {
                Notice { tone: NoticeTone::Danger, roomy: true, title: t.submit_failed_title,
                    p { class: "text-xs break-words", "{message}" }
                }
            }

            PrivacyNotice { lang }

            div { class: "pt-1 border-t border-amber-100 flex",
                Button { label: t.btn_submit, loading: submit_task.is_busy(), onclick: on_submit }
            }
        }
    }
}
