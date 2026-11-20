//! The team's plan, as computed by the backend for its token, drawn with the design the
//! organizer made active. Without a design there is no plan to show: the server hands none out,
//! and the page does not fall back to a classic sheet.

use dioxus::prelude::*;
use uuid::Uuid;

use super::super::lang::Lang;
use crate::data::Storage;
use crate::schedule_design::ScheduleDesign;
use crate::side::details::plan::PlanContent;
use crate::side::details::run_schedule::{Schedule, SheetInfo};
use crate::storage::design::{is_team_canceled_message, DesignErrorKind};
use crate::storage::{Language, TeamAccess};
use crate::ui::*;

/// Title of the sheet: participants cannot read the project, so its name is not known to them.
const SHEET_TITLE: &str = "Cook & Run";

/// What the tab shows once it is loaded.
#[derive(Clone, PartialEq)]
enum Loaded {
    /// The team canceled (`409`): no data is available for it any more.
    Canceled,
    /// The organiser has not chosen a design (anymore): there is no plan to show.
    NotPublished,
    Plan { schedule: Box<Schedule>, design: Box<ScheduleDesign> },
}

/// Mounted only while the tab is open, so nothing is requested before the team looks at it.
#[component]
pub fn PlanTab(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    lang: Lang,
) -> Element {
    let t = lang.txt();
    let plan = use_resource(move || async move {
        let backend = storage.read().await;
        let access = TeamAccess::Token(access_token);
        let team_plan = match backend.select_team_plan(cook_and_run_id, team_id, &access).await {
            Ok(plan) => plan,
            Err(e) if is_team_canceled_message(&e) => return Ok(Loaded::Canceled),
            // The server answers 404 `no_active_design` while no design is in use.
            Err(e) if e.contains("no_active_design") || e.to_lowercase().contains("no active design") => {
                return Ok(Loaded::NotPublished)
            }
            Err(e) => return Err(e),
        };
        // The meeting points only dress up the sheet; it works without them (they may not be
        // readable with a team token).
        let start = backend.select_cook_and_run_start_point(cook_and_run_id).await.ok().flatten();
        let end = backend.select_cook_and_run_end_point(cook_and_run_id).await.ok().flatten();
        // `404` (no design is active) is `None`; any other failure only costs the design.
        let design = match backend.load_team_design(cook_and_run_id, team_id, &access).await {
            Ok(design) => design,
            Err(e) if e.is(DesignErrorKind::TeamCanceled) => return Ok(Loaded::Canceled),
            Err(e) => {
                web_sys::console::warn_1(&format!("The design could not be loaded: {e}").into());
                None
            }
        };
        let Some(design) = design else { return Ok(Loaded::NotPublished) };
        Ok::<_, String>(Loaded::Plan {
            schedule: Box::new(Schedule::from_team_plan(&team_plan, &start, &end)),
            design: Box::new(design),
        })
    });

    match &*plan.read_unchecked() {
        None => rsx! {
            div { class: "space-y-3 px-4 sm:px-0", "aria-busy": "true",
                Skeleton { class: "h-8 w-48" }
                Skeleton { class: "h-40 w-full" }
                Skeleton { class: "h-40 w-full" }
                span { class: "sr-only", "Loading…" }
            }
        },
        Some(Err(details)) => rsx! {
            EmptyState { title: t.plan_unavailable_title, text: t.plan_unavailable_body,
                p { class: "text-xs text-zinc-400 break-words max-w-xs", "{details}" }
            }
        },
        Some(Ok(Loaded::NotPublished)) => rsx! {
            EmptyState { title: t.plan_unavailable_title, text: t.plan_unavailable_body }
        },
        Some(Ok(Loaded::Canceled)) => rsx! {
            EmptyState { title: t.plan_canceled_title, text: t.plan_canceled_body }
        },
        Some(Ok(Loaded::Plan { schedule, design })) => {
            // The sheet follows the visitor's language, not the organizer's.
            let language = match lang {
                Lang::De => Language::German,
                Lang::En => Language::English,
            };
            let info = SheetInfo::new(SHEET_TITLE, None, language);
            rsx! {
                PlanContent { info, schedule: (**schedule).clone(), design: (**design).clone() }
            }
        }
    }
}
