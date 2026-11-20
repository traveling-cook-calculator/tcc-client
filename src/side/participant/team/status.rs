//! The top of the team page: where the team stands, at a glance, in one card instead of a stack of
//! boxes: its name and state, two chips, the deadline, and what the team has to do or to know.

use dioxus::prelude::*;
use uuid::Uuid;

use super::super::lang::Lang;
use super::Lock;
use crate::data::Storage;
use crate::storage::{ShareTeamConfig, TeamAccess, TeamData, TeamStatus};
use crate::time::format_local;
use super::super::views::ClockSVG;
use crate::ui::*;

/// The top of the page, kept short so that the details are visible without scrolling on a phone:
/// the team's name with its state, two small chips (email, plan), the deadline in one line and,
/// only when there is something to do or to know, a note.
#[component]
pub fn TeamHeader(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    team: TeamData,
    config: Option<ShareTeamConfig>,
    lock: Option<Lock>,
    /// `None` if the configuration could not be read.
    plan_published: Option<bool>,
    /// The secret of the confirmation link in the invitation mail, if the page was opened with it.
    verify_secret: Option<Uuid>,
    lang: Lang,
) -> Element {
    let t = lang.txt();
    let canceled = team.status == TeamStatus::Canceled;
    let cloud = team.cloud.as_ref();
    let verified = cloud.is_some_and(|cloud| cloud.email_verified_at.is_some());
    let has_mail = team.mail.is_some();
    let require_verification = config.as_ref().is_some_and(|config| config.require_email_verification);
    let edit_deadline = config.as_ref().and_then(|config| config.edit_deadline);
    let resends_left = cloud.and_then(|cloud| cloud.verification_resends_left);

    let (participation_tone, participation) = match team.status {
        TeamStatus::Canceled => (PillTone::Red, t.val_canceled),
        TeamStatus::Review => (PillTone::Amber, t.status_review),
        _ => (PillTone::Green, t.val_active),
    };
    let (email_tone, email) = match (has_mail, verified) {
        (false, _) => (PillTone::Neutral, t.chip_email_none),
        (true, true) => (PillTone::Green, t.chip_email_ok),
        (true, false) => (PillTone::Amber, t.chip_email_pending),
    };
    let (plan_tone, plan) = match (canceled, plan_published) {
        (false, Some(true)) => (PillTone::Green, t.chip_plan_ready),
        _ => (PillTone::Neutral, t.chip_plan_wait),
    };
    // The deadline always has a line, also when there is none: a guest must not wonder whether it
    // was overlooked.
    let deadline_line = match (edit_deadline, lock) {
        (Some(deadline), Some(Lock::EditClosed)) => format!("{} {}", t.deadline_over, lang.format_deadline(deadline)),
        (Some(deadline), _) => format!("{} {}", t.deadline_open, lang.format_deadline(deadline)),
        (None, _) => t.deadline_none.to_string(),
    };
    let deadline_over = lock == Some(Lock::EditClosed);
    let cancel_info = cloud.map(|cloud| (cloud.canceled_at, cloud.cancel_reason.clone()));

    rsx! {
        Card { padded: false, class: "max-sm:rounded-none max-sm:border-x-0",
            div { class: "p-4 space-y-3",
                div { class: "flex items-start justify-between gap-3",
                    div { class: "min-w-0 break-words", Headline2 { headline: team.name.clone() } }
                    Pill { tone: participation_tone, "{participation}" }
                }
                if !canceled {
                    ul { class: "flex flex-wrap gap-1.5",
                        li { Pill { tone: email_tone, "{email}" } }
                        li { Pill { tone: plan_tone, "{plan}" } }
                    }
                }
                if !canceled {
                    p { class: if deadline_over { "flex items-start gap-1.5 text-xs font-medium text-amber-900" } else { "flex items-start gap-1.5 text-xs text-zinc-600" },
                        ClockSVG { class: "w-3.5 h-3.5 mt-px shrink-0" }
                        span { "{deadline_line}" }
                    }
                }
            }

            if canceled {
                div { role: "status", class: "border-t border-red-200 bg-red-50 px-4 py-3 space-y-1",
                    p { class: "text-sm text-red-900 leading-relaxed", "{t.hint_canceled}" }
                    if let Some((when, reason)) = cancel_info {
                        if let Some(when) = when {
                            p { class: "text-xs text-red-800", "{t.canceled_on} {format_local(when)}" }
                        }
                        if let Some(reason) = reason.filter(|reason| !reason.trim().is_empty()) {
                            p { class: "text-xs text-red-800 whitespace-pre-wrap", "{t.canceled_reason}: {reason}" }
                        }
                    }
                }
            } else {
                if team.status == TeamStatus::Review {
                    p { role: "status", class: "border-t border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-900 leading-relaxed",
                        "{t.hint_review}"
                    }
                }
                if has_mail && !verified {
                    VerifyAction {
                        storage,
                        cook_and_run_id,
                        team_id,
                        access_token,
                        require_verification,
                        edit_closed: lock == Some(Lock::EditClosed),
                        resends_left,
                        verify_secret,
                        lang,
                    }
                }
                if lock == Some(Lock::EditClosed) {
                    p { role: "status", class: "border-t border-amber-300 bg-amber-50 px-4 py-3 text-sm font-medium text-amber-900 leading-relaxed",
                        "{t.hint_edit_closed}"
                    }
                }
            }
        }
    }
}

/// The address is not confirmed yet. Opened with the link of the mail (it carries the secret) the
/// page confirms it on arrival; otherwise it says what to do and can send the mail again.
/// The team link alone can not confirm anything, so no button pretends it can.
#[component]
fn VerifyAction(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    require_verification: bool,
    /// The edit deadline is over: the address can no longer be changed here.
    edit_closed: bool,
    resends_left: Option<u8>,
    verify_secret: Option<Uuid>,
    lang: Lang,
) -> Element {
    let t = lang.txt();
    let toasts = use_toasts();
    let verify_task = use_busy();
    let resend_task = use_busy();
    // The secret of the link did not work (outdated: a newer mail replaced it).
    let mut link_failed = use_signal(|| false);

    // Opened through the mail link (it carries the secret): the address is confirmed on arrival.
    // Only that link can do it, the team link the guest keeps has no secret. If it fails (network,
    // outdated link) the button below tries again.
    let confirm = move || {
        let Some(secret) = verify_secret else { return };
        verify_task.run(async move {
            let result = {
                storage.read().await.verify_team_email(cook_and_run_id, team_id, access_token, secret).await
            };
            match result {
                Ok(()) => {
                    crate::web::forget_verify_secret();
                    toasts.success(t.verify_done, "");
                    storage.invalidate();
                }
                Err(e) => {
                    link_failed.set(true);
                    toasts.error(t.verify_failed_title, e);
                }
            }
        });
    };
    use_hook(confirm);

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
                Ok(()) => {
                    toasts.success(t.resent_title, t.resent_body);
                    storage.invalidate();
                }
                Err(e) => toasts.error(t.verify_failed_title, e),
            }
        });
    };

    let confirming = verify_task.is_busy();
    let resends_text = resends_left
        .map(|left| t.resends_left.replace("{n}", &left.to_string()))
        .unwrap_or_default();
    let no_resends = resends_left == Some(0);
    // The button is only for a second try after the automatic confirmation failed.
    let can_confirm = verify_secret.is_some() && link_failed();
    let text = if confirming {
        t.hint_verifying
    } else if link_failed() {
        t.hint_verify_link_old
    } else if edit_closed {
        t.hint_pending_closed
    } else if require_verification {
        t.hint_pending_required
    } else {
        t.hint_pending_optional
    };

    rsx! {
        div { role: "status", class: "border-t border-amber-300 bg-amber-50 px-4 py-3 space-y-3",
            p { class: "text-sm text-amber-900 leading-relaxed", "{text}" }
            if can_confirm || confirming {
                div {
                    Button { label: t.btn_confirm_email, loading: confirming, onclick: move |_| confirm() }
                }
            }
            if !confirming {
                div { class: "flex flex-wrap items-center gap-3",
                    Button {
                        label: t.btn_resend,
                        variant: ButtonVariant::Secondary,
                        loading: resend_task.is_busy(),
                        disabled: no_resends,
                        onclick: on_resend,
                    }
                    if no_resends {
                        p { class: "text-xs text-amber-900", "{t.resends_none}" }
                    } else if resends_left.is_some() {
                        p { class: "text-xs text-amber-900", "{resends_text}" }
                    }
                }
            }
        }
    }
}
