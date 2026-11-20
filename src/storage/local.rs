use chrono::Utc;
use uuid::Uuid;

use super::{CookAndRunData, CookAndRunMetaData};

use crate::storage::{
    CookAndRunCreate, CookAndRunMetaUpdate, CourseCreate, CourseData, CourseUpdate,
    MeetingPointData, NoteCreate, NoteData, PlanData, Storage, TeamCreate,
    TeamData, TeamMetaData, TeamStatus, TeamUpdate, MAX_COURSES_PER_PROJECT, MAX_PROJECTS_PER_USER,
    MAX_TEAMS_PER_PROJECT,
};

/// The schema changed with the new backend API, so the data lives under a new key
/// (old data is neither read nor overwritten).
const DATA_KEY: &str = "tcc_data_v2";

/// Local storage holds exactly what the backend can hold, restricted to what makes sense without a
/// server: no share configuration, no access tokens, no audit log, no participant self-service,
/// no email notifications and no plan "stale" tracking.
///
/// # Where the data lives
/// In the browser's `localStorage`: it survives closing the tab, closing the browser and restarting
/// the computer. (Until now the data was in `sessionStorage`, which the browser deletes as soon as
/// the tab or browser is closed; existing session data is moved over, see `migrate_from_session`.)
///
/// # Several tabs
/// `localStorage` is shared by all tabs of the app, but every tab keeps its own copy in memory.
/// Every write therefore starts by re-reading the stored data (`reload`) and applies the change to
/// that; otherwise a tab with an outdated copy would overwrite what another tab saved.
#[derive(PartialEq, Clone, Debug)]
pub struct LocalStorage {
    storage: web_sys::Storage,
    cook_and_run_data: Vec<CookAndRunData>,
    /// Projects that are stored but cannot be read by this version of the app (e.g. written by a
    /// newer version). They are not shown, but they are written back unchanged instead of being lost.
    unreadable: Vec<serde_json::Value>,
}

/// Writes a warning to the browser console (nowhere else to put it; outside the browser, in tests, it goes to stderr).
fn log_warning(message: String) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::warn_1(&message.into());
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{message}");
}

/// What a stored JSON text contains.
struct Parsed {
    projects: Vec<CookAndRunData>,
    unreadable: Vec<serde_json::Value>,
}

/// Reads the stored list project by project: one project that cannot be read must not make all
/// the others unavailable (or, worse, be overwritten by the next save).
fn parse_projects(raw: &str) -> Result<Parsed, String> {
    let values: Vec<serde_json::Value> =
        serde_json::from_str(raw).map_err(|e| format!("Data could not parse json: {}", e))?;
    let mut parsed = Parsed { projects: Vec::new(), unreadable: Vec::new() };
    for value in values {
        match serde_json::from_value::<CookAndRunData>(value.clone()) {
            Ok(project) => parsed.projects.push(project),
            Err(e) => {
                log_warning(format!("A stored project could not be read and is kept untouched: {e}"));
                parsed.unreadable.push(value);
            }
        }
    }
    Ok(parsed)
}

/// The JSON text that is stored: the readable projects followed by the unreadable ones.
fn serialize_projects(
    projects: &[CookAndRunData],
    unreadable: &[serde_json::Value],
) -> Result<String, String> {
    let mut values = Vec::with_capacity(projects.len() + unreadable.len());
    for project in projects {
        values.push(
            serde_json::to_value(project)
                .map_err(|e| format!("Struct could not be parsed into json: {}", e))?,
        );
    }
    values.extend(unreadable.iter().cloned());
    serde_json::to_string(&values).map_err(|e| format!("Struct could not be parsed into json: {}", e))
}

/// Adds the projects of `incoming` that `projects` does not have yet (by id). Returns whether
/// something was added. Existing projects always win: the data in `localStorage` is newer than
/// what an old session left behind.
fn merge_projects(projects: &mut Vec<CookAndRunData>, incoming: Vec<CookAndRunData>) -> bool {
    let mut added = false;
    for project in incoming {
        if !projects.iter().any(|existing| existing.id == project.id) {
            projects.push(project);
            added = true;
        }
    }
    added
}

impl CourseCreate {
    pub fn to_local(&self, course_id: Uuid) -> CourseData {
        CourseData {
            id: course_id,
            name: self.name.clone(),
            time: self.time,
        }
    }
}

impl CourseUpdate {
    pub fn to_local(&self, course_id: Uuid) -> CourseData {
        CourseData {
            id: course_id,
            name: self.name.clone(),
            time: self.time,
        }
    }
}

impl TeamCreate {
    pub fn to_local(&self, team_id: Uuid) -> TeamData {
        let now = Utc::now().naive_utc();
        TeamData {
            id: team_id,
            name: self.name.clone(),
            created: now,
            edited: now,
            address: self.address.clone(),
            mail: self.mail.clone(),
            phone: self.phone.clone(),
            members: self.members,
            diets: self.diets.clone(),
            status: if self.needs_check {
                TeamStatus::Review
            } else {
                TeamStatus::Active
            },
            cloud: None,
            note_list: vec![],
        }
    }
}

impl CookAndRunCreate {
    fn to_cook_and_run_data(&self, cook_and_run_id: Uuid) -> CookAndRunData {
        // The admin notification email only matters for the backend and is not stored locally.
        CookAndRunData::new(cook_and_run_id, self.name.clone())
    }
}

fn project_not_found(id: Uuid) -> String {
    format!("Cook and run project with ID {} not found", id)
}

fn team_not_found(team_id: Uuid, cook_and_run_id: Uuid) -> String {
    format!(
        "Team with ID {} not found in Cook and Run project {}",
        team_id, cook_and_run_id
    )
}

impl LocalStorage {
    pub fn new() -> Result<Self, String> {
        let window = web_sys::window().ok_or_else(|| "No global `window` exists".to_string())?;

        let storage = window
            .local_storage()
            .map_err(|e| {
                format!(
                    "Local storage could not be loaded: {}",
                    e.as_string().unwrap_or_default()
                )
            })?
            .ok_or_else(|| "Local storage is not available".to_string())?;

        let parsed = Self::load_cook_and_run_data(&storage)
            .map_err(|e| format!("Cook and run data could not be loaded: {}", e))?;

        let mut local = LocalStorage {
            storage,
            cook_and_run_data: parsed.projects,
            unreadable: parsed.unreadable,
        };
        if let Ok(Some(session)) = window.session_storage() {
            local.migrate_from_session(&session);
        }
        Ok(local)
    }

    fn load_cook_and_run_data(storage: &web_sys::Storage) -> Result<Parsed, String> {
        let data = storage
            .get_item(DATA_KEY)
            .map_err(|e| {
                format!(
                    "Data could not be loaded: {}",
                    e.as_string().unwrap_or_default()
                )
            })?
            .unwrap_or_else(|| "[]".to_string());

        parse_projects(&data)
    }

    /// Re-reads the stored data. Called before every write (see the type docs: several tabs).
    /// If the stored data cannot be read at all, nothing is written.
    fn reload(&mut self) -> Result<(), String> {
        let parsed = Self::load_cook_and_run_data(&self.storage)?;
        self.cook_and_run_data = parsed.projects;
        self.unreadable = parsed.unreadable;
        Ok(())
    }

    /// One-time move of the data from the old `sessionStorage` location: projects that were only in
    /// the session are added, then the session copy is removed. Nothing is removed unless the data
    /// was stored successfully.
    fn migrate_from_session(&mut self, session: &web_sys::Storage) {
        let Ok(Some(raw)) = session.get_item(DATA_KEY) else { return };
        let old = match parse_projects(&raw) {
            Ok(old) => old,
            Err(e) => {
                log_warning(format!("Old session data was not migrated: {e}"));
                return;
            }
        };
        let added = merge_projects(&mut self.cook_and_run_data, old.projects);
        if added {
            if let Err(e) = self.persist() {
                log_warning(format!("Old session data was not migrated: {e}"));
                // Back to what is really stored; the session copy stays for the next attempt.
                let _ = self.reload();
                return;
            }
        }
        if old.unreadable.is_empty() {
            let _ = session.remove_item(DATA_KEY);
        }
    }

    /// Cheap existence check (the storage manager uses it to decide between local and cloud).
    pub fn contains(&self, id: Uuid) -> bool {
        self.cook_and_run_data.iter().any(|data| data.id == id)
    }

    fn get(&self, id: Uuid) -> Result<&CookAndRunData, String> {
        self.cook_and_run_data
            .iter()
            .find(|data| data.id == id)
            .ok_or_else(|| project_not_found(id))
    }

    /// Writes the in-memory state to the browser storage.
    fn persist(&self) -> Result<(), String> {
        let json = serialize_projects(&self.cook_and_run_data, &self.unreadable)?;

        self.storage.set_item(DATA_KEY, &json).map_err(|e| {
            format!(
                "Data could not be stored (is the browser storage full?): {}",
                e.as_string().unwrap_or_default()
            )
        })
    }

    /// Applies `change` to a copy of the project, stamps `edited` and persists it. If the change or
    /// the persisting fails the stored state stays untouched.
    fn modify<F>(&mut self, id: Uuid, change: F) -> Result<(), String>
    where
        F: FnOnce(&mut CookAndRunData) -> Result<(), String>,
    {
        self.reload()?;
        let index = self
            .cook_and_run_data
            .iter()
            .position(|data| data.id == id)
            .ok_or_else(|| project_not_found(id))?;

        let mut project = self.cook_and_run_data[index].clone();
        change(&mut project)?;
        project.edited = Utc::now().naive_utc();

        let previous = std::mem::replace(&mut self.cook_and_run_data[index], project);
        if let Err(e) = self.persist() {
            self.cook_and_run_data[index] = previous;
            return Err(e);
        }
        Ok(())
    }
}

impl Storage for LocalStorage {
    // ----- project ---------------------------------------------------------------------------

    async fn create_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        cook_and_run: &CookAndRunCreate,
    ) -> Result<(), String> {
        cook_and_run.validate()?;
        self.reload()?;
        if self.contains(cook_and_run_id) {
            return Err(format!(
                "Cook and run project with ID {} already exists",
                cook_and_run_id
            ));
        }
        if self.cook_and_run_data.len() >= MAX_PROJECTS_PER_USER {
            return Err(format!(
                "Project limit of {} reached",
                MAX_PROJECTS_PER_USER
            ));
        }

        self.cook_and_run_data
            .push(cook_and_run.to_cook_and_run_data(cook_and_run_id));
        if let Err(e) = self.persist() {
            self.cook_and_run_data.pop();
            return Err(e);
        }
        Ok(())
    }

    async fn delete_cook_and_run(&mut self, id: Uuid) -> Result<(), String> {
        self.reload()?;
        let index = self
            .cook_and_run_data
            .iter()
            .position(|data| data.id == id)
            .ok_or_else(|| project_not_found(id))?;

        let removed = self.cook_and_run_data.remove(index);
        if let Err(e) = self.persist() {
            self.cook_and_run_data.insert(index, removed);
            return Err(e);
        }
        Ok(())
    }

    async fn select_cook_and_run(&self, id: Uuid) -> Result<CookAndRunData, String> {
        self.get(id).cloned()
    }

    async fn select_cook_and_run_meta(&self, id: Uuid) -> Result<CookAndRunMetaData, String> {
        self.get(id).map(|data| data.to_meta())
    }

    async fn select_cook_and_run_meta_list(&self) -> Result<Vec<CookAndRunMetaData>, String> {
        Ok(self.cook_and_run_data.iter().map(|x| x.to_meta()).collect())
    }

    async fn update_meta_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        cook_and_run_meta: &CookAndRunMetaUpdate,
    ) -> Result<(), String> {
        cook_and_run_meta.validate()?;
        self.modify(cook_and_run_id, |project| {
            project.name = cook_and_run_meta.name.clone();
            project.occur = cook_and_run_meta.occur;
            Ok(())
        })
    }

    // ----- start / end point -----------------------------------------------------------------

    async fn select_cook_and_run_start_point(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<MeetingPointData>, String> {
        Ok(self.get(cook_and_run_id)?.start_point.clone())
    }

    async fn update_start_point_in_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        start_point: &Option<MeetingPointData>,
    ) -> Result<(), String> {
        if let Some(point) = start_point {
            point.validate()?;
        }
        self.modify(cook_and_run_id, |project| {
            project.start_point = start_point.clone();
            Ok(())
        })
    }

    async fn select_cook_and_run_end_point(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<MeetingPointData>, String> {
        Ok(self.get(cook_and_run_id)?.end_point.clone())
    }

    async fn update_end_point_in_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        end_point: &Option<MeetingPointData>,
    ) -> Result<(), String> {
        if let Some(point) = end_point {
            point.validate()?;
        }
        self.modify(cook_and_run_id, |project| {
            project.end_point = end_point.clone();
            Ok(())
        })
    }

    // ----- courses ---------------------------------------------------------------------------

    async fn select_cook_and_run_course_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<CourseData>, String> {
        let mut courses = self.get(cook_and_run_id)?.course_list.clone();
        // The backend returns courses ordered by time.
        courses.sort_by_key(|course| course.time);
        Ok(courses)
    }

    async fn create_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
        course: &CourseCreate,
    ) -> Result<(), String> {
        course.validate()?;
        self.modify(cook_and_run_id, |project| {
            if project.course_list.len() >= MAX_COURSES_PER_PROJECT {
                return Err(format!(
                    "Course limit of {} reached for project {}",
                    MAX_COURSES_PER_PROJECT, cook_and_run_id
                ));
            }
            if project.course_list.iter().any(|c| c.id == course_id) {
                return Err(format!(
                    "Course with ID {} already exists in Cook and Run project {}",
                    course_id, cook_and_run_id
                ));
            }
            project.course_list.push(course.to_local(course_id));
            Ok(())
        })
    }

    async fn update_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
        course: &CourseUpdate,
    ) -> Result<(), String> {
        course.validate()?;
        self.modify(cook_and_run_id, |project| {
            let existing = project
                .course_list
                .iter_mut()
                .find(|c| c.id == course_id)
                .ok_or_else(|| {
                    format!(
                        "Course with ID {} not found in Cook and Run project {}",
                        course_id, cook_and_run_id
                    )
                })?;
            *existing = course.to_local(course_id);
            Ok(())
        })
    }

    async fn delete_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
    ) -> Result<(), String> {
        self.modify(cook_and_run_id, |project| {
            let original_len = project.course_list.len();
            project.course_list.retain(|c| c.id != course_id);
            if project.course_list.len() == original_len {
                return Err(format!(
                    "Course with ID {} not found in Cook and Run project {}",
                    course_id, cook_and_run_id
                ));
            }
            // Keep the plan free of dangling references.
            if let Some(plan) = project.top_plan.as_mut() {
                *plan = plan.remap(|course| (course != course_id).then_some(course), Some);
            }
            Ok(())
        })
    }

    // ----- teams -----------------------------------------------------------------------------

    async fn select_cook_and_run_team_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<TeamData>, String> {
        Ok(self.get(cook_and_run_id)?.team_list.clone())
    }

    async fn select_cook_and_run_team_meta_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<TeamMetaData>, String> {
        Ok(self
            .get(cook_and_run_id)?
            .team_list
            .iter()
            .map(|team| team.to_meta())
            .collect())
    }

    async fn select_cook_and_run_team(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<TeamData, String> {
        self.get(cook_and_run_id)?
            .team_list
            .iter()
            .find(|team| team.id == team_id)
            .cloned()
            .ok_or_else(|| team_not_found(team_id, cook_and_run_id))
    }

    async fn create_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        team: &TeamCreate,
    ) -> Result<(), String> {
        team.validate()?;
        self.modify(cook_and_run_id, |project| {
            if project.team_list.len() >= MAX_TEAMS_PER_PROJECT {
                return Err(format!(
                    "Maximum number of teams exceeded: {} for project {}",
                    MAX_TEAMS_PER_PROJECT, cook_and_run_id
                ));
            }
            if project.team_list.iter().any(|t| t.id == team_id) {
                return Err(format!(
                    "Team with ID {} already exists in Cook and Run project {}",
                    team_id, cook_and_run_id
                ));
            }
            project.team_list.push(team.to_local(team_id));
            Ok(())
        })
    }

    async fn update_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        team: &TeamUpdate,
    ) -> Result<(), String> {
        team.validate()?;
        self.modify(cook_and_run_id, |project| {
            let existing = project
                .team_list
                .iter_mut()
                .find(|t| t.id == team_id)
                .ok_or_else(|| team_not_found(team_id, cook_and_run_id))?;
            if existing.status == TeamStatus::Canceled {
                return Err("Team is canceled and can no longer be edited".to_string());
            }
            // Notes, creation date and id are kept, everything else is overwritten.
            existing.name = team.name.clone();
            existing.address = team.address.clone();
            existing.mail = team.mail.clone();
            existing.phone = team.phone.clone();
            existing.members = team.members;
            existing.diets = team.diets.clone();
            existing.status = if team.needs_check {
                TeamStatus::Review
            } else {
                TeamStatus::Active
            };
            existing.edited = Utc::now().naive_utc();
            Ok(())
        })
    }

    async fn delete_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<(), String> {
        self.modify(cook_and_run_id, |project| {
            let original_len = project.team_list.len();
            project.team_list.retain(|t| t.id != team_id);
            if project.team_list.len() == original_len {
                return Err(team_not_found(team_id, cook_and_run_id));
            }
            // Keep the plan free of dangling references.
            if let Some(plan) = project.top_plan.as_mut() {
                *plan = plan.remap(Some, |team| (team != team_id).then_some(team));
            }
            Ok(())
        })
    }

    // ----- notes -----------------------------------------------------------------------------

    async fn select_team_note_list_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<Vec<NoteData>, String> {
        let mut notes = self
            .select_cook_and_run_team(cook_and_run_id, team_id)
            .await?
            .note_list;
        // The backend returns notes ordered by creation time (oldest first).
        notes.sort_by_key(|note| note.created);
        Ok(notes)
    }

    async fn create_team_note_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        note_id: Uuid,
        note_data: &NoteCreate,
    ) -> Result<(), String> {
        note_data.validate()?;
        self.modify(cook_and_run_id, |project| {
            let team = project
                .team_list
                .iter_mut()
                .find(|t| t.id == team_id)
                .ok_or_else(|| team_not_found(team_id, cook_and_run_id))?;

            if team.note_list.iter().any(|n| n.id == note_id) {
                return Err(format!(
                    "Note with ID {} already exists in Team {} of Cook and Run project {}",
                    note_id, team_id, cook_and_run_id
                ));
            }
            team.note_list.push(note_data.to_note(note_id));
            Ok(())
        })
    }

    async fn delete_team_note_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        note_id: Uuid,
    ) -> Result<(), String> {
        self.modify(cook_and_run_id, |project| {
            let team = project
                .team_list
                .iter_mut()
                .find(|t| t.id == team_id)
                .ok_or_else(|| team_not_found(team_id, cook_and_run_id))?;

            let original_len = team.note_list.len();
            team.note_list.retain(|n| n.id != note_id);
            if team.note_list.len() == original_len {
                return Err(format!(
                    "Note with ID {} not found in Team {} of Cook and Run project {}",
                    note_id, team_id, cook_and_run_id
                ));
            }
            Ok(())
        })
    }

    // ----- plan ------------------------------------------------------------------------------

    async fn select_plan_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<PlanData>, String> {
        Ok(self.get(cook_and_run_id)?.top_plan.clone())
    }

    async fn update_plan_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        plan: &PlanData,
    ) -> Result<(), String> {
        plan.validate()?;
        self.modify(cook_and_run_id, |project| {
            let mut plan = plan.clone();
            // Local plans are never "stale"; that marker is maintained by the backend only.
            plan.stale_since = None;
            project.top_plan = Some(plan);
            Ok(())
        })
    }

    async fn delete_plan_of_cook_and_run(&mut self, cook_and_run_id: Uuid) -> Result<(), String> {
        self.modify(cook_and_run_id, |project| {
            if project.top_plan.is_none() {
                return Err(format!(
                    "Plan not found in Cook and Run project with ID {}",
                    cook_and_run_id
                ));
            }
            project.top_plan = None;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(n: u128, name: &str) -> CookAndRunData {
        CookAndRunData::new(Uuid::from_u128(n), name.to_string())
    }

    #[test]
    fn stored_text_round_trips() {
        let projects = vec![project(1, "A"), project(2, "B")];
        let json = serialize_projects(&projects, &[]).unwrap();
        let parsed = parse_projects(&json).unwrap();
        assert_eq!(parsed.projects, projects);
        assert!(parsed.unreadable.is_empty());
    }

    #[test]
    fn one_unreadable_project_does_not_hide_or_destroy_the_others() {
        let good = project(1, "Good");
        let broken = serde_json::json!({ "id": "not-a-project", "unexpected": true });
        let json = serde_json::to_string(&vec![serde_json::to_value(&good).unwrap(), broken.clone()]).unwrap();

        let parsed = parse_projects(&json).unwrap();
        assert_eq!(parsed.projects, vec![good.clone()]);
        assert_eq!(parsed.unreadable, vec![broken.clone()]);

        // Saving writes the unreadable project back untouched.
        let saved = serialize_projects(&parsed.projects, &parsed.unreadable).unwrap();
        let again = parse_projects(&saved).unwrap();
        assert_eq!(again.projects, vec![good]);
        assert_eq!(again.unreadable, vec![broken]);
    }

    #[test]
    fn text_that_is_not_a_list_is_an_error_and_never_an_empty_list() {
        assert!(parse_projects("{\"a\": 1}").is_err());
        assert!(parse_projects("not json").is_err());
        assert!(parse_projects("[]").unwrap().projects.is_empty());
    }

    #[test]
    fn merging_adds_only_new_projects_and_keeps_existing_ones() {
        let mut stored = vec![project(1, "Stored version")];
        let added = merge_projects(&mut stored, vec![project(1, "Old session version"), project(2, "Only in session")]);
        assert!(added);
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].name, "Stored version");
        assert_eq!(stored[1].name, "Only in session");

        assert!(!merge_projects(&mut stored, vec![project(2, "Again")]));
        assert_eq!(stored.len(), 2);
    }
}
