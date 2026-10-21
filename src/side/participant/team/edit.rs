//! Edit card, cancel card and email verification of a team (all through the team's token).

use dioxus::prelude::*;
use uuid::Uuid;

use super::super::fields::TeamFormFields;
use super::super::form::{RegisterDraft, VisibleFields};
use super::super::lang::Lang;
use crate::data::Storage;
use crate::side::details::address::{Address, AddressParam};
use crate::storage::{validate_cancel_reason, ShareTeamConfig, TeamAccess, TeamData};
use crate::ui::*;

// ─────────────────────────────────────────────
//  Edit
// ─────────────────────────────────────────────

#[component]
pub fn TeamEditCard(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    team: TeamData,
    config: Option<ShareTeamConfig>,
    read_only: bool,
    reviews_changes: bool,
    lang: Lang,
) -> Element {
    let t = lang.txt();
    let toasts = use_toasts();
    let save_task = use_busy();

    let draft = use_signal(|| RegisterDraft::from_team(&team));
    let address = AddressParam::new(&team.address);
    // What is stored. After a successful save it becomes the new reference ("unsaved changes" is
    // everything that differs from it).
    let mut saved = use_signal(|| RegisterDraft::from_team(&team));
    let initial_address = use_hook(|| address.snapshot());
    let mut saved_address = use_signal(move || initial_address);

    let required = config
        .as_ref()
        .map(|config| VisibleFields::from_config(&config.required_fields))
        .unwrap_or_default();
    let current = draft.read().clone();
    let errors = current.edit_errors(required, t);
    let dirty = current != *saved.read() || address.snapshot() != *saved_address.read();
    let can_save = !read_only && dirty && errors.is_empty();
    // Keeps the current status; the backend applies the organizer's review rules on top.
    let needs_check = team.needs_check();
    let all_fields = VisibleFields { mail: true, phone: true, members: true, diets: true };

    let on_save = move |_| {
        let current = draft.peek().clone();
        let Ok(address_data) = address.get_address_data() else { return };
        let update = match current.to_update(address_data, needs_check) {
            Ok(update) => update,
            Err(e) => return toasts.error(t.save_failed_title, e),
        };
        save_task.run(async move {
            let result = {
                storage
                    .read()
                    .await
                    .update_team_with_access(cook_and_run_id, team_id, &TeamAccess::Token(access_token), &update)
                    .await
            };
            match result {
                Ok(()) => {
                    saved.set(current);
                    saved_address.set(address.snapshot());
                    toasts.success(t.saved_title, t.saved_body);
                    storage.invalidate();
                }
                Err(e) => toasts.error(t.save_failed_title, e),
            }
        });
    };

    rsx! {
        Card { title: t.tab_team, class: "max-sm:rounded-none max-sm:border-x-0",
            if reviews_changes && !read_only {
                p { class: "text-xs text-amber-700 bg-amber-50 border border-amber-200 rounded-xl px-3 py-2",
                    "{t.review_notice}"
                }
            }
            div { class: "flex flex-col md:flex-row gap-6",
                div { class: "flex-1 md:border-r border-amber-100 md:pr-6",
                    TeamFormFields { draft, errors, shown: all_fields, lang, disabled: read_only }
                }
                div { class: if read_only { "flex-1 pointer-events-none opacity-60" } else { "flex-1" },
                    Address { param: address }
                }
            }
            if !read_only {
                div { class: "pt-1 border-t border-amber-100 flex",
                    Button { label: t.btn_save, loading: save_task.is_busy(), disabled: !can_save, onclick: on_save }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Cancel
// ─────────────────────────────────────────────

#[component]
pub fn CancelCard(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    lang: Lang,
) -> Element {
    let t = lang.txt();
    let toasts = use_toasts();
    let cancel_task = use_busy();
    let mut reason = use_signal(String::new);
    let mut confirm = use_signal(|| false);

    let reason_text = reason.read().trim().to_string();
    let reason_value = (!reason_text.is_empty()).then(|| reason_text.clone());
    // The rule comes from the storage layer; the page only translates the context.
    let reason_error = validate_cancel_reason(&reason_value).err().unwrap_or_default();

    let on_cancel = move |_| {
        let reason = reason_value.clone();
        cancel_task.run(async move {
            let result = {
                storage.read().await.cancel_team(cook_and_run_id, team_id, access_token, reason).await
            };
            match result {
                Ok(()) => storage.invalidate(),
                Err(e) => toasts.error(t.cancel_failed_title, e),
            }
        });
    };

    rsx! {
        Card { title: t.cancel_title, tone: Tone::Danger, class: "max-sm:rounded-none max-sm:border-x-0",
            p { class: "text-sm text-zinc-600 leading-relaxed", "{t.cancel_body}" }
            Field { label: t.lbl_cancel_reason, error: reason_error.clone(),
                TextArea {
                    value: reason(),
                    placeholder: t.cancel_reason_ph,
                    rows: 2,
                    error: !reason_error.is_empty(),
                    oninput: move |e: Event<FormData>| reason.set(e.value()),
                }
            }
            if confirm() {
                div { class: "flex flex-wrap gap-3",
                    Button {
                        label: t.btn_cancel_confirm,
                        variant: ButtonVariant::Danger,
                        loading: cancel_task.is_busy(),
                        disabled: !reason_error.is_empty(),
                        onclick: on_cancel,
                    }
                    Button {
                        label: t.btn_back,
                        variant: ButtonVariant::Secondary,
                        disabled: cancel_task.is_busy(),
                        onclick: move |_| confirm.set(false),
                    }
                }
            } else {
                div {
                    Button { label: t.btn_cancel, variant: ButtonVariant::DangerOutline, onclick: move |_| confirm.set(true) }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Email verification
// ─────────────────────────────────────────────

#[component]
pub fn VerifyNotice(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    lang: Lang,
    /// Set when the page was opened through the link in the verification mail.
    auto_verify: bool,
) -> Element {
    let t = lang.txt();
    let toasts = use_toasts();
    let verify_task = use_busy();
    let resend_task = use_busy();

    let verify = move || {
        verify_task.run(async move {
            let result = { storage.read().await.verify_team_email(cook_and_run_id, team_id, access_token).await };
            match result {
                Ok(()) => {
                    toasts.success(t.verify_done, "");
                    storage.invalidate();
                }
                Err(e) => toasts.error(t.verify_failed_title, e),
            }
        });
    };

    // Opened through the mail link: confirm right away (once, when the notice appears).
    use_hook(move || {
        if auto_verify {
            verify();
        }
    });

    let on_resend = move |_| {
        resend_task.run(async move {
            let result = {
                storage
                    .read()
                    .await
                    .resend_team_verification(cook_and_run_id, team_id, &TeamAccess::Token(access_token))
                    .await
            };
            match result {
                Ok(()) => toasts.success(t.resent_title, t.resent_body),
                Err(e) => toasts.error(t.verify_failed_title, e),
            }
        });
    };

    rsx! {
        Card { title: t.verify_title, class: "max-sm:rounded-none max-sm:border-x-0",
            p { class: "text-sm text-zinc-600 leading-relaxed", "{t.verify_body}" }
            div { class: "flex flex-wrap gap-3",
                Button { label: t.btn_verify, loading: verify_task.is_busy(), onclick: move |_| verify() }
                Button {
                    label: t.btn_resend,
                    variant: ButtonVariant::Secondary,
                    loading: resend_task.is_busy(),
                    onclick: on_resend,
                }
            }
        }
    }
}
