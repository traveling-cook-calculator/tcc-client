//! The schedule sheet of one hosting team.

use dioxus::prelude::*;
use uuid::Uuid;

use super::run_schedule::design::{download_design, DesignedSchedule};
use super::run_schedule::export::file_stem;
use super::run_schedule::sheet_design::{load_active_design, LanguageTabs};
use super::run_schedule::{browser_language, Schedule, SheetInfo};
use crate::data::use_storage;
use crate::schedule_design::{ScheduleDesign, EXPORT_WIDTH};
use crate::storage::mapper::Plan as ResolvedPlan;
use crate::storage::{Language, MeetingPointData, PlanData, StorageManager, TeamAccess};
use crate::time::to_local;
use crate::ui::*;

/// Everything the page can end up showing.
#[derive(Clone)]
enum PlanPage {
    NoPlan,
    NotInPlan,
    Ready { project_name: String, date: chrono::NaiveDate, design: Option<ScheduleDesign>, schedule: Schedule },
}

/// The schedule sheet of one team, or `None` if the team is not part of the plan.
pub(crate) async fn team_schedule(
    storage: &StorageManager,
    id: Uuid,
    team_id: Uuid,
    plan: &PlanData,
    is_cloud: bool,
    start: &Option<MeetingPointData>,
    end: &Option<MeetingPointData>,
) -> Result<Option<Schedule>, String> {
    if is_cloud {
        // The backend computes the sheet of one team: one request. Resolving it from the
        // team list would cost one request per team and exceeded the rate limit.
        let team_plan = storage.select_team_plan(id, team_id, &TeamAccess::Admin).await?;
        Ok(Some(Schedule::from_team_plan(&team_plan, start, end)))
    } else {
        // Local projects: everything is in the browser, no requests involved.
        let courses = storage.select_cook_and_run_course_list(id).await?;
        let teams = storage.select_cook_and_run_team_list(id).await?;
        let resolved = ResolvedPlan::from_plan_data(plan, &courses, &teams);
        Ok(Schedule::for_team(team_id, &resolved, start, end))
    }
}

/// Loads everything for the sheet of one team (used by the schedule designer for its preview).
pub(crate) async fn load_team_schedule(
    storage: crate::data::Storage,
    id: Uuid,
    team_id: Uuid,
) -> Result<Option<Schedule>, String> {
    let storage = storage.read().await;
    let is_cloud = storage.select_cook_and_run_meta(id).await?.is_in_cloud;
    let Some(plan) = storage.select_plan_of_cook_and_run(id).await? else {
        return Ok(None);
    };
    let start = storage.select_cook_and_run_start_point(id).await?;
    let end = storage.select_cook_and_run_end_point(id).await?;
    team_schedule(&storage, id, team_id, &plan, is_cloud, &start, &end).await
}

#[component]
pub fn Plan(cook_and_run_id: ReadSignal<Uuid>, team_id: ReadSignal<Uuid>) -> Element {
    let storage = use_storage();

    // One load for the whole page (the old code started one request per involved team,
    // from inside a loop and behind several early returns).
    let page = use_resource(move || {
        let (id, team_id) = (cook_and_run_id(), team_id());
        async move {
            let _ = storage.revision();
            let storage = storage.read().await;

            let meta = storage.select_cook_and_run_meta(id).await?;
            let (is_cloud, project_name) = (meta.is_in_cloud, meta.name);
            let date = to_local(meta.occur).date_naive();
            let Some(plan) = storage.select_plan_of_cook_and_run(id).await? else {
                return Ok(PlanPage::NoPlan);
            };
            let start = storage.select_cook_and_run_start_point(id).await?;
            let end = storage.select_cook_and_run_end_point(id).await?;

            let schedule = team_schedule(&storage, id, team_id, &plan, is_cloud, &start, &end).await?;

            // A design that cannot be loaded is reported like a missing one (there is no other sheet).
            let design = match load_active_design(&storage, id, is_cloud).await {
                Ok(design) => design,
                Err(e) => {
                    web_sys::console::warn_1(&format!("The design could not be loaded: {e}").into());
                    None
                }
            };

            Ok(match schedule {
                Some(schedule) => PlanPage::Ready { project_name, date, design, schedule },
                None => PlanPage::NotInPlan,
            })
        }
    });

    let key = format!("{}-{}", cook_and_run_id(), team_id());
    render_resource(
        page,
        "Could not load the plan. You may need to log in or the servers may be offline.",
        move |page| match page {
            PlanPage::NoPlan => rsx! {
                ErrorView {
                    error_text: "No plan found in this project.",
                    error_details: "Please come back later, when a plan has been generated.",
                }
            },
            PlanPage::NotInPlan => rsx! {
                ErrorView {
                    error_text: "This team is not part of the plan.",
                    error_details: "Recalculate the plan to include it.",
                }
            },
            PlanPage::Ready { project_name, date, design, schedule } => rsx! {
                OwnerPlan { key: "{key}", project_name, date, design, schedule }
            },
        },
    )
}

/// The sheet for the organizer, who picks the language of the sheet.
#[component]
fn OwnerPlan(
    project_name: String,
    date: chrono::NaiveDate,
    design: Option<ScheduleDesign>,
    schedule: Schedule,
) -> Element {
    let mut language = use_signal(browser_language);
    let info = SheetInfo::new(project_name.clone(), Some(date), language());
    // The sheets are drawn with the design of the project. Without one there is nothing to show.
    let Some(design) = design else {
        return rsx! {
            EmptyState {
                title: "No design for the sheets yet",
                text: "Create a design on the Schedules page (and use it for the teams). The sheets of the teams appear here once there is one.",
            }
        };
    };
    rsx! {
        div { class: "w-full min-w-0",
            div { class: "mb-4 flex justify-center",
                LanguageTabs { value: language(), onchange: move |value: Language| language.set(value) }
            }
            PlanContent { project_name: Some(project_name), info, schedule, design }
        }
    }
}

#[component]
pub(crate) fn PlanContent(
    /// Name of the project, the first part of the file name of the download. Without it (the page of
    /// a participant, who cannot read the project) the title of the sheet is used.
    #[props(default)]
    project_name: Option<String>,
    info: SheetInfo,
    schedule: Schedule,
    /// The design of the project (made on the Schedules page). There is no other way to draw a sheet.
    design: ScheduleDesign,
) -> Element {
    let download_task = use_busy();
    let toasts = use_toasts();
    let file_name = file_stem(project_name.as_deref().unwrap_or(&info.title), &schedule.host.name);

    rsx! {
        FloatingButton {
            label: "Download as image",
            onclick: move |_| {
                let file_name = file_name.clone();
                download_task.run(async move {
                    if let Err(e) = download_design(&file_name, EXPORT_WIDTH).await {
                        toasts.error("The image could not be created", e);
                    }
                });
            },
            if download_task.is_busy() {
                Spinner {}
            } else {
                DownloadSVG {}
            }
        }

        div { class: "w-full min-w-0",
            div { class: "mx-auto w-full max-w-xl",
                DesignedSchedule { design, config: info, schedule }
            }
        }
    }
}
