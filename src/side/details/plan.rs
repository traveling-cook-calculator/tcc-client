//! The schedule sheet of one hosting team.

use dioxus::prelude::*;
use uuid::Uuid;

use super::run_schedule::run_schedule::{download, RunSchedule};
use super::run_schedule::Schedule;
use crate::data::use_storage;
use crate::storage::mapper::Plan as ResolvedPlan;
use crate::storage::{PlanConfigData, TeamAccess};
use crate::ui::*;

/// Everything the page can end up showing.
#[derive(Clone)]
enum PlanPage {
    NoPlan,
    NotInPlan,
    Ready { config: PlanConfigData, schedule: Schedule },
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

            let is_cloud = storage.select_cook_and_run_meta(id).await?.is_in_cloud;
            let Some(plan) = storage.select_plan_of_cook_and_run(id).await? else {
                return Ok(PlanPage::NoPlan);
            };
            let config = storage.select_plan_config_of_cook_and_run(id).await?.unwrap_or_default();
            let start = storage.select_cook_and_run_start_point(id).await?;
            let end = storage.select_cook_and_run_end_point(id).await?;

            let schedule = if is_cloud {
                // The backend computes the sheet of one team: one request. Resolving it from the
                // team list would cost one request per team and exceeded the rate limit.
                let team_plan = storage.select_team_plan(id, team_id, &TeamAccess::Admin).await?;
                Some(Schedule::from_team_plan(&team_plan, &start, &end))
            } else {
                // Local projects: everything is in the browser, no requests involved.
                let courses = storage.select_cook_and_run_course_list(id).await?;
                let teams = storage.select_cook_and_run_team_list(id).await?;
                let resolved = ResolvedPlan::from_plan_data(&plan, &courses, &teams);
                Schedule::for_team(team_id, &resolved, &start, &end)
            };

            Ok(match schedule {
                Some(schedule) => PlanPage::Ready { config, schedule },
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
            PlanPage::Ready { config, schedule } => rsx! {
                PlanContent { key: "{key}", config, schedule }
            },
        },
    )
}

#[component]
pub(crate) fn PlanContent(config: PlanConfigData, schedule: Schedule) -> Element {
    let download_task = use_busy();
    let title = config.title.clone();
    let team_name = schedule.host.name.clone();

    rsx! {
        button {
            r#type: "button",
            "aria-label": "Download schedule as image",
            title: "Download as image",
            class: "fixed bottom-6 right-6 z-40 flex items-center justify-center w-14 h-14 rounded-full \
                    bg-brand-hover text-white shadow-lg hover:shadow-xl hover:scale-110 \
                    transition-all duration-150 cursor-pointer",
            onclick: move |_| {
                let (title, team_name) = (title.clone(), team_name.clone());
                download_task.run(async move { download(title, team_name).await });
            },
            if download_task.is_busy() {
                Spinner {}
            } else {
                DownloadSVG {}
            }
        }

        RunSchedule { plan_config: config, schedule }
    }
}
