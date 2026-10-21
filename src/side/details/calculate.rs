//! Plan configuration, schedule preview and the calculated walking path.

use std::collections::{HashMap, HashSet};

use chrono::{NaiveDate, NaiveTime};
use dioxus::prelude::*;
use uuid::Uuid;

use super::run_schedule::{run_schedule::RunSchedule, Schedule};
use crate::calculator::{
    Calculator, Course as CalcCourse, DataMapper, Point, SchemaCalculator, Team as CalcTeam,
};
use crate::data::use_storage;
use crate::storage::{
    CourseData, Language, MeetingPointData, PlanConfigData, PlanData, TeamMetaData, TeamStatus,
};
use crate::ui::*;
use crate::Route;

/// Limits of `SchemaCalculator` (it refuses anything else).
const MIN_TEAMS: usize = 9;
const REQUIRED_COURSES: usize = 3;

const DATE_FORMAT: &str = "%Y-%m-%d";

/// Everything this page needs, loaded in one go. Teams come as *short form* (one cheap request);
/// the full data (coordinates) is only fetched when the calculation actually runs.
#[derive(Clone, PartialEq)]
struct CalcData {
    config: PlanConfigData,
    courses: Vec<CourseData>,
    start: Option<MeetingPointData>,
    end: Option<MeetingPointData>,
    teams: Vec<TeamMetaData>,
    plan: Option<PlanData>,
}

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
            let storage = storage.read().await;
            Ok::<_, String>(CalcData {
                config: storage.select_plan_config_of_cook_and_run(id).await?.unwrap_or_default(),
                courses: storage.select_cook_and_run_course_list(id).await?,
                start: storage.select_cook_and_run_start_point(id).await?,
                end: storage.select_cook_and_run_end_point(id).await?,
                teams: storage.select_cook_and_run_team_meta_list(id).await?,
                plan: storage.select_plan_of_cook_and_run(id).await?,
            })
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
fn CalculateContent(cook_and_run_id: Uuid, data: CalcData) -> Element {
    // The draft is shared: the settings card edits it, the preview shows it live.
    let draft = use_signal(|| data.config.clone());

    rsx! {
        PageShell {
            PageHeader { title: "Calculation" }

            div { class: "grid grid-cols-1 xl:grid-cols-2 gap-6 items-start",
                SettingsCard { cook_and_run_id, saved_config: data.config.clone(), draft }
                PreviewCard { draft, has_courses: !data.courses.is_empty() }
            }

            PlanSection { cook_and_run_id, data }
        }
    }
}

// ─────────────────────────────────────────────
//  Settings
// ─────────────────────────────────────────────

#[component]
fn SettingsCard(
    cook_and_run_id: Uuid,
    saved_config: PlanConfigData,
    draft: Signal<PlanConfigData>,
) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let flash = use_flash();
    let save_task = use_busy();
    let mut saved = use_signal(|| saved_config);

    let current = draft.read().clone();

    // Field errors come from the storage validation; the other field is neutralized
    // so that only one message is produced at a time.
    let title_error = {
        let mut probe = current.clone();
        probe.description = String::new();
        probe.validate().err().unwrap_or_default()
    };
    let description_error = {
        let mut probe = current.clone();
        probe.title = "Plan".to_string();
        probe.validate().err().unwrap_or_default()
    };
    let can_save = current != *saved.read() && title_error.is_empty() && description_error.is_empty();

    let on_save = move |_| {
        let config = draft.peek().clone();
        save_task.run(async move {
            let result = {
                storage.write().await.update_plan_config_of_cook_and_run(cook_and_run_id, &config).await
            };
            match result {
                Ok(()) => {
                    saved.set(config);
                    flash.trigger();
                }
                Err(e) => toasts.error("Saving the plan settings failed", e),
            }
        });
    };

    rsx! {
        Card { title: "Plan Configuration", flash: flash.is_active(),
            Field { label: "Title", error: title_error.clone(),
                TextInput {
                    value: current.title.clone(),
                    placeholder: "Plan title",
                    error: !title_error.is_empty(),
                    oninput: move |e: Event<FormData>| draft.write().title = e.value(),
                }
            }
            Field {
                label: "Description",
                error: description_error.clone(),
                hint: "Markdown is supported.".to_string(),
                TextArea {
                    value: current.description.clone(),
                    placeholder: "Short description",
                    rows: 3,
                    error: !description_error.is_empty(),
                    oninput: move |e: Event<FormData>| draft.write().description = e.value(),
                }
            }
            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                Field { label: "Date",
                    TextInput {
                        kind: InputKind::Date,
                        value: current.date.format(DATE_FORMAT).to_string(),
                        oninput: move |e: Event<FormData>| {
                            if let Ok(date) = NaiveDate::parse_from_str(&e.value(), DATE_FORMAT) {
                                draft.write().date = date;
                            }
                        },
                    }
                }
                Field { label: "Language",
                    Select {
                        value: current.language.to_string(),
                        options: vec![("eng", "English"), ("deu", "German")],
                        onchange: move |value: String| draft.write().language = Language::from_string(value),
                    }
                }
            }
            div {
                Button {
                    label: "Save Settings",
                    loading: save_task.is_busy(),
                    disabled: !can_save,
                    onclick: on_save,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Preview
// ─────────────────────────────────────────────

#[component]
fn PreviewCard(draft: Signal<PlanConfigData>, has_courses: bool) -> Element {
    rsx! {
        Card { title: "Schedule Preview", padded: false,
            if has_courses {
                div { class: "p-4",
                    RunSchedule {
                        plan_config: draft.read().clone(),
                        schedule: Schedule::default(false, true, 3, 2, 2, true, true, true, true),
                    }
                }
            } else {
                div { class: "p-4",
                    EmptyState {
                        title: "No courses configured yet",
                        text: "Add at least one course to see the schedule preview.",
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Plan: prerequisites, calculation, table
// ─────────────────────────────────────────────

/// Why the plan cannot be (re)calculated right now. Derived from the data on every render.
fn blockers(data: &CalcData) -> Vec<String> {
    let mut found = Vec::new();

    if data.end.is_none() {
        found.push("No end point configured. Set one on the Start & End page.".to_string());
    }
    if data.courses.len() != REQUIRED_COURSES {
        found.push(format!(
            "Exactly {REQUIRED_COURSES} courses are required (currently {}).",
            data.courses.len()
        ));
    }
    let taking_part = data.teams.iter().filter(|team| team.status != TeamStatus::Canceled).count();
    if taking_part < MIN_TEAMS {
        found.push(format!(
            "At least {MIN_TEAMS} teams are required (currently {taking_part}, canceled teams do not count)."
        ));
    }

    let times: Vec<NaiveTime> = data.courses.iter().map(|course| course.time).collect();
    if let (Some(first), Some(last)) = (times.iter().min(), times.iter().max()) {
        if let Some(start) = &data.start
            && start.time > *first
        {
            found.push("The start point time must be before all course times.".to_string());
        }
        if let Some(end) = &data.end
            && end.time < *last
        {
            found.push("The end point time must be after all course times.".to_string());
        }
    }
    found
}

#[component]
fn PlanSection(cook_and_run_id: Uuid, data: CalcData) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let calc_task = use_busy();

    let problems = blockers(&data);
    let stale = data.plan.as_ref().is_some_and(PlanData::is_stale);
    let has_plan = data.plan.is_some();

    let calc_data = data.clone();
    let on_calculate = move |_| {
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
                    storage.invalidate();
                    toasts.success("Plan calculated", "The walking path was updated.");
                }
                Err(e) => toasts.error("Saving the plan failed", e),
            }
        });
    };

    let calculate_button = rsx! {
        Button {
            label: if has_plan { "Recalculate" } else { "Calculate" },
            loading: calc_task.is_busy(),
            disabled: !problems.is_empty(),
            onclick: on_calculate,
        }
    };

    rsx! {
        div { class: "space-y-4",
            if !problems.is_empty() {
                Card { title: "The plan cannot be calculated yet", tone: Tone::Danger,
                    ul { class: "list-disc pl-5 space-y-1 text-sm text-zinc-700",
                        for problem in problems.iter() {
                            li { key: "{problem}", "{problem}" }
                        }
                    }
                    div { class: "flex flex-wrap gap-2",
                        Link {
                            to: Route::StartEnd { cook_and_run_id },
                            class: button_class(ButtonVariant::Secondary),
                            "Start & End"
                        }
                        Link {
                            to: Route::Courses { cook_and_run_id },
                            class: button_class(ButtonVariant::Secondary),
                            "Courses"
                        }
                        Link {
                            to: Route::Teams { cook_and_run_id },
                            class: button_class(ButtonVariant::Secondary),
                            "Teams"
                        }
                    }
                }
            }

            match &data.plan {
                None => rsx! {
                    EmptyState {
                        title: "No plan generated yet",
                        text: "Run the calculation to create a walking-path plan for this Cook & Run.",
                        {calculate_button}
                    }
                },
                Some(plan) => rsx! {
                    if stale {
                        div { class: "rounded-2xl border border-amber-300 bg-amber-50 px-5 py-4 text-sm text-amber-900",
                            "The plan is outdated: start/end point or teams changed since it was calculated. Recalculate it to apply the changes."
                        }
                    }
                    div { class: "flex flex-wrap items-end justify-between gap-3",
                        div {
                            Headline2 { headline: "Walking path" }
                            p { class: "text-zinc-500 text-sm mt-0.5",
                                "{data.teams.len()} Teams  ·  {data.courses.len()} Courses"
                            }
                        }
                        {calculate_button}
                    }
                    PlanTable {
                        cook_and_run_id,
                        plan: plan.clone(),
                        teams: data.teams.clone(),
                        courses: data.courses.clone(),
                    }
                },
            }
        }
    }
}

/// Who visits whom: team id → (course id → host team id). A host visits itself.
fn visits(plan: &PlanData) -> HashMap<Uuid, HashMap<Uuid, Uuid>> {
    let mut visits: HashMap<Uuid, HashMap<Uuid, Uuid>> = HashMap::new();
    for hosting in &plan.hosting_list {
        for team in std::iter::once(hosting.host).chain(hosting.guest_list.iter().copied()) {
            visits.entry(team).or_default().insert(hosting.name, hosting.host);
        }
    }
    visits
}

#[component]
fn PlanTable(
    cook_and_run_id: Uuid,
    plan: PlanData,
    teams: Vec<TeamMetaData>,
    courses: Vec<CourseData>,
) -> Element {
    // Resolved from the short form: names are all the table shows.
    let visits = visits(&plan);
    let names: HashMap<Uuid, &str> = teams.iter().map(|team| (team.id, team.name.as_str())).collect();
    let name_of = |id: &Uuid| names.get(id).copied().unwrap_or("(deleted team)").to_string();

    let mut columns = courses.clone();
    columns.sort_by_key(|course| course.time);

    let mut rows: Vec<(Uuid, String)> = visits.keys().map(|id| (*id, name_of(id))).collect();
    rows.sort_by(|a, b| a.1.cmp(&b.1));

    let grid = format!(
        "grid-template-columns: minmax(200px, 260px) repeat({}, minmax(160px, 1fr));",
        columns.len()
    );
    let not_in_plan = teams
        .iter()
        .filter(|team| team.status != TeamStatus::Canceled && !visits.contains_key(&team.id))
        .count();
    let head = "text-[11px] font-bold tracking-[0.18em] uppercase text-amber-700/60";

    rsx! {
        div { class: "overflow-x-auto rounded-2xl border border-amber-200 shadow-sm",
            div { class: "min-w-max w-full",
                div { class: "grid border-b border-amber-200 bg-amber-50", style: "{grid}",
                    div { class: "px-5 py-3.5 flex items-center gap-2.5",
                        div { class: "w-1.5 h-4 rounded-full bg-amber-400/70" }
                        span { class: "{head}", "Team" }
                    }
                    for column in columns.iter() {
                        div { key: "{column.id}", class: "px-5 py-3.5 border-l border-amber-200 flex items-center",
                            span { class: "{head}", "{column.name}" }
                        }
                    }
                }

                for (index, (team_id, team_name)) in rows.iter().enumerate() {
                    Link {
                        key: "{team_id}",
                        to: Route::Plan { cook_and_run_id, team_id: *team_id },
                        class: "grid border-b border-amber-200/70 last:border-b-0 transition-colors duration-100 \
                                hover:bg-amber-100/70 cursor-pointer {zebra(index)}",
                        style: "{grid}",

                        div { class: "px-5 py-4 border-r border-amber-200/70 flex items-start gap-3",
                            div { class: "mt-1.5 w-6 h-6 shrink-0 rounded-md bg-brand flex items-center justify-center",
                                span { class: "text-white text-[10px] font-bold", "{index + 1}" }
                            }
                            span { class: "mt-0.5 font-semibold text-sm leading-snug truncate text-zinc-800",
                                "{team_name}"
                            }
                        }

                        for column in columns.iter() {
                            PathCell {
                                key: "{column.id}",
                                host: visits
                                    .get(team_id)
                                    .and_then(|per_course| per_course.get(&column.id))
                                    .map(|host_id| (name_of(host_id), host_id == team_id)),
                            }
                        }
                    }
                }
            }
        }
        if not_in_plan > 0 {
            p { class: "text-xs text-zinc-400",
                "{not_in_plan} team(s) are not part of this plan. Recalculate to include them."
            }
        }
    }
}

fn zebra(index: usize) -> &'static str {
    if index % 2 == 0 { "bg-page" } else { "bg-surface" }
}

/// `host`: name of the hosting team and whether it is the team of the row itself.
#[component]
fn PathCell(host: Option<(String, bool)>) -> Element {
    rsx! {
        div { class: "px-4 py-4 border-l border-amber-200/70 flex flex-col justify-center gap-1",
            match host {
                Some((name, is_own)) => {
                    let tone = if is_own {
                        ("border-brand-hover", "text-brand-hover")
                    } else {
                        ("border-amber-200", "text-zinc-700")
                    };
                    rsx! {
                        div { class: "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border {tone.0} w-fit max-w-full",
                            span { class: "text-xs font-semibold truncate {tone.1}", "{name}" }
                        }
                    }
                }
                None => rsx! {
                    span { class: "text-amber-300 text-base select-none", "—" }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::HostingData;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn visits_lists_the_host_and_every_guest_per_course() {
        let (starter, main) = (id(100), id(101));
        let plan = PlanData {
            hosting_list: vec![
                HostingData { name: starter, host: id(1), guest_list: vec![id(2), id(3)] },
                HostingData { name: main, host: id(2), guest_list: vec![id(1), id(3)] },
            ],
            stale_since: None,
        };
        let visits = visits(&plan);

        assert_eq!(visits.len(), 3);
        // A host visits itself.
        assert_eq!(visits[&id(1)][&starter], id(1));
        assert_eq!(visits[&id(2)][&starter], id(1));
        assert_eq!(visits[&id(3)][&starter], id(1));
        assert_eq!(visits[&id(1)][&main], id(2));
        assert_eq!(visits[&id(3)][&main], id(2));
        assert_eq!(visits[&id(3)].len(), 2);
    }

    #[test]
    fn an_empty_plan_has_no_visits() {
        assert!(visits(&PlanData { hosting_list: vec![], stale_since: None }).is_empty());
    }
}
