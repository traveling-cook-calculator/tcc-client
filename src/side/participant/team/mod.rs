//! A team's personal page: `/cook-and-run/{project}/team/{team}/{token}`.
//!
//! The token (chosen by the browser at registration, see `register.rs`) is the team's only
//! credential and part of the URL. Everything the backend offers for it is available here:
//!
//! | backend call                                  | where                         |
//! |-----------------------------------------------|-------------------------------|
//! | `select_team_with_access`                     | loading the page              |
//! | `update_team_with_access`                     | `edit.rs` – "Save changes"    |
//! | `cancel_team`                                 | `edit.rs` – cancel card       |
//! | `verify_team_email`, `resend_team_verification` | `edit.rs` – verification card |
//! | `select_team_plan`                            | `plan.rs` – "Plan" tab        |
//!
//! `/…/{token}/verify` is the link for the verification mail: same page, and it confirms the
//! email address on arrival.

mod edit;
mod plan;

use chrono::Utc;
use dioxus::prelude::*;
use uuid::Uuid;

use super::lang::Lang;
use super::use_participant;
use crate::data::Storage;
use crate::storage::{ShareTeamConfig, TeamAccess, TeamData, TeamStatus};
use crate::time::format_local;
use crate::ui::*;
use edit::{CancelCard, TeamEditCard, VerifyNotice};
use plan::PlanTab;

/// Team and the rules of its project, loaded together.
#[derive(Clone, PartialEq)]
struct TeamPageData {
    team: TeamData,
    /// Optional: required fields and deadlines. The page works (with fewer hints) without it.
    config: Option<ShareTeamConfig>,
}

#[component]
pub fn TeamPage(
    cook_and_run_id: ReadSignal<Uuid>,
    team_id: ReadSignal<Uuid>,
    access_token: ReadSignal<Uuid>,
) -> Element {
    rsx! { TeamView { cook_and_run_id, team_id, access_token, verify: false } }
}

#[component]
pub fn TeamVerifyPage(
    cook_and_run_id: ReadSignal<Uuid>,
    team_id: ReadSignal<Uuid>,
    access_token: ReadSignal<Uuid>,
) -> Element {
    rsx! { TeamView { cook_and_run_id, team_id, access_token, verify: true } }
}

#[component]
fn TeamView(
    cook_and_run_id: ReadSignal<Uuid>,
    team_id: ReadSignal<Uuid>,
    access_token: ReadSignal<Uuid>,
    verify: bool,
) -> Element {
    let participant = use_participant();
    let mut lang = use_signal(Lang::default);
    let handle = participant.storage();

    let mut loaded = use_resource(move || {
        let (id, team_id, token) = (cook_and_run_id(), team_id(), access_token());
        let participant = participant.clone();
        async move {
            // Subscribe first: `invalidate()` (after saving, canceling, …) reloads the team.
            if let Some(storage) = participant.storage() {
                let _ = storage.revision();
            }
            let storage = participant.connect().await?;
            let backend = storage.read().await;
            let team = backend.select_team_with_access(id, team_id, &TeamAccess::Token(token)).await?;
            let config = backend.select_cook_and_run_share_config(id).await.ok().flatten();
            Ok::<_, String>(TeamPageData { team, config })
        }
    });

    let current = lang();
    let t = current.txt();

    rsx! {
        div { class: "flex-1 min-w-0 w-full sm:max-w-2xl sm:mx-auto flex flex-col sm:py-10 sm:px-4 gap-4 sm:gap-5",
            div { class: "px-4 sm:px-0 pt-4 sm:pt-0",
                PageHeader {
                    title: t.team_page_title,
                    actions: rsx! {
                        SegmentedTabs {
                            options: Lang::ALL.iter().map(|lang| lang.label()).collect::<Vec<_>>(),
                            selected: current.index(),
                            onselect: move |index: usize| lang.set(Lang::ALL[index]),
                        }
                    },
                }
            }

            match (&*loaded.read_unchecked(), handle) {
                (Some(Ok(data)), Some(storage)) => rsx! {
                    TeamContent {
                        key: "{team_id()}",
                        storage,
                        cook_and_run_id: cook_and_run_id(),
                        team_id: team_id(),
                        access_token: access_token(),
                        data: data.clone(),
                        lang: current,
                        verify,
                    }
                },
                (Some(Err(details)), _) => rsx! {
                    EmptyState { title: t.team_not_found_title, text: t.team_not_found_body,
                        p { class: "text-xs text-zinc-400 break-words max-w-xs", "{details}" }
                        Button { label: t.btn_retry, variant: ButtonVariant::Secondary, onclick: move |_| loaded.restart() }
                    }
                },
                _ => rsx! { TeamPageSkeleton {} },
            }
        }
    }
}

/// Same shape as the loaded page, shown while the team loads.
#[component]
fn TeamPageSkeleton() -> Element {
    rsx! {
        div { class: "space-y-4", "aria-busy": "true",
            div { class: "flex gap-4 px-4 sm:px-0",
                Skeleton { class: "h-6 w-24" }
                Skeleton { class: "h-6 w-16" }
            }
            Card { class: "max-sm:rounded-none max-sm:border-x-0",
                for field in 0..5 {
                    div { key: "{field}", class: "space-y-1.5",
                        Skeleton { class: "h-3 w-28" }
                        Skeleton { class: "h-10 w-full" }
                    }
                }
                Skeleton { class: "h-9 w-40" }
            }
            span { class: "sr-only", "Loading…" }
        }
    }
}

#[component]
fn TeamContent(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    data: TeamPageData,
    lang: Lang,
    verify: bool,
) -> Element {
    let t = lang.txt();
    let mut tab = use_signal(|| 0usize);
    // Captured once; the clock must not be read in the render body.
    let now = use_hook(|| Utc::now().naive_utc());

    let TeamPageData { team, config } = &data;
    let canceled = team.status == TeamStatus::Canceled;
    let edit_closed = config
        .as_ref()
        .and_then(|config| config.edit_deadline)
        .is_some_and(|deadline| deadline < now);
    let verified = team.cloud.as_ref().is_some_and(|cloud| cloud.email_verified_at.is_some());
    let needs_verification = team.mail.is_some()
        && !verified
        && !canceled
        && (verify || config.as_ref().is_some_and(|config| config.require_email_verification));
    let reviews_changes = config.as_ref().is_some_and(|config| !config.review_trigger_fields.is_empty());

    rsx! {
        if canceled {
            CanceledCard { team: team.clone(), lang }
        }
        if needs_verification {
            VerifyNotice { storage, cook_and_run_id, team_id, access_token, lang, auto_verify: verify }
        }
        if team.status == TeamStatus::Review {
            div { class: "mx-4 sm:mx-0 flex flex-wrap items-center gap-2",
                Pill { tone: PillTone::Amber, "{t.status_review}" }
            }
        }
        if edit_closed && !canceled {
            div { class: "mx-4 sm:mx-0 rounded-xl border border-amber-300 bg-amber-50 px-4 py-3",
                p { class: "text-sm font-semibold text-amber-700", "{t.edit_closed}" }
            }
        }

        div { class: "px-4 sm:px-0",
            Tabs {
                tabs: vec![t.tab_team, t.tab_plan],
                selected: tab(),
                onselect: move |index| tab.set(index),
            }
        }

        // The plan tab only exists (and loads) once it is opened.
        if tab() == 0 {
            TeamEditCard {
                storage,
                cook_and_run_id,
                team_id,
                access_token,
                team: team.clone(),
                config: config.clone(),
                read_only: canceled || edit_closed,
                reviews_changes,
                lang,
            }
            if !canceled {
                CancelCard { storage, cook_and_run_id, team_id, access_token, lang }
            }
        } else {
            PlanTab { storage, cook_and_run_id, team_id, access_token, lang }
        }
    }
}

#[component]
fn CanceledCard(team: TeamData, lang: Lang) -> Element {
    let t = lang.txt();
    let cloud = team.cloud.as_ref();
    let when = cloud.and_then(|cloud| cloud.canceled_at).map(format_local);
    let reason = cloud
        .and_then(|cloud| cloud.cancel_reason.clone())
        .filter(|reason| !reason.trim().is_empty());

    rsx! {
        Card { title: t.canceled_title, tone: Tone::Danger, class: "max-sm:rounded-none max-sm:border-x-0",
            p { class: "text-sm text-zinc-700", "{t.canceled_body}" }
            if let Some(when) = when {
                p { class: "text-xs text-zinc-500", "{when}" }
            }
            if let Some(reason) = reason {
                p { class: "text-xs text-zinc-600 whitespace-pre-wrap", "{t.canceled_reason}: {reason}" }
            }
        }
    }
}
