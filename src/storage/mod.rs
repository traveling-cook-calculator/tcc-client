mod cloud;
mod local;
pub mod mapper;

use std::{collections::HashMap, f64::consts::PI, hash::Hash};

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use web_sys::console;

use crate::{
    keycloak::AuthState,
    storage::{cloud::CloudStorage, local::LocalStorage},
};

// ---------------------------------------------------------------------------
// Limits of the backend (see swagger.yml). Local storage enforces the same
// limits so that everything stored locally can later be uploaded.
// ---------------------------------------------------------------------------

pub const MAX_PROJECTS_PER_USER: usize = 255;
pub const MAX_TEAMS_PER_PROJECT: usize = 255;
pub const MAX_COURSES_PER_PROJECT: usize = 9;
pub const MIN_GUESTS_PER_HOSTING: usize = 1;
pub const MAX_GUESTS_PER_HOSTING: usize = 5;

const MAX_NAME_LEN: usize = 200;
const MAX_ADDRESS_LEN: usize = 500;
const MAX_PHONE_LEN: usize = 50;
const MAX_DIETS_LEN: usize = 500;
const MAX_MEMBERS: u16 = 10_000;
const MAX_NOTE_CONTENT_LEN: usize = 50_000;
const MAX_PLAN_DESCRIPTION_LEN: usize = 2_000;
const MAX_INVITE_TEXT_LEN: usize = 5_000;
const MAX_CANCEL_REASON_LEN: usize = 1_000;
const MAX_SHARE_MAX_TEAMS: u32 = 128;

// ---------------------------------------------------------------------------
// Validation helpers (the backend validates the same rules, we fail early)
// ---------------------------------------------------------------------------

fn check_text(field: &str, value: &str, min: usize, max: usize) -> Result<(), String> {
    let len = value.chars().count();
    if len < min || len > max {
        Err(format!(
            "{} must contain between {} and {} characters (got {})",
            field, min, max, len
        ))
    } else {
        Ok(())
    }
}

fn check_optional_text(field: &str, value: &Option<String>, max: usize) -> Result<(), String> {
    match value {
        Some(value) => check_text(field, value, 0, max),
        None => Ok(()),
    }
}

fn check_email(field: &str, value: &str) -> Result<(), String> {
    let valid = match value.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && !domain.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !value.chars().any(char::is_whitespace)
        }
        None => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("{} is not a valid email address", field))
    }
}

fn validate_team_fields(
    name: &str,
    address: &AddressData,
    mail: &Option<String>,
    phone: &Option<String>,
    members: &Option<u16>,
    diets: &Option<String>,
) -> Result<(), String> {
    check_text("Team name", name, 1, MAX_NAME_LEN)?;
    address.validate()?;
    if let Some(mail) = mail {
        check_email("Team mail", mail)?;
    }
    check_optional_text("Team phone", phone, MAX_PHONE_LEN)?;
    if let Some(members) = members {
        if *members < 1 || *members > MAX_MEMBERS {
            return Err(format!(
                "Team members must be between 1 and {} (got {})",
                MAX_MEMBERS, members
            ));
        }
    }
    check_optional_text("Team diets", diets, MAX_DIETS_LEN)?;
    Ok(())
}

/// Serde helper: times of day are `HH:MM` on the wire (the backend rejects `HH:MM:SS`).
/// Deserialization additionally accepts `HH:MM:SS[.f]` so that older local data and exports still load.
mod hhmm {
    use chrono::NaiveTime;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(time: &NaiveTime, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&time.format("%H:%M").to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NaiveTime, D::Error> {
        let raw = String::deserialize(deserializer)?;
        NaiveTime::parse_from_str(&raw, "%H:%M")
            .or_else(|_| NaiveTime::parse_from_str(&raw, "%H:%M:%S%.f"))
            .map_err(|e| serde::de::Error::custom(format!("invalid time '{}': {}", raw, e)))
    }
}

// ---------------------------------------------------------------------------
// StorageManager
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct StorageManager {
    local: LocalStorage,
    cloud: Option<CloudStorage>,
}

/// Runs a `&mut self` storage method on the local storage if the project lives there, otherwise in the cloud.
macro_rules! on_project_mut {
    ($this:ident, $id:expr, $method:ident $(, $arg:expr)* $(,)?) => {{
        let project_id: Uuid = $id;
        if $this.local.contains(project_id) {
            $this.local.$method(project_id $(, $arg)*).await
        } else {
            $this.cloud_for_mut(project_id)?.$method(project_id $(, $arg)*).await
        }
    }};
}

/// Same as `on_project_mut!` for `&self` methods.
macro_rules! on_project {
    ($this:ident, $id:expr, $method:ident $(, $arg:expr)* $(,)?) => {{
        let project_id: Uuid = $id;
        if $this.local.contains(project_id) {
            $this.local.$method(project_id $(, $arg)*).await
        } else {
            $this.cloud_for(project_id)?.$method(project_id $(, $arg)*).await
        }
    }};
}

/// Copies a whole project into `target` (local or cloud) with fresh ids and returns the new project id.
///
/// * Course and team ids are regenerated; plan references are remapped.
/// * Canceled teams are not transferred (they can neither be created locally nor in the cloud);
///   hostings that reference them are dropped from the plan.
/// * If anything fails the partially created project is removed again.
async fn transfer<T: Storage>(
    target: &mut T,
    source: CookAndRunData,
    admin_notification_email: Option<String>,
) -> Result<Uuid, String> {
    let new_id = Uuid::new_v4();
    let create = CookAndRunCreate {
        name: source.name.clone(),
        admin_notification_email: admin_notification_email.clone(),
    };
    target
        .create_cook_and_run(new_id, &create)
        .await
        .map_err(|e| {
            format!(
                "Error while creating the project in the target storage: {}",
                e
            )
        })?;

    if let Err(e) = fill_project(target, new_id, source, admin_notification_email).await {
        let e = format!("Error while transferring the project: {}", e);
        return match target.delete_cook_and_run(new_id).await {
            Ok(()) => Err(e),
            Err(rollback) => Err(format!(
                "{} (removing the partially created project failed as well: {})",
                e, rollback
            )),
        };
    }
    Ok(new_id)
}

async fn fill_project<T: Storage>(
    target: &mut T,
    id: Uuid,
    source: CookAndRunData,
    admin_notification_email: Option<String>,
) -> Result<(), String> {
    target
        .update_meta_of_cook_and_run(
            id,
            &CookAndRunMetaUpdate {
                name: source.name.clone(),
                occur: source.occur,
                admin_notification_email,
            },
        )
        .await?;

    let mut course_ids: HashMap<Uuid, Uuid> = HashMap::new();
    for course in &source.course_list {
        let new_course_id = Uuid::new_v4();
        target
            .create_course_of_cook_and_run(
                id,
                new_course_id,
                &CourseCreate {
                    name: course.name.clone(),
                    time: course.time,
                },
            )
            .await?;
        course_ids.insert(course.id, new_course_id);
    }

    let mut team_ids: HashMap<Uuid, Uuid> = HashMap::new();
    let mut skipped_canceled = 0;
    for team in &source.team_list {
        if team.status == TeamStatus::Canceled {
            skipped_canceled += 1;
            continue;
        }
        let new_team_id = Uuid::new_v4();
        target
            .create_team_of_cook_and_run(
                id,
                new_team_id,
                &TeamCreate {
                    name: team.name.clone(),
                    address: team.address.clone(),
                    mail: team.mail.clone(),
                    phone: team.phone.clone(),
                    members: team.members,
                    diets: team.diets.clone(),
                    needs_check: team.needs_check(),
                },
            )
            .await?;
        for note in &team.note_list {
            target
                .create_team_note_of_cook_and_run(
                    id,
                    new_team_id,
                    Uuid::new_v4(),
                    &NoteCreate {
                        headline: note.headline.clone(),
                        content: note.content.clone(),
                    },
                )
                .await?;
        }
        team_ids.insert(team.id, new_team_id);
    }
    if skipped_canceled > 0 {
        console::warn_1(
            &format!(
                "{} canceled team(s) were not transferred and were removed from the plan",
                skipped_canceled
            )
            .into(),
        );
    }

    if source.start_point.is_some() {
        target
            .update_start_point_in_cook_and_run(id, &source.start_point)
            .await?;
    }
    if source.end_point.is_some() {
        target
            .update_end_point_in_cook_and_run(id, &source.end_point)
            .await?;
    }
    if let Some(plan_config) = &source.plan_config {
        target
            .update_plan_config_of_cook_and_run(id, plan_config)
            .await?;
    }
    // The plan goes last: saving it clears the stale marker that creating teams would set in the cloud.
    if let Some(plan) = &source.top_plan {
        let mut plan = plan.remap(
            |course| course_ids.get(&course).copied(),
            |team| team_ids.get(&team).copied(),
        );
        plan.stale_since = None;
        target.update_plan_of_cook_and_run(id, &plan).await?;
    }
    Ok(())
}

impl StorageManager {
    pub fn new() -> Result<Self, String> {
        Ok(StorageManager {
            local: LocalStorage::new()?,
            cloud: None,
        })
    }

    /// Connects to the backend with the session of the logged in user.
    pub async fn load_cloud(
        &mut self,
        auth_state: AuthState,
        base_url: String,
    ) -> Result<(), String> {
        let cloud = CloudStorage::new(auth_state, base_url).await;
        match cloud {
            Ok(cloud) => {
                self.cloud = Some(cloud);
                Ok(())
            }
            Err(e) => {
                self.cloud = None;
                Err(format!("Error while loading cloud connection: {}", e))
            }
        }
    }

    /// Connects to the backend without a login. Only the participant operations
    /// (team registration and self-service via access token) are available.
    pub async fn load_cloud_anonymous(&mut self, base_url: String) -> Result<(), String> {
        match CloudStorage::new_anonymous(base_url).await {
            Ok(cloud) => {
                self.cloud = Some(cloud);
                Ok(())
            }
            Err(e) => {
                self.cloud = None;
                Err(format!("Error while loading cloud connection: {}", e))
            }
        }
    }

    pub fn disconnect_cloud(&mut self) {
        self.cloud = None;
    }

    pub fn is_cloud_connected(&self) -> bool {
        self.cloud.is_some()
    }

    fn cloud(&self) -> Result<&CloudStorage, String> {
        self.cloud
            .as_ref()
            .ok_or_else(|| "Cloud is not configured".to_string())
    }

    fn cloud_for(&self, project_id: Uuid) -> Result<&CloudStorage, String> {
        self.cloud.as_ref().ok_or_else(|| {
            format!(
                "Cook and run with id {} not found in local storage and the cloud is not configured",
                project_id
            )
        })
    }

    fn cloud_for_mut(&mut self, project_id: Uuid) -> Result<&mut CloudStorage, String> {
        self.cloud.as_mut().ok_or_else(|| {
            format!(
                "Cook and run with id {} not found in local storage and the cloud is not configured",
                project_id
            )
        })
    }

    // ----- moving projects between local storage and cloud ------------------------------------

    /// Moves a local project to the cloud and returns the id of the new cloud project.
    /// The backend requires an admin notification email for every project.
    pub async fn upload_to_cloud(
        &mut self,
        cook_and_run_id: Uuid,
        admin_notification_email: String,
    ) -> Result<Uuid, String> {
        let cloud = self
            .cloud
            .as_mut()
            .ok_or_else(|| "Cloud is not configured".to_string())?;
        let local = &mut self.local;

        let data = local
            .select_cook_and_run(cook_and_run_id)
            .await
            .map_err(|e| format!("Error while loading cook and run from local storage: {}", e))?;
        let new_id = transfer(cloud, data, Some(admin_notification_email)).await?;
        local
            .delete_cook_and_run(cook_and_run_id)
            .await
            .map_err(|e| {
                format!(
                    "Uploaded as {}, but the local copy could not be removed: {}",
                    new_id, e
                )
            })?;
        Ok(new_id)
    }

    /// Moves a cloud project to the local storage and returns the id of the new local project.
    pub async fn download_from_cloud(&mut self, cook_and_run_id: Uuid) -> Result<Uuid, String> {
        let cloud = self
            .cloud
            .as_mut()
            .ok_or_else(|| "Cloud is not configured".to_string())?;
        let local = &mut self.local;

        let data = cloud
            .select_cook_and_run(cook_and_run_id)
            .await
            .map_err(|e| format!("Error while loading cook and run from cloud: {}", e))?;
        let new_id = transfer(local, data, None).await?;
        cloud
            .delete_cook_and_run(cook_and_run_id)
            .await
            .map_err(|e| {
                format!(
                    "Downloaded as {}, but the cloud copy could not be removed: {}",
                    new_id, e
                )
            })?;
        Ok(new_id)
    }

    pub async fn create_from_file(&mut self, cook_and_run: CookAndRunData) -> Result<Uuid, String> {
        transfer(&mut self.local, cook_and_run, None).await
    }

    // ----- project -----------------------------------------------------------------------------

    /// Projects are always created locally; use `upload_to_cloud` to move them to the cloud.
    pub async fn create_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        cook_and_run: &CookAndRunCreate,
    ) -> Result<(), String> {
        self.local
            .create_cook_and_run(cook_and_run_id, cook_and_run)
            .await
    }

    pub async fn delete_cook_and_run(&mut self, cook_and_run_id: Uuid) -> Result<(), String> {
        on_project_mut!(self, cook_and_run_id, delete_cook_and_run)
    }

    pub async fn update_meta_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        cook_and_run_meta: &CookAndRunMetaUpdate,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            update_meta_of_cook_and_run,
            cook_and_run_meta
        )
    }

    pub async fn select_cook_and_run(&self, id: Uuid) -> Result<CookAndRunData, String> {
        on_project!(self, id, select_cook_and_run)
    }

    pub async fn select_cook_and_run_meta(&self, id: Uuid) -> Result<CookAndRunMetaData, String> {
        on_project!(self, id, select_cook_and_run_meta)
    }

    /// Local projects plus (if connected and logged in) all cloud projects, newest first.
    pub async fn select_cook_and_run_meta_list(&self) -> Result<Vec<CookAndRunMetaData>, String> {
        let local_data = self.local.select_cook_and_run_meta_list().await?;

        let cloud: &CloudStorage = match self.cloud() {
            Ok(cloud) => cloud,
            Err(e) => {
                console::warn_1(&format!("Error when loading cloud connection: {}", e).into());
                return Ok(local_data);
            }
        };

        let cloud_data = match cloud.select_cook_and_run_meta_list().await {
            Ok(data) => data,
            Err(e) => {
                console::warn_1(
                    &format!(
                        "Error when loading all cook and run projects from cloud: {}",
                        e
                    )
                    .into(),
                );
                Vec::new()
            }
        };

        let mut combined_data = HashMap::new();
        for data in cloud_data {
            combined_data.insert(data.id, data);
        }
        for data in local_data {
            combined_data.entry(data.id).or_insert(data);
        }

        let mut list: Vec<CookAndRunMetaData> = combined_data.into_values().collect();
        list.sort_by(|a, b| b.created.cmp(&a.created));
        Ok(list)
    }

    // ----- start / end point -------------------------------------------------------------------

    pub async fn select_cook_and_run_start_point(
        &self,
        id: Uuid,
    ) -> Result<Option<MeetingPointData>, String> {
        on_project!(self, id, select_cook_and_run_start_point)
    }

    pub async fn select_cook_and_run_end_point(
        &self,
        id: Uuid,
    ) -> Result<Option<MeetingPointData>, String> {
        on_project!(self, id, select_cook_and_run_end_point)
    }

    /// `None` removes the start point.
    pub async fn update_start_point_in_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        start_point: &Option<MeetingPointData>,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            update_start_point_in_cook_and_run,
            start_point
        )
    }

    /// `None` removes the end point.
    pub async fn update_end_point_in_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        end_point: &Option<MeetingPointData>,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            update_end_point_in_cook_and_run,
            end_point
        )
    }

    // ----- courses -----------------------------------------------------------------------------

    pub async fn select_cook_and_run_course_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<CourseData>, String> {
        on_project!(self, cook_and_run_id, select_cook_and_run_course_list)
    }

    pub async fn create_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
        course: &CourseCreate,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            create_course_of_cook_and_run,
            course_id,
            course
        )
    }

    pub async fn update_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
        course: &CourseUpdate,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            update_course_of_cook_and_run,
            course_id,
            course
        )
    }

    pub async fn delete_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            delete_course_of_cook_and_run,
            course_id
        )
    }

    // ----- teams -------------------------------------------------------------------------------

    /// Full team data (without notes in the cloud, use `select_cook_and_run_team` for those).
    pub async fn select_cook_and_run_team_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<TeamData>, String> {
        on_project!(self, cook_and_run_id, select_cook_and_run_team_list)
    }

    /// Short form of all teams (one request per 100 teams in the cloud).
    pub async fn select_cook_and_run_team_meta_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<TeamMetaData>, String> {
        on_project!(self, cook_and_run_id, select_cook_and_run_team_meta_list)
    }

    pub async fn select_cook_and_run_team(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<TeamData, String> {
        on_project!(self, cook_and_run_id, select_cook_and_run_team, team_id)
    }

    pub async fn create_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        team: &TeamCreate,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            create_team_of_cook_and_run,
            team_id,
            team
        )
    }

    pub async fn update_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        team: &TeamUpdate,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            update_team_of_cook_and_run,
            team_id,
            team
        )
    }

    pub async fn delete_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<(), String> {
        on_project_mut!(self, cook_and_run_id, delete_team_of_cook_and_run, team_id)
    }

    // ----- notes -------------------------------------------------------------------------------

    pub async fn select_team_note_list_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<Vec<NoteData>, String> {
        on_project!(
            self,
            cook_and_run_id,
            select_team_note_list_of_cook_and_run,
            team_id
        )
    }

    pub async fn create_team_note_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        note_id: Uuid,
        note_data: &NoteCreate,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            create_team_note_of_cook_and_run,
            team_id,
            note_id,
            note_data
        )
    }

    pub async fn delete_team_note_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        note_id: Uuid,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            delete_team_note_of_cook_and_run,
            team_id,
            note_id
        )
    }

    // ----- plan and plan config ----------------------------------------------------------------

    pub async fn select_plan_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<PlanData>, String> {
        on_project!(self, cook_and_run_id, select_plan_of_cook_and_run)
    }

    pub async fn update_plan_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        plan: &PlanData,
    ) -> Result<(), String> {
        on_project_mut!(self, cook_and_run_id, update_plan_of_cook_and_run, plan)
    }

    pub async fn delete_plan_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
    ) -> Result<(), String> {
        on_project_mut!(self, cook_and_run_id, delete_plan_of_cook_and_run)
    }

    pub async fn select_plan_config_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<PlanConfigData>, String> {
        on_project!(self, cook_and_run_id, select_plan_config_of_cook_and_run)
    }

    pub async fn update_plan_config_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        plan_config: &PlanConfigData,
    ) -> Result<(), String> {
        on_project_mut!(
            self,
            cook_and_run_id,
            update_plan_config_of_cook_and_run,
            plan_config
        )
    }

    pub async fn delete_plan_config_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
    ) -> Result<(), String> {
        on_project_mut!(self, cook_and_run_id, delete_plan_config_of_cook_and_run)
    }

    // =========================================================================================
    // Cloud only (no local counterpart): sharing, audit log, plan confirmation, route mails and
    // the participant self-service. All of these require the project to live in the cloud.
    // =========================================================================================

    pub async fn cloud_health(&self) -> Result<HealthData, String> {
        self.cloud()?.health().await
    }

    pub async fn cloud_health_live(&self) -> Result<HealthData, String> {
        self.cloud()?.health_live().await
    }

    pub async fn cloud_health_ready(&self) -> Result<HealthData, String> {
        self.cloud()?.health_ready().await
    }

    pub async fn select_cook_and_run_meta_page(
        &self,
        page: u32,
        limit: u32,
        sort: ProjectSort,
    ) -> Result<Page<CookAndRunMetaData>, String> {
        self.cloud()?
            .select_cook_and_run_meta_page(page, limit, sort)
            .await
    }

    pub async fn select_cook_and_run_team_meta_page(
        &self,
        cook_and_run_id: Uuid,
        page: u32,
        limit: u32,
        sort: TeamSort,
    ) -> Result<Page<TeamMetaData>, String> {
        self.cloud()?
            .select_team_meta_page(cook_and_run_id, page, limit, sort)
            .await
    }

    pub async fn select_team_audit_log_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        page: u32,
        limit: u32,
    ) -> Result<Page<AuditLogEntry>, String> {
        self.cloud()?
            .select_team_audit_log(cook_and_run_id, team_id, page, limit)
            .await
    }

    pub async fn select_cook_and_run_share_config(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<ShareTeamConfig>, String> {
        self.cloud()?
            .select_cook_and_run_share_config(cook_and_run_id)
            .await
    }

    pub async fn create_cook_and_run_share_config(
        &self,
        cook_and_run_id: Uuid,
        share_config: &ShareTeamConfigCreate,
    ) -> Result<(), String> {
        self.cloud()?
            .create_cook_and_run_share_config(cook_and_run_id, share_config)
            .await
    }

    pub async fn update_cook_and_run_share_config(
        &self,
        cook_and_run_id: Uuid,
        share_config: &ShareTeamConfigCreate,
    ) -> Result<(), String> {
        self.cloud()?
            .update_cook_and_run_share_config(cook_and_run_id, share_config)
            .await
    }

    pub async fn delete_cook_and_run_share_config(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<(), String> {
        self.cloud()?
            .delete_cook_and_run_share_config(cook_and_run_id)
            .await
    }

    /// Declares a stale plan as still valid without changing it.
    pub async fn confirm_plan_of_cook_and_run(&self, cook_and_run_id: Uuid) -> Result<(), String> {
        self.cloud()?.confirm_plan(cook_and_run_id).await
    }

    /// Queues the "route updated" mail for the given teams (1 to 128, no duplicates).
    pub async fn send_route_mails_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
        team_ids: &[Uuid],
    ) -> Result<(), String> {
        self.cloud()?
            .send_route_mails(cook_and_run_id, team_ids)
            .await
    }

    /// Public team registration (no login). The share configuration of the project must allow it.
    pub async fn register_team_as_participant(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        team: &TeamRegistration,
    ) -> Result<(), String> {
        self.cloud()?
            .create_team_as_participant(cook_and_run_id, team_id, team)
            .await
    }

    pub async fn select_team_with_access(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
    ) -> Result<TeamData, String> {
        self.cloud()?
            .select_team_with_access(cook_and_run_id, team_id, access)
            .await
    }

    pub async fn update_team_with_access(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
        team: &TeamUpdate,
    ) -> Result<(), String> {
        self.cloud()?
            .update_team_with_access(cook_and_run_id, team_id, access, team)
            .await
    }

    pub async fn verify_team_email(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        access_token: Uuid,
    ) -> Result<(), String> {
        self.cloud()?
            .verify_team_email(cook_and_run_id, team_id, access_token)
            .await
    }

    pub async fn cancel_team(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        access_token: Uuid,
        reason: Option<String>,
    ) -> Result<(), String> {
        self.cloud()?
            .cancel_team(cook_and_run_id, team_id, access_token, reason)
            .await
    }

    pub async fn resend_team_verification(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
    ) -> Result<(), String> {
        self.cloud()?
            .resend_team_verification(cook_and_run_id, team_id, access)
            .await
    }

    pub async fn select_team_plan(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
    ) -> Result<TeamPlanData, String> {
        self.cloud()?
            .select_team_plan(cook_and_run_id, team_id, access)
            .await
    }
}

// ---------------------------------------------------------------------------
// Storage trait: everything that exists locally AND in the cloud.
// Cloud-only features are inherent methods of `CloudStorage` (see cloud.rs).
// ---------------------------------------------------------------------------

pub trait Storage {
    // project
    async fn create_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        cook_and_run: &CookAndRunCreate,
    ) -> Result<(), String>;
    async fn delete_cook_and_run(&mut self, id: Uuid) -> Result<(), String>;
    async fn update_meta_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        cook_and_run_meta: &CookAndRunMetaUpdate,
    ) -> Result<(), String>;
    async fn select_cook_and_run_meta_list(&self) -> Result<Vec<CookAndRunMetaData>, String>;
    async fn select_cook_and_run(&self, id: Uuid) -> Result<CookAndRunData, String>;
    async fn select_cook_and_run_meta(&self, id: Uuid) -> Result<CookAndRunMetaData, String>;

    // start / end point (`None` deletes the point)
    async fn select_cook_and_run_start_point(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<MeetingPointData>, String>;
    async fn update_start_point_in_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        start_point: &Option<MeetingPointData>,
    ) -> Result<(), String>;
    async fn select_cook_and_run_end_point(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<MeetingPointData>, String>;
    async fn update_end_point_in_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        end_point: &Option<MeetingPointData>,
    ) -> Result<(), String>;

    // courses
    async fn select_cook_and_run_course_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<CourseData>, String>;
    async fn create_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
        course: &CourseCreate,
    ) -> Result<(), String>;
    async fn update_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
        course: &CourseUpdate,
    ) -> Result<(), String>;
    async fn delete_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
    ) -> Result<(), String>;

    // teams (administrator view)
    async fn select_cook_and_run_team_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<TeamData>, String>;
    async fn select_cook_and_run_team_meta_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<TeamMetaData>, String>;
    async fn select_cook_and_run_team(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<TeamData, String>;
    async fn create_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        team: &TeamCreate,
    ) -> Result<(), String>;
    async fn update_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        team: &TeamUpdate,
    ) -> Result<(), String>;
    async fn delete_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<(), String>;

    // notes
    async fn select_team_note_list_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<Vec<NoteData>, String>;
    async fn create_team_note_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        note_id: Uuid,
        note_data: &NoteCreate,
    ) -> Result<(), String>;
    async fn delete_team_note_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        note_id: Uuid,
    ) -> Result<(), String>;

    // plan
    async fn select_plan_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<PlanData>, String>;
    async fn update_plan_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        plan: &PlanData,
    ) -> Result<(), String>;
    async fn delete_plan_of_cook_and_run(&mut self, cook_and_run_id: Uuid) -> Result<(), String>;

    // plan config
    async fn select_plan_config_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<PlanConfigData>, String>;
    async fn update_plan_config_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        plan_config: &PlanConfigData,
    ) -> Result<(), String>;
    async fn delete_plan_config_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
    ) -> Result<(), String>;
}

// ---------------------------------------------------------------------------
// Courses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CourseCreate {
    pub name: String,
    #[serde(with = "hhmm")]
    pub time: NaiveTime,
}

impl CourseCreate {
    pub fn validate(&self) -> Result<(), String> {
        check_text("Course name", &self.name, 1, MAX_NAME_LEN)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CourseUpdate {
    pub name: String,
    #[serde(with = "hhmm")]
    pub time: NaiveTime,
}

impl CourseUpdate {
    pub fn validate(&self) -> Result<(), String> {
        check_text("Course name", &self.name, 1, MAX_NAME_LEN)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CourseData {
    pub id: Uuid,
    pub name: String,
    #[serde(with = "hhmm")]
    pub time: NaiveTime,
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

/// One course hosted by one team for 1 to 5 guest teams. Several hostings of the same course
/// (different hosts) mean that the course has several hosts.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostingData {
    /*Course ID*/
    pub name: Uuid,
    /*Team ID */
    pub host: Uuid,
    /*Team ID */
    pub guest_list: Vec<Uuid>,
}

impl HostingData {
    /// The backend does not store hosting ids. This is a stable, derived id (same hosting = same id)
    /// that the UI can use as key.
    pub fn id(&self) -> Uuid {
        Uuid::from_u128(self.name.as_u128() ^ self.host.as_u128().rotate_left(64))
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanData {
    pub hosting_list: Vec<HostingData>,
    /// Cloud only: set when the plan became outdated (start/end point changed, team created,
    /// deleted or canceled, team address changed). Never set for local projects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_since: Option<NaiveDateTime>,
}

impl PlanData {
    pub fn is_stale(&self) -> bool {
        self.stale_since.is_some()
    }

    pub fn validate(&self) -> Result<(), String> {
        for hosting in &self.hosting_list {
            let guests = hosting.guest_list.len();
            if guests < MIN_GUESTS_PER_HOSTING || guests > MAX_GUESTS_PER_HOSTING {
                return Err(format!(
                    "Hosting of course {} by team {} must contain between {} and {} guests (got {})",
                    hosting.name, hosting.host, MIN_GUESTS_PER_HOSTING, MAX_GUESTS_PER_HOSTING, guests
                ));
            }
        }
        Ok(())
    }

    /// Rewrites all course and team ids. Ids for which the closures return `None` are removed:
    /// hostings with an unknown course or host are dropped, unknown guests are removed and hostings
    /// that end up without guests are dropped as well.
    pub fn remap(
        &self,
        course: impl Fn(Uuid) -> Option<Uuid>,
        team: impl Fn(Uuid) -> Option<Uuid>,
    ) -> PlanData {
        let hosting_list = self
            .hosting_list
            .iter()
            .filter_map(|hosting| {
                let name = course(hosting.name)?;
                let host = team(hosting.host)?;
                let guest_list: Vec<Uuid> = hosting
                    .guest_list
                    .iter()
                    .filter_map(|guest| team(*guest))
                    .collect();
                if guest_list.is_empty() {
                    None
                } else {
                    Some(HostingData {
                        name,
                        host,
                        guest_list,
                    })
                }
            })
            .collect();
        PlanData {
            hosting_list,
            stale_since: self.stale_since,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanAccess {
    Link,
    Account,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanConfigData {
    /// Cloud only (who may open the plan). Always empty for local projects.
    #[serde(default)]
    pub access: Vec<PlanAccess>,
    pub title: String,
    pub description: String,
    pub date: NaiveDate,
    pub language: Language,
}

impl PlanConfigData {
    pub fn validate(&self) -> Result<(), String> {
        check_text("Plan title", &self.title, 1, MAX_NAME_LEN)?;
        check_text(
            "Plan description",
            &self.description,
            0,
            MAX_PLAN_DESCRIPTION_LEN,
        )?;
        if self.access.len() > 2 {
            return Err("Plan access must not contain more than 2 entries".to_string());
        }
        Ok(())
    }
}

impl Default for PlanConfigData {
    fn default() -> Self {
        Self {
            access: vec![],
            title: "Plan".to_string(),
            description: "<center>This is the plan for the cook and run event.</center>"
                .to_string(),
            date: Utc::now().naive_utc().date(),
            language: Language::English,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Language {
    English,
    German,
}

impl Language {
    pub fn from_string(value: String) -> Self {
        match value.as_str() {
            "deu" => Language::German,
            _ => Language::English,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Language::German => "deu".to_string(),
            Language::English => "eng".to_string(),
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        Language::English
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let iso = String::deserialize(deserializer)?;
        Self::from_iso639_3(&iso)
            .ok_or_else(|| serde::de::Error::custom(format!("Unknown language code: {}", iso)))
    }
}

impl Serialize for Language {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let iso = self.to_iso639_3();
        serializer.serialize_str(iso)
    }
}

impl Language {
    pub fn to_iso639_3(&self) -> &str {
        match self {
            Language::English => "eng",
            Language::German => "deu",
        }
    }

    pub fn from_iso639_3(iso: &str) -> Option<Self> {
        match iso {
            "eng" => Some(Language::English),
            "deu" => Some(Language::German),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Address, meeting points
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressData {
    pub address: String,
    pub latitude: f64,
    pub longitude: f64,
}

impl PartialEq for AddressData {
    fn eq(&self, other: &Self) -> bool {
        self.address.eq(&other.address)
    }
}

impl Eq for AddressData {}

impl Hash for AddressData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.address.hash(state);
    }
}

impl AddressData {
    fn deg_to_rad(deg: f64) -> f64 {
        deg * PI / 180.0
    }

    pub fn distance(&self, addr: &AddressData) -> f64 {
        let r = 6371.0;

        let dlat = Self::deg_to_rad(addr.latitude - self.latitude);
        let dlon = Self::deg_to_rad(addr.longitude - self.longitude);

        let lat1_rad = Self::deg_to_rad(self.latitude);
        let lat2_rad = Self::deg_to_rad(addr.latitude);

        let a = (dlat / 2.0).sin().powi(2)
            + lat1_rad.cos() * lat2_rad.cos() * (dlon / 2.0).sin().powi(2);

        let c = 2.0 * a.sqrt().asin();

        r * c
    }

    pub fn validate(&self) -> Result<(), String> {
        check_text("Address", &self.address, 1, MAX_ADDRESS_LEN)?;
        if !(-90.0..=90.0).contains(&self.latitude) {
            return Err(format!(
                "Latitude must be between -90 and 90 (got {})",
                self.latitude
            ));
        }
        if !(-180.0..=180.0).contains(&self.longitude) {
            return Err(format!(
                "Longitude must be between -180 and 180 (got {})",
                self.longitude
            ));
        }
        Ok(())
    }
}

impl Default for AddressData {
    fn default() -> Self {
        Self {
            address: "[Address]".to_string(),
            latitude: 0.0,
            longitude: 0.0,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetingPointData {
    pub name: String,
    #[serde(with = "hhmm")]
    pub time: NaiveTime,
    pub address: AddressData,
}

impl MeetingPointData {
    pub fn validate(&self) -> Result<(), String> {
        check_text("Meeting point name", &self.name, 1, MAX_NAME_LEN)?;
        self.address.validate()
    }
}

// ---------------------------------------------------------------------------
// Teams and notes
// ---------------------------------------------------------------------------

/// `active` = approved, `review` = has to be checked by the admin, `canceled` = canceled by the team
/// (cloud only: nobody can cancel locally and canceled teams are not transferred between storages).
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamStatus {
    #[default]
    Active,
    Review,
    Canceled,
}

/// Data that only exists for teams stored in the cloud. Always `None` for local teams.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TeamCloudData {
    /// Secret token of the team (self-service link). Treat as confidential.
    pub access_token: Uuid,
    pub created_by_user: Option<String>,
    pub canceled_at: Option<NaiveDateTime>,
    pub cancel_reason: Option<String>,
    pub email_verified_at: Option<NaiveDateTime>,
    pub last_route_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub struct TeamData {
    pub id: Uuid,
    pub name: String,
    pub created: NaiveDateTime,
    pub edited: NaiveDateTime,
    pub address: AddressData,
    pub mail: Option<String>,
    pub phone: Option<String>,
    pub members: Option<u16>,
    pub diets: Option<String>,
    #[serde(default)]
    pub status: TeamStatus,
    #[serde(default)]
    pub cloud: Option<TeamCloudData>,
    #[serde(default)]
    pub note_list: Vec<NoteData>,
}

impl TeamData {
    pub fn needs_check(&self) -> bool {
        self.status == TeamStatus::Review
    }

    pub fn to_meta(&self) -> TeamMetaData {
        TeamMetaData {
            id: self.id,
            name: self.name.clone(),
            mail: self.mail.is_some(),
            phone: self.phone.is_some(),
            members: self.members,
            diets: self.diets.is_some(),
            created_by_user: self
                .cloud
                .as_ref()
                .and_then(|cloud| cloud.created_by_user.clone()),
            created: self.created,
            edited: self.edited,
            status: self.status,
            canceled_at: self.cloud.as_ref().and_then(|cloud| cloud.canceled_at),
            email_verified: self
                .cloud
                .as_ref()
                .map(|cloud| cloud.email_verified_at.is_some())
                .unwrap_or(false),
        }
    }
}

impl Default for TeamData {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "[Team Name]".to_string(),
            created: Utc::now().naive_utc(),
            edited: Utc::now().naive_utc(),
            address: AddressData::default(),
            mail: Some("[mail address]".to_string()),
            phone: Some("[phone number]".to_string()),
            members: Some(2),
            diets: Some("[some diets]".to_string()),
            status: TeamStatus::Active,
            cloud: None,
            note_list: vec![],
        }
    }
}

/// Short form of a team (list view): contact data is only hinted at.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TeamMetaData {
    pub id: Uuid,
    pub name: String,
    pub mail: bool,
    pub phone: bool,
    pub members: Option<u16>,
    pub diets: bool,
    pub created_by_user: Option<String>,
    pub created: NaiveDateTime,
    pub edited: NaiveDateTime,
    pub status: TeamStatus,
    pub canceled_at: Option<NaiveDateTime>,
    pub email_verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamCreate {
    pub name: String,
    pub address: AddressData,
    pub mail: Option<String>,
    pub phone: Option<String>,
    pub members: Option<u16>,
    pub diets: Option<String>,
    /// `true` creates the team in status `review`, `false` in status `active`.
    pub needs_check: bool,
}

impl TeamCreate {
    pub fn validate(&self) -> Result<(), String> {
        validate_team_fields(
            &self.name,
            &self.address,
            &self.mail,
            &self.phone,
            &self.members,
            &self.diets,
        )
    }
}

/// Full overwrite of the team master data (missing optional values become `None`).
#[derive(Debug, Clone, Serialize)]
pub struct TeamUpdate {
    pub name: String,
    pub address: AddressData,
    pub mail: Option<String>,
    pub phone: Option<String>,
    pub members: Option<u16>,
    pub diets: Option<String>,
    /// `true` sets the status to `review`, `false` to `active`.
    /// Ignored when a participant edits its own team (status is decided by the share configuration).
    pub needs_check: bool,
}

impl TeamUpdate {
    pub fn validate(&self) -> Result<(), String> {
        validate_team_fields(
            &self.name,
            &self.address,
            &self.mail,
            &self.phone,
            &self.members,
            &self.diets,
        )
    }
}

/// Self-registration of a team (cloud only). The access token is chosen by the client and is the
/// only way for the team to edit, cancel or view its plan later.
#[derive(Debug, Clone, Serialize)]
pub struct TeamRegistration {
    pub name: String,
    pub address: AddressData,
    pub mail: Option<String>,
    pub phone: Option<String>,
    pub members: Option<u16>,
    pub diets: Option<String>,
    pub access_token: Uuid,
}

impl TeamRegistration {
    pub fn validate(&self) -> Result<(), String> {
        validate_team_fields(
            &self.name,
            &self.address,
            &self.mail,
            &self.phone,
            &self.members,
            &self.diets,
        )
    }
}

/// How a team is accessed on endpoints that accept an admin login as well as a team token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamAccess {
    /// Bearer token of the logged in project owner.
    Admin,
    /// Secret `x-access-token` of the team (no login, the bearer token is not sent).
    Token(Uuid),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoteCreate {
    pub headline: String,
    pub content: String,
}

impl NoteCreate {
    pub fn to_note(&self, note_id: Uuid) -> NoteData {
        NoteData {
            id: note_id,
            headline: self.headline.clone(),
            content: self.content.clone(),
            created: Utc::now().naive_utc(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        check_text("Note headline", &self.headline, 1, MAX_NAME_LEN)?;
        check_text("Note content", &self.content, 1, MAX_NOTE_CONTENT_LEN)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub struct NoteData {
    pub id: Uuid,
    pub headline: String,
    pub content: String,
    pub created: NaiveDateTime,
}

// ---------------------------------------------------------------------------
// Project
// ---------------------------------------------------------------------------

/// Complete project. In the backend this is called "project".
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CookAndRunData {
    pub id: Uuid,
    pub name: String,
    pub created: NaiveDateTime,
    pub edited: NaiveDateTime,
    pub occur: NaiveDateTime,
    #[serde(default)]
    pub is_in_cloud: bool,
    /// Cloud only. Always `None` for local projects.
    #[serde(default)]
    pub admin_notification_email: Option<String>,
    #[serde(default)]
    pub team_list: Vec<TeamData>,
    #[serde(default)]
    pub course_list: Vec<CourseData>,
    #[serde(default)]
    pub start_point: Option<MeetingPointData>,
    #[serde(default)]
    pub end_point: Option<MeetingPointData>,
    #[serde(default)]
    pub top_plan: Option<PlanData>,
    #[serde(default)]
    pub plan_config: Option<PlanConfigData>,
}

impl CookAndRunData {
    pub fn new(id: Uuid, name: String) -> Self {
        let now = Utc::now().naive_utc();
        CookAndRunData {
            id,
            name,
            created: now,
            edited: now,
            occur: now + Duration::days(7),
            is_in_cloud: false,
            admin_notification_email: None,
            team_list: vec![],
            course_list: vec![],
            start_point: None,
            end_point: None,
            top_plan: None,
            plan_config: None,
        }
    }

    pub fn to_meta(&self) -> CookAndRunMetaData {
        CookAndRunMetaData {
            id: self.id,
            name: self.name.clone(),
            created: self.created,
            edited: self.edited,
            occur: self.occur,
            is_in_cloud: self.is_in_cloud,
            admin_notification_email: self.admin_notification_email.clone(),
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self)
            .map_err(|e| format!("Failed to serialize CookAndRunData: {}", e))
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json)
            .map_err(|e| format!("Failed to deserialize CookAndRunData: {}", e))
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CookAndRunMetaData {
    pub id: Uuid,
    pub name: String,
    pub created: NaiveDateTime,
    pub edited: NaiveDateTime,
    pub occur: NaiveDateTime,
    #[serde(default)]
    pub is_in_cloud: bool,
    /// Cloud only. Always `None` for local projects.
    #[serde(default)]
    pub admin_notification_email: Option<String>,
}

impl CookAndRunMetaData {
    pub fn new(id: Uuid, name: String, occur: NaiveDateTime) -> Self {
        CookAndRunMetaData {
            id,
            name,
            created: Utc::now().naive_utc(),
            edited: Utc::now().naive_utc(),
            occur,
            is_in_cloud: false,
            admin_notification_email: None,
        }
    }
}

/// The backend overwrites name, occur and admin notification email together.
#[derive(Debug, Clone)]
pub struct CookAndRunMetaUpdate {
    pub name: String,
    pub occur: NaiveDateTime,
    /// Cloud only (ignored for local projects). If `None` the current address of the project is kept.
    pub admin_notification_email: Option<String>,
}

impl CookAndRunMetaUpdate {
    pub fn validate(&self) -> Result<(), String> {
        check_text("Project name", &self.name, 1, MAX_NAME_LEN)?;
        if let Some(email) = &self.admin_notification_email {
            check_email("Admin notification email", email)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CookAndRunCreate {
    pub name: String,
    /// Required by the backend when the project is created in the cloud, ignored locally.
    pub admin_notification_email: Option<String>,
}

impl CookAndRunCreate {
    pub fn new(name: String) -> Self {
        CookAndRunCreate {
            name,
            admin_notification_email: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        check_text("Project name", &self.name, 1, MAX_NAME_LEN)?;
        if let Some(email) = &self.admin_notification_email {
            check_email("Admin notification email", email)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pagination, sorting (cloud lists)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct PaginationData {
    pub page: u32,
    pub limit: u32,
    pub total: u32,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub pagination: PaginationData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSort {
    CreatedAsc,
    CreatedDesc,
    NameAsc,
    NameDesc,
    EditedAsc,
    EditedDesc,
}

impl ProjectSort {
    pub fn as_query(&self) -> &'static str {
        match self {
            ProjectSort::CreatedAsc => "created_asc",
            ProjectSort::CreatedDesc => "created_desc",
            ProjectSort::NameAsc => "name_asc",
            ProjectSort::NameDesc => "name_desc",
            ProjectSort::EditedAsc => "edited_asc",
            ProjectSort::EditedDesc => "edited_desc",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamSort {
    NameAsc,
    NameDesc,
    CreatedAsc,
    CreatedDesc,
}

impl TeamSort {
    pub fn as_query(&self) -> &'static str {
        match self {
            TeamSort::NameAsc => "name_asc",
            TeamSort::NameDesc => "name_desc",
            TeamSort::CreatedAsc => "created_asc",
            TeamSort::CreatedDesc => "created_desc",
        }
    }
}

// ---------------------------------------------------------------------------
// Audit log (cloud only)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActorType {
    Admin,
    Participant,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Created,
    Updated,
    Canceled,
    PlanInvalidated,
    ResendVerificationMail,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditChangedField {
    pub field: String,
    pub new_value: String,
    pub old_value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub actor_type: AuditActorType,
    /// JWT `sub` of the admin, `None` for participants.
    pub actor_label: Option<String>,
    pub action: AuditAction,
    /// Only filled for `AuditAction::Updated`.
    pub changed_fields: Vec<AuditChangedField>,
    pub created_at: NaiveDateTime,
}

// ---------------------------------------------------------------------------
// Share configuration (cloud only)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RequiredField {
    Mail,
    Phone,
    Members,
    Diets,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ShareTeamConfig {
    pub invite_text: String,
    pub require_email_verification: bool,
    pub default_needs_check: bool,
    pub required_fields: Vec<RequiredField>,
    pub max_teams: Option<u32>,
    pub registration_deadline: Option<NaiveDateTime>,
    pub edit_deadline: Option<NaiveDateTime>,
    pub review_trigger_fields: Vec<RequiredField>,
    pub notify_admin_on_review: bool,
    pub notify_admin_on_create: bool,
    pub notify_admin_on_cancel: bool,
    pub created: NaiveDateTime,
}

impl ShareTeamConfig {
    pub fn to_create(&self) -> ShareTeamConfigCreate {
        ShareTeamConfigCreate {
            invite_text: self.invite_text.clone(),
            require_email_verification: self.require_email_verification,
            default_needs_check: self.default_needs_check,
            required_fields: self.required_fields.clone(),
            max_teams: self.max_teams,
            registration_deadline: self.registration_deadline,
            edit_deadline: self.edit_deadline,
            review_trigger_fields: self.review_trigger_fields.clone(),
            notify_admin_on_review: self.notify_admin_on_review,
            notify_admin_on_create: self.notify_admin_on_create,
            notify_admin_on_cancel: self.notify_admin_on_cancel,
        }
    }
}

impl Default for ShareTeamConfig {
    fn default() -> Self {
        let defaults = ShareTeamConfigCreate::default();
        Self {
            invite_text: defaults.invite_text,
            require_email_verification: defaults.require_email_verification,
            default_needs_check: defaults.default_needs_check,
            required_fields: defaults.required_fields,
            max_teams: defaults.max_teams,
            registration_deadline: defaults.registration_deadline,
            edit_deadline: defaults.edit_deadline,
            review_trigger_fields: defaults.review_trigger_fields,
            notify_admin_on_review: defaults.notify_admin_on_review,
            notify_admin_on_create: defaults.notify_admin_on_create,
            notify_admin_on_cancel: defaults.notify_admin_on_cancel,
            created: Utc::now().naive_utc(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShareTeamConfigCreate {
    pub invite_text: String,
    pub require_email_verification: bool,
    pub default_needs_check: bool,
    pub required_fields: Vec<RequiredField>,
    pub max_teams: Option<u32>,
    pub registration_deadline: Option<NaiveDateTime>,
    pub edit_deadline: Option<NaiveDateTime>,
    pub review_trigger_fields: Vec<RequiredField>,
    pub notify_admin_on_review: bool,
    pub notify_admin_on_create: bool,
    pub notify_admin_on_cancel: bool,
}

impl ShareTeamConfigCreate {
    pub fn validate(&self) -> Result<(), String> {
        check_text("Invite text", &self.invite_text, 1, MAX_INVITE_TEXT_LEN)?;
        if let Some(max_teams) = self.max_teams {
            if max_teams < 1 || max_teams > MAX_SHARE_MAX_TEAMS {
                return Err(format!(
                    "Max teams must be between 1 and {} (got {})",
                    MAX_SHARE_MAX_TEAMS, max_teams
                ));
            }
        }
        if self.required_fields.len() > 4 {
            return Err("Required fields must not contain more than 4 entries".to_string());
        }
        if self.review_trigger_fields.len() > 4 {
            return Err("Review trigger fields must not contain more than 4 entries".to_string());
        }
        Ok(())
    }
}

impl Default for ShareTeamConfigCreate {
    fn default() -> Self {
        Self {
            invite_text: "".to_string(),
            require_email_verification: false,
            default_needs_check: true,
            required_fields: vec![RequiredField::Mail, RequiredField::Diets],
            max_teams: None,
            registration_deadline: Utc::now()
                .naive_utc()
                .date()
                .and_hms_opt(18, 0, 0)
                .and_then(|dt| dt.checked_add_signed(chrono::Duration::days(28))),
            edit_deadline: None,
            review_trigger_fields: vec![],
            notify_admin_on_review: false,
            notify_admin_on_create: false,
            notify_admin_on_cancel: false,
        }
    }
}

/// Reason a team gives when canceling (at most 1,000 characters).
pub fn validate_cancel_reason(reason: &Option<String>) -> Result<(), String> {
    check_optional_text("Cancel reason", reason, MAX_CANCEL_REASON_LEN)
}

// ---------------------------------------------------------------------------
// Personal plan of a team (cloud only, computed by the backend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TeamPlanHostData {
    pub name: String,
    pub address: AddressData,
    pub phone: Option<String>,
    pub members: Option<u16>,
    pub diets: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TeamPlanGuestData {
    pub name: String,
    pub phone: Option<String>,
    pub members: Option<u16>,
    pub diets: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TeamPlanVisitHostData {
    pub name: String,
    pub address: AddressData,
    pub phone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TeamPlanData {
    /// The team itself (with address) as host.
    pub host: TeamPlanHostData,
    /// Guests of the hosting where the team is host (of the last one if it hosts several courses).
    pub host_guest: Vec<TeamPlanGuestData>,
    /// `(course, host)` for every course the team visits (or hosts itself).
    pub visitations: Vec<(CourseData, TeamPlanVisitHostData)>,
}

// ---------------------------------------------------------------------------
// Health (cloud only)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum HealthStatus {
    #[serde(rename = "UP")]
    Up,
    #[serde(rename = "DOWN")]
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HealthCheckData {
    pub name: String,
    pub status: HealthStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HealthData {
    pub status: HealthStatus,
    pub checks: Vec<HealthCheckData>,
}
