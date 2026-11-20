//! A team's personal page: `/project/{project}/team/{team}#token=…`.
//!
//! The token (chosen by the browser at registration, see `register.rs`) is the team's only
//! credential and part of the link (in the fragment). The page is one status card on top, then the
//! team's details next to a column with its link, its data and the cancellation:
//!
//! | backend call                                    | where                          |
//! |-------------------------------------------------|--------------------------------|
//! | `select_team_with_access`                       | loading the page               |
//! | `update_team_with_access`                       | `edit.rs` - "Save changes"     |
//! | `cancel_team`                                   | `edit.rs` - cancel card        |
//! | `verify_team_email`, `resend_team_verification` | `status.rs` - email address    |
//! | `select_team_plan`                              | `plan.rs` - "Plan" tab         |
//!
//! Opened with the link of the invitation mail (`#token=…&verify=…`) the page also confirms the
//! email address on arrival. The secret of that link exists only in the mail: the link the team
//! keeps (and everything the page and the API show) can not confirm anything.

mod edit;
mod plan;
mod status;

use chrono::Utc;
use dioxus::prelude::*;
use uuid::Uuid;

use super::lang::Lang;
use super::use_participant;
use crate::data::Storage;
use crate::storage::{ShareTeamConfig, TeamAccess, TeamData, TeamStatus};
use crate::ui::*;
use edit::{CancelSection, DataSection, LinkSection, TeamEditCard};
use plan::PlanTab;
use status::TeamHeader;

/// Why the details can not be changed (the area is greyed out). Checked on the page, so a request
/// that the server would refuse is not made.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lock {
    /// The team canceled: its data is deleted or frozen.
    Canceled,
    /// The edit deadline of the organiser has passed.
    EditClosed,
}


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
    rsx! { TeamView { cook_and_run_id, team_id, access_token, verify_secret: None } }
}

/// Link of the mails: `/project/{project}/team/{team}#token={token}`. The token is read from the
/// URL fragment (see `deeplink.rs`).
#[component]
pub fn TeamDeepLink(cook_and_run_id: ReadSignal<Uuid>, team_id: ReadSignal<Uuid>) -> Element {
    let token = crate::web::team_token(team_id());
    let access_token = use_signal(|| token.unwrap_or_else(Uuid::nil));
    // Only the link of the invitation mail carries it.
    let verify_secret = use_hook(crate::web::team_verify_secret);

    match token {
        Some(_) => rsx! {
            TeamView { key: "{team_id()}", cook_and_run_id, team_id, access_token: ReadSignal::from(access_token), verify_secret }
        },
        None => {
            let lang = Lang::default();
            let t = lang.txt();
            rsx! {
                div { class: "flex-1 min-w-0 w-full sm:max-w-2xl sm:mx-auto flex flex-col sm:py-10 sm:px-4 gap-4",
                    EmptyState { title: t.link_incomplete_title, text: t.link_incomplete_body }
                }
            }
        }
    }
}

#[component]
pub fn TeamVerifyPage(
    cook_and_run_id: ReadSignal<Uuid>,
    team_id: ReadSignal<Uuid>,
    access_token: ReadSignal<Uuid>,
) -> Element {
    // Links of this shape (token in the path) were mailed before the confirmation secret existed:
    // they open the team, the address is confirmed with the new mail (see the status card).
    rsx! { TeamView { cook_and_run_id, team_id, access_token, verify_secret: None } }
}

#[component]
fn TeamView(
    cook_and_run_id: ReadSignal<Uuid>,
    team_id: ReadSignal<Uuid>,
    access_token: ReadSignal<Uuid>,
    verify_secret: Option<Uuid>,
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
            // The team's own link carries no login either: the public configuration.
            let config = backend.select_public_share_config(id).await.ok().flatten();
            Ok::<_, String>(TeamPageData { team, config })
        }
    });

    let current = lang();
    let t = current.txt();

    rsx! {
        div { class: "flex-1 min-w-0 w-full sm:max-w-5xl sm:mx-auto flex flex-col sm:py-10 sm:px-4 gap-4 sm:gap-5",
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
                        verify_secret,
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
    verify_secret: Option<Uuid>,
) -> Element {
    let t = lang.txt();
    let mut tab = use_signal(|| 0usize);
    // Captured once; the clock must not be read in the render body.
    let now = use_hook(|| Utc::now().naive_utc());

    let TeamPageData { team, config } = &data;
    let canceled = team.status == TeamStatus::Canceled;
    let edit_deadline = config.as_ref().and_then(|config| config.edit_deadline);
    let edit_closed = edit_deadline.is_some_and(|deadline| deadline < now);
    // An unconfirmed address does not lock anything: the address may be wrong, and then the team
    // has to be able to correct it (or cancel).
    let lock = if canceled {
        Some(Lock::Canceled)
    } else if edit_closed {
        Some(Lock::EditClosed)
    } else {
        None
    };
    let plan_published = config.as_ref().map(|config| config.plan_published);
    let team_link = super::team_url(cook_and_run_id, team_id, access_token);
    // Cancelling stays possible while the address is unconfirmed; only the deadline and an
    // earlier cancellation stop it.
    let cancel_closed = canceled || edit_closed;

    rsx! {
        TeamHeader {
            storage,
            cook_and_run_id,
            team_id,
            access_token,
            team: team.clone(),
            config: config.clone(),
            lock,
            plan_published,
            verify_secret,
            lang,
        }

        div { class: "px-4 sm:px-0",
            Tabs {
                tabs: vec![t.tab_team, t.tab_plan],
                selected: tab(),
                onselect: move |index| tab.set(index),
            }
        }

        if tab() == 0 {
            div { class: "grid gap-4 sm:gap-5 lg:grid-cols-3 items-start",
                div { class: "lg:col-span-2 min-w-0",
                    TeamEditCard {
                        storage,
                        cook_and_run_id,
                        team_id,
                        access_token,
                        team: team.clone(),
                        config: config.clone(),
                        lock,
                        lang,
                    }
                }
                // Link, data and cancellation: one line each until opened.
                DisclosureList {
                    Disclosure { title: t.link_title, open: team.mail.is_none(),
                        LinkSection { team_link, has_mail: team.mail.is_some(), lang }
                    }
                    Disclosure { title: t.data_title,
                        DataSection { storage, cook_and_run_id, team_id, access_token, lang }
                    }
                    if !canceled {
                        Disclosure { title: t.cancel_title, danger: true,
                            CancelSection {
                                storage,
                                cook_and_run_id,
                                team_id,
                                access_token,
                                edit_deadline,
                                closed: cancel_closed,
                                lang,
                            }
                        }
                    }
                }
            }
        } else if canceled {
            EmptyState { title: t.plan_canceled_title, text: t.plan_canceled_body }
        } else if plan_published == Some(false) {
            // No design chosen yet: there is no plan to show, and nothing is requested for it.
            EmptyState { title: t.plan_unavailable_title, text: t.plan_unavailable_body }
        } else {
            // The plan tab only exists (and loads) once it is opened.
            PlanTab { storage, cook_and_run_id, team_id, access_token, lang }
        }
    }
}

