//! The cards of a team's page that change something: the details, the cancellation, and the two
//! that help to keep access (own link, data download). All of it goes through the team's token.
//!
//! What the organiser allows decides what is shown (`FieldRules`), and a lock (canceled, edit
//! deadline over, address not confirmed) greys the whole area out. Nothing here sends a request
//! that the page already knows the server would refuse.

use chrono::Utc;
use dioxus::prelude::*;
use uuid::Uuid;

use super::super::fields::TeamFormFields;
use super::super::form::{FieldRules, RegisterDraft};
use super::super::lang::{with_days, Lang};
use super::Lock;
use crate::config::Config;
use crate::data::Storage;
use crate::side::details::address::{Address, AddressParam};
use crate::storage::{validate_cancel_reason, ShareTeamConfig, TeamAccess, TeamData};
use crate::ui::*;

/// Greyed-out look of an area that cannot be changed (the same for the form and the address).
pub const LOCKED: &str = "opacity-60 pointer-events-none select-none";

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
    lock: Option<Lock>,
    lang: Lang,
) -> Element {
    let t = lang.txt();
    let toasts = use_toasts();
    let save_task = use_busy();
    let read_only = lock.is_some();

    let draft = use_signal(|| RegisterDraft::from_team(&team));
    let mut consent = use_signal(|| false);
    let address = AddressParam::new(&team.address);
    // What is stored. After a successful save it becomes the new reference ("unsaved changes" is
    // everything that differs from it).
    let mut saved = use_signal(|| RegisterDraft::from_team(&team));
    let initial_address = use_hook(|| address.snapshot());
    let mut saved_address = use_signal(move || initial_address);

    let rules = config.as_ref().map(FieldRules::from_config).unwrap_or_else(FieldRules::open);
    let edit_deadline = config.as_ref().and_then(|config| config.edit_deadline);
    let current = draft.read().clone();
    let errors = current.errors(rules, t);
    let dirty = current != *saved.read() || address.snapshot() != *saved_address.read();
    // A new or changed dietary entry needs the express consent again (Art. 9 GDPR); an unchanged one
    // was consented to when it was entered.
    let diets_changed = rules.allowed.diets
        && !current.diets.trim().is_empty()
        && current.diets.trim() != saved.read().diets.trim();
    let can_save = !read_only && dirty && errors.is_empty() && (!diets_changed || consent());
    let reviews_changes = config
        .as_ref()
        .is_some_and(|config| config.review_trigger_fields.iter().any(|field| config.allows(*field)));
    // Keeps the current status; the backend applies the organizer's review rules on top.
    let needs_check = team.needs_check();
    let existing = team.clone();

    let on_save = move |_| {
        // The deadline may have passed while the page was open.
        if edit_deadline.is_some_and(|deadline| deadline < Utc::now().naive_utc()) {
            return toasts.error(t.save_failed_title, t.hint_edit_closed);
        }
        let current = draft.peek().clone();
        let Ok(address_data) = address.get_address_data() else { return };
        let update = match current.to_update(address_data, needs_check, rules.allowed, &existing) {
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
                    consent.set(false);
                    toasts.success(t.saved_title, t.saved_body);
                    storage.invalidate();
                }
                Err(e) => toasts.error(t.save_failed_title, e),
            }
        });
    };

    // Address: shown as one line; the editor (search, map, manual entry) is behind "change".
    let (address_text, _, _) = address.snapshot();
    let address_line = if address_text.trim().is_empty() { t.address_none.to_string() } else { address_text };
    let show_bar = dirty && !read_only;

    rsx! {
        // No title: the tab above already says "Team data".
        Card { class: "max-sm:rounded-none max-sm:border-x-0",
            if reviews_changes && !read_only {
                Notice { tone: NoticeTone::Attention, small: true, "{t.hint_review_changes}" }
            }
            if lock.is_some() {
                // The reason is in the status card above; here only that the details are locked.
                Notice { small: true, "{t.lock_note}" }
            }
            div {
                class: if read_only { "space-y-4 {LOCKED}" } else { "space-y-4" },
                "aria-disabled": read_only,
                TeamFormFields {
                    draft,
                    errors,
                    rules,
                    lang,
                    disabled: read_only,
                    consent: diets_changed.then_some(consent),
                }
                Panel { flush: true,
                    div { class: "flex items-start gap-2 px-3 py-2.5",
                        span { class: "mt-0.5 text-amber-700", AddressSVG {} }
                        div { class: "min-w-0 flex-1",
                            Eyebrow { text: t.address_label, class: "block" }
                            p { class: "text-sm text-zinc-800 break-words", "{address_line}" }
                        }
                    }
                    if !read_only {
                        details { class: "group border-t border-amber-100",
                            summary { class: "flex items-center justify-between px-3 min-h-11 py-2 text-sm font-medium text-brand-hover cursor-pointer select-none list-none [&::-webkit-details-marker]:hidden",
                                span { "{t.address_change}" }
                                svg {
                                    class: "w-4 h-4 transition-transform group-open:rotate-180",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2.5",
                                    "aria-hidden": "true",
                                    polyline { points: "6 9 12 15 18 9" }
                                }
                            }
                            div { class: "px-3 pb-3", Address { param: address, lang: lang.address() } }
                        }
                    }
                }
            }
        }
        if show_bar {
            // Save bar: on a phone fixed at the bottom of the screen, so "Save" is always in reach,
            // on larger screens simply below the form.
            div { class: "fixed inset-x-0 bottom-0 z-30 border-t border-amber-200 bg-white/95 backdrop-blur px-4 py-3 \
                           shadow-[0_-4px_14px_rgba(0,0,0,0.08)] sm:static sm:z-auto sm:border-0 sm:bg-transparent sm:px-0 \
                           sm:py-0 sm:shadow-none sm:mt-4",
                div { class: "mx-auto flex max-w-5xl items-center justify-between gap-3",
                    p { class: "text-xs text-zinc-600", "{t.unsaved}" }
                    Button { label: t.btn_save, loading: save_task.is_busy(), disabled: !can_save, onclick: on_save }
                }
            }
            // Room for the bar, so that it does not cover the last field.
            div { class: "h-16 sm:hidden", "aria-hidden": "true" }
        }
    }
}

// ─────────────────────────────────────────────
//  Own link
// ─────────────────────────────────────────────

/// The team's own link, with the warning that it is the only way back if no email address is on
/// file (the row is open from the start in that case).
#[component]
pub fn LinkSection(team_link: String, has_mail: bool, lang: Lang) -> Element {
    let t = lang.txt();
    rsx! {
        p { class: "text-sm text-zinc-600 leading-relaxed", "{t.link_body}" }
        if !has_mail {
            Notice { tone: NoticeTone::Danger, small: true, "{t.link_nomail_warning}" }
        }
        CopyField {
            label: t.lbl_your_link,
            value: team_link,
            button_label: t.btn_copy,
            copied_label: t.copied,
            failed_label: t.copy_failed_title,
        }
    }
}

// ─────────────────────────────────────────────
//  Cancel
// ─────────────────────────────────────────────

/// Canceling is the only way for a guest to leave. Deleting the team is up to the organiser.
/// After the edit deadline the card is greyed out and points to the organiser.
#[component]
pub fn CancelSection(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    edit_deadline: Option<chrono::NaiveDateTime>,
    closed: bool,
    lang: Lang,
) -> Element {
    let t = lang.txt();
    let toasts = use_toasts();
    let cancel_task = use_busy();
    let mut reason = use_signal(String::new);
    let mut confirm = use_signal(|| false);
    let config = use_context::<Signal<Option<Config>>>();
    let canceled_days = config.read().as_ref().and_then(|config| config.legal.retention_canceled_team_days);

    let reason_text = reason.read().trim().to_string();
    let reason_value = (!reason_text.is_empty()).then(|| reason_text.clone());
    // The rule comes from the storage layer; the page only translates the context.
    let reason_error = validate_cancel_reason(&reason_value).err().unwrap_or_default();

    let on_cancel = move |_| {
        // After the deadline the server refuses a cancellation: tell instead of asking.
        if edit_deadline.is_some_and(|deadline| deadline < Utc::now().naive_utc()) {
            confirm.set(false);
            return toasts.error(t.cancel_failed_title, t.cancel_locked);
        }
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
        div { class: "space-y-3",
            if closed {
                Notice { small: true, "{t.cancel_locked}" }
            }
            div { class: if closed { "space-y-4 {LOCKED}" } else { "space-y-4" }, "aria-disabled": closed,
                p { class: "text-sm text-zinc-600 leading-relaxed", "{t.cancel_body}" }
                if let Some(days) = canceled_days {
                    p { class: "text-xs text-zinc-500 leading-relaxed", "{with_days(t.cancel_retention, days)}" }
                }
                Field { label: t.lbl_cancel_reason, error: reason_error.clone(),
                    TextArea {
                        value: reason(),
                        placeholder: t.cancel_reason_ph,
                        rows: 2,
                        error: !reason_error.is_empty(),
                        disabled: closed,
                        oninput: move |e: Event<FormData>| reason.set(e.value()),
                    }
                }
                if confirm() && !closed {
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
                        Button {
                            label: t.btn_cancel,
                            variant: ButtonVariant::DangerOutline,
                            disabled: closed,
                            onclick: move |_| confirm.set(true),
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Own data: download
// ─────────────────────────────────────────────

/// "Your data": download of everything stored about the team. (Deleting the team is not offered
/// here: the organiser does that, see `CancelCard`.)
#[component]
pub fn DataSection(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    lang: Lang,
) -> Element {
    let t = lang.txt();
    let toasts = use_toasts();
    let download_task = use_busy();

    let on_download = move |_| {
        download_task.run(async move {
            let result = { storage.read().await.export_team(cook_and_run_id, team_id, access_token).await };
            match result {
                Ok(json) => {
                    if let Err(e) = crate::web::download_text(&format!("team-data-{team_id}.json"), &json) {
                        toasts.error(t.download_failed_title, e);
                    }
                }
                Err(e) => toasts.error(t.download_failed_title, e),
            }
        });
    };

    rsx! {
        p { class: "text-sm text-zinc-600 leading-relaxed", "{t.data_body}" }
        div {
            Button {
                label: t.btn_download,
                variant: ButtonVariant::Secondary,
                loading: download_task.is_busy(),
                onclick: on_download,
            }
        }
    }
}
