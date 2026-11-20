//! Data shared by the planning pages (Calculation, Plan Editor, Schedules): loaded in one go and
//! cheap (the short form of the teams only; no per-team requests in the cloud).

use std::collections::HashMap;

use chrono::NaiveDate;
use uuid::Uuid;

use crate::data::Storage;
use crate::planning::{Context, CourseInfo, TeamInfo, DEFAULT_PERSONS};
use crate::storage::{
    CourseData, MeetingPointData, PlanData, TeamMetaData, TeamStatus,
};
use crate::time::to_local;

#[derive(Clone, PartialEq)]
pub(crate) struct PlanBundle {
    /// Name of the project (part of the file names of downloaded sheets).
    pub project_name: String,
    /// Day of the event (local time).
    pub date: NaiveDate,
    pub courses: Vec<CourseData>,
    pub start: Option<MeetingPointData>,
    pub end: Option<MeetingPointData>,
    /// All teams, canceled ones included (their names are needed to explain the plan).
    pub teams: Vec<TeamMetaData>,
    pub plan: Option<PlanData>,
    pub is_cloud: bool,
}

pub(crate) async fn load_bundle(storage: Storage, id: Uuid) -> Result<PlanBundle, String> {
    let storage = storage.read().await;
    let meta = storage.select_cook_and_run_meta(id).await?;
    Ok(PlanBundle {
        date: to_local(meta.occur).date_naive(),
        project_name: meta.name,
        is_cloud: meta.is_in_cloud,
        courses: storage.select_cook_and_run_course_list(id).await?,
        start: storage.select_cook_and_run_start_point(id).await?,
        end: storage.select_cook_and_run_end_point(id).await?,
        teams: storage.select_cook_and_run_team_meta_list(id).await?,
        plan: storage.select_plan_of_cook_and_run(id).await?,
    })
}

impl PlanBundle {
    /// Teams that take part (canceled teams do not).
    pub fn taking_part(&self) -> impl Iterator<Item = &TeamMetaData> {
        self.teams.iter().filter(|team| team.status != TeamStatus::Canceled)
    }

    /// The input of the rules: teams sorted by name, courses sorted by time.
    pub fn context(&self) -> Context {
        let mut teams: Vec<TeamInfo> = self
            .taking_part()
            .map(|team| TeamInfo {
                id: team.id,
                name: team.name.clone(),
                persons: team.members.unwrap_or(DEFAULT_PERSONS),
            })
            .collect();
        teams.sort_by_key(|team| team.name.to_lowercase());
        let courses = self
            .courses
            .iter()
            .map(|course| CourseInfo { id: course.id, name: course.name.clone(), time: course.time })
            .collect();
        Context::new(teams, courses)
    }

    /// Names of all teams, also of canceled ones.
    pub fn names(&self) -> HashMap<Uuid, String> {
        self.teams.iter().map(|team| (team.id, team.name.clone())).collect()
    }
}

/// Colors of one team: `(background, border, text)` as CSS. The same team has the same color
/// everywhere, so that it can be followed through the courses at a glance.
pub(crate) fn team_palette(ctx: &Context) -> HashMap<Uuid, TeamColors> {
    ctx.teams
        .iter()
        .enumerate()
        .map(|(index, team)| {
            // Golden angle: neighbors in the list get clearly different hues.
            let hue = (index as f32 * 137.508) % 360.0;
            (
                team.id,
                TeamColors {
                    background: format!("hsl({hue:.0} 80% 90%)"),
                    border: format!("hsl({hue:.0} 55% 58%)"),
                    ink: format!("hsl({hue:.0} 45% 22%)"),
                    solid: format!("hsl({hue:.0} 60% 50%)"),
                },
            )
        })
        .collect()
}

#[derive(Clone, PartialEq, Default)]
pub(crate) struct TeamColors {
    pub background: String,
    pub border: String,
    pub ink: String,
    pub solid: String,
}
