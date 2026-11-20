//! Calculation: checks what is needed, calculates the plan, and shows it course by course.
//!
//! The plan is shown as the board of the courses (one row per course, one card per table). Teams can
//! be dragged between tables, the rules check every change and nothing is stored until "Save" is
//! pressed. The design of the schedule sheets is on the Schedules page.

use std::collections::{hash_map::DefaultHasher, HashSet};
use std::hash::{Hash, Hasher};

use chrono::NaiveTime;
use dioxus::prelude::*;
use uuid::Uuid;

use super::design_notice::{use_design_in_use, NoDesignNotice};
use super::plan_bundle::{load_bundle, PlanBundle};
use super::plan_editor::PlanBoard;
use crate::calculator::{
    Calculator, Course as CalcCourse, DataMapper, Point, SchemaCalculator, Team as CalcTeam,
};
use crate::data::use_storage;
use crate::storage::{PlanData, TeamStatus};
use crate::ui::*;
use crate::Route;

/// Limits of `SchemaCalculator` (it refuses anything else).
const MIN_TEAMS: usize = 9;
const REQUIRED_COURSES: usize = 3;

// ─────────────────────────────────────────────
//  Root
// ─────────────────────────────────────────────

#[component]
pub fn Calculate(cook_and_run_id: ReadSignal<Uuid>) -> Element {
    let storage = use_storage();
    let data = use_resource(move || {
        let id = cook_and_run_id();
        async move {
            let _ = storage.revision();
            load_bundle(storage, id).await
        }
    });

    let id = cook_and_run_id();
    render_resource(
        data,
        "Could not load the calculation data. You may need to log in or the servers may be offline.",
        move |data| rsx! { CalculateContent { key: "{id}", cook_and_run_id: id, data } },
    )
}

#[component]
fn CalculateContent(cook_and_run_id: Uuid, data: PlanBundle) -> Element {
    let design_in_use = use_design_in_use(cook_and_run_id, data.is_cloud);
    rsx! {
        PageShell {
            PageHeader {
                title: "Calculation",
                subtitle: "The plan course by course. Move teams between tables by hand; design the sheets on the Schedules page.".to_string(),
            }
            NoDesignNotice { cook_and_run_id, in_use: design_in_use }
            PlanSection { cook_and_run_id, data }
        }
    }
}

// ─────────────────────────────────────────────
//  Plan: prerequisites, calculation, board
// ─────────────────────────────────────────────

/// What is missing before the plan can be (re)calculated, and where to fix it.
#[derive(Clone, Copy, PartialEq)]
enum FixIn {
    StartEnd,
    Courses,
    Teams,
}

struct Blocker {
    text: String,
    fix_in: FixIn,
}

/// Why the plan cannot be (re)calculated right now. Derived from the data on every render.
fn blockers(data: &PlanBundle) -> Vec<Blocker> {
    let mut found = Vec::new();
    let mut add = |text: String, fix_in: FixIn| found.push(Blocker { text, fix_in });

    if data.end.is_none() {
        add("No end point configured.".to_string(), FixIn::StartEnd);
    }
    if data.courses.len() != REQUIRED_COURSES {
        add(
            format!("Exactly {REQUIRED_COURSES} courses are required (currently {}).", data.courses.len()),
            FixIn::Courses,
        );
    }
    let taking_part = data.teams.iter().filter(|team| team.status != TeamStatus::Canceled).count();
    if taking_part < MIN_TEAMS {
        add(
            format!("At least {MIN_TEAMS} teams are required (currently {taking_part}, canceled teams do not count)."),
            FixIn::Teams,
        );
    }

    let times: Vec<NaiveTime> = data.courses.iter().map(|course| course.time).collect();
    if let (Some(first), Some(last)) = (times.iter().min(), times.iter().max()) {
        if let Some(start) = &data.start
            && start.time > *first
        {
            add("The start point time must be before all course times.".to_string(), FixIn::StartEnd);
        }
        if let Some(end) = &data.end
            && end.time < *last
        {
            add("The end point time must be after all course times.".to_string(), FixIn::StartEnd);
        }
    }
    found
}

#[component]
fn PlanSection(cook_and_run_id: Uuid, data: PlanBundle) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let calc_task = use_busy();
    let mut confirm = use_signal(|| false);
    // Counts the calculations. Part of the board's key, so a recalculation always resets the board,
    // even if the new plan happens to be identical to the stored one (manual edits are dropped).
    let generation = use_signal(|| 0u32);

    let problems = blockers(&data);
    let has_plan = data.plan.is_some();

    let calc_data = data.clone();
    let run_calculation = move || {
        let data = calc_data.clone();
        calc_task.run(async move {
            let Some(end) = &data.end else { return };
            // Canceled teams do not take part any more.
            let taking_part: HashSet<Uuid> = data
                .teams
                .iter()
                .filter(|team| team.status != TeamStatus::Canceled)
                .map(|team| team.id)
                .collect();

            // The calculation needs every team's coordinates, which the short form does not carry.
            // This is the one place that loads all teams in full (one request per team in the cloud),
            // and only on an explicit click.
            let full_teams = { storage.read().await.select_cook_and_run_team_list(cook_and_run_id).await };
            let full_teams = match full_teams {
                Ok(teams) => teams,
                Err(e) => return toasts.error("The teams could not be loaded", e),
            };
            let teams: Vec<CalcTeam> = full_teams
                .iter()
                .filter(|team| taking_part.contains(&team.id))
                .map(CalcTeam::from_data)
                .collect();
            let courses: Vec<CalcCourse> = data.courses.iter().map(CalcCourse::from_data).collect();
            let plan = match SchemaCalculator::new(&Point::from_data(&end.address), &teams, &courses) {
                Ok(calculator) => calculator.calculate().to_data(),
                Err(e) => return toasts.error("The plan could not be calculated", e),
            };

            let result = { storage.write().await.update_plan_of_cook_and_run(cook_and_run_id, &plan).await };
            match result {
                Ok(()) => {
                    let mut generation = generation;
                    *generation.write() += 1;
                    storage.invalidate();
                    toasts.success("Plan calculated", "The courses were updated.");
                }
                Err(e) => toasts.error("Saving the plan failed", e),
            }
        });
    };
    let run_for_confirm = run_calculation.clone();

    // Teams that take part but have no seat in the stored plan.
    let not_in_plan = data.plan.as_ref().map_or(0, |plan| {
        let seated: HashSet<Uuid> = plan
            .hosting_list
            .iter()
            .flat_map(|hosting| std::iter::once(hosting.host).chain(hosting.guest_list.iter().copied()))
            .collect();
        data.taking_part().filter(|team| !seated.contains(&team.id)).count()
    });

    let on_calculate = move |_| {
        // A recalculation throws away every manual change.
        if has_plan {
            confirm.set(true);
        } else {
            run_calculation();
        }
    };
    let calculating = calc_task.is_busy();

    rsx! {
        div { class: "space-y-4",
            if !problems.is_empty() {
                Card { title: "Before the plan can be calculated", tone: Tone::Danger,
                    ul { class: "space-y-1.5 text-sm text-zinc-700",
                        for problem in problems.iter() {
                            li { key: "{problem.text}", class: "flex flex-wrap items-baseline gap-x-2",
                                span { "{problem.text}" }
                                match problem.fix_in {
                                    FixIn::StartEnd => rsx! {
                                        Link { to: Route::StartEnd { cook_and_run_id }, class: LINK_CLASS, "Start & End →" }
                                    },
                                    FixIn::Courses => rsx! {
                                        Link { to: Route::Courses { cook_and_run_id }, class: LINK_CLASS, "Courses →" }
                                    },
                                    FixIn::Teams => rsx! {
                                        Link { to: Route::Teams { cook_and_run_id, team: String::new() }, class: LINK_CLASS, "Teams →" }
                                    },
                                }
                            }
                        }
                    }
                }
            }

            match &data.plan {
                None => rsx! {
                    EmptyState {
                        title: "No plan generated yet",
                        text: "Run the calculation to create the plan for this Cook & Run.",
                        Button {
                            label: "Calculate",
                            loading: calculating,
                            disabled: !problems.is_empty(),
                            onclick: on_calculate,
                        }
                    }
                },
                Some(plan) => rsx! {
                    // New `key` whenever the stored plan changes (recalculated or saved): the board
                    // then starts again from the stored plan instead of keeping an old draft.
                    PlanBoard {
                        key: "{plan_key(plan)}-{generation()}",
                        cook_and_run_id,
                        bundle: data.clone(),
                        plan: plan.clone(),
                        // Rarely needed, so they live in the "More" menu instead of being buttons.
                        menu_items: rsx! {
                            MenuItem {
                                label: if calculating { "Recalculating…" } else { "Recalculate from scratch…" },
                                danger: true,
                                disabled: !problems.is_empty() || calculating,
                                onclick: on_calculate,
                            }
                            Link {
                                to: Route::Schedules { cook_and_run_id },
                                class: menu_item_class(false),
                                role: "menuitem",
                                "Design the schedules →"
                            }
                        },
                    }
                    if not_in_plan > 0 {
                        p { class: "text-xs text-zinc-400",
                            "{not_in_plan} team(s) are not part of this plan. Place them on the board or recalculate."
                        }
                    }
                },
            }
        }
        if confirm() {
            Dialog { title: "Recalculate the plan?", onclose: move |_| confirm.set(false),
                p { class: "text-sm text-zinc-700",
                    "The plan is replaced by a newly calculated one. All changes made by hand are lost."
                }
                div { class: "flex flex-wrap justify-end gap-2",
                    Button { label: "Keep the plan", variant: ButtonVariant::Secondary, onclick: move |_| confirm.set(false) }
                    Button {
                        label: "Recalculate",
                        variant: ButtonVariant::Danger,
                        onclick: move |_| {
                            confirm.set(false);
                            run_for_confirm();
                        },
                    }
                }
            }
        }
    }
}

const LINK_CLASS: &str = "text-xs font-medium text-amber-700 hover:underline";

/// Identifies the stored plan: changes whenever the plan changes, equal for equal plans.
fn plan_key(plan: &PlanData) -> u64 {
    let mut hasher = DefaultHasher::new();
    format!("{plan:?}").hash(&mut hasher);
    hasher.finish()
}
