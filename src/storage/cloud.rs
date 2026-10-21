use std::{collections::HashSet, fmt};

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use reqwest::{header::CONTENT_TYPE, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;
use web_sys::console;

use crate::{
    keycloak::{AuthState, SessionData},
    storage::{
        validate_cancel_reason, AddressData, AuditAction, AuditActorType, AuditChangedField,
        AuditLogEntry, CookAndRunCreate, CookAndRunData, CookAndRunMetaData, CookAndRunMetaUpdate,
        CourseCreate, CourseData, CourseUpdate, HealthData, HealthStatus, HostingData,
        MeetingPointData, NoteCreate, NoteData, Page, PaginationData, PlanConfigData, PlanData,
        ProjectSort, RequiredField, ShareTeamConfig, ShareTeamConfigCreate, Storage, TeamAccess,
        TeamCloudData, TeamCreate, TeamData, TeamMetaData, TeamPlanData, TeamRegistration,
        TeamSort, TeamStatus, TeamUpdate,
    },
};

/// Page size used when a whole list is loaded (the backend caps `limit` at 100).
const PAGE_LIMIT: u32 = 100;
/// Hard stop for page loops (the backend never holds more than 255 entries of a list).
const MAX_PAGES: u32 = 255;
/// How often a request is repeated after `429 Too Many Requests`.
const MAX_RETRIES: u32 = 4;

// ---------------------------------------------------------------------------
// OpenTracing / trace context propagation
//
// Every public storage call is one trace (all requests of the call share the trace id), every HTTP
// request (and every retry) is its own span. Two header formats are sent, switch off what the
// backend does not read:
//   * `uber-trace-id`  Jaeger / OpenTracing format  {trace-id}:{span-id}:{parent-span-id}:{flags}
//   * `traceparent`    W3C trace context (what OpenTelemetry reads by default)
//
// IMPORTANT: the backend's CORS configuration must list every header sent here in
// `Access-Control-Allow-Headers`, otherwise the browser blocks all requests in the preflight.
// ---------------------------------------------------------------------------

const TRACE_HEADER_JAEGER: bool = true;
const TRACE_HEADER_W3C: bool = true;

#[derive(Debug, Clone, Copy)]
struct TraceContext {
    trace_id: u128,
}

fn random_u64() -> u64 {
    let high = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    let low = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    (high << 32) | low
}

fn random_non_zero_u64() -> u64 {
    loop {
        let value = random_u64();
        if value != 0 {
            return value;
        }
    }
}

impl TraceContext {
    fn new() -> Self {
        // A non-zero upper half guarantees a valid (non-zero) 128 bit trace id.
        let trace_id = ((random_non_zero_u64() as u128) << 64) | random_u64() as u128;
        TraceContext { trace_id }
    }

    /// Adds the trace headers for a new span to the request. Sampled flag is always set.
    fn apply(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let span_id = random_non_zero_u64();
        let mut builder = builder;
        if TRACE_HEADER_JAEGER {
            // parent span id 0 = the client does not report spans itself, so the request span is a root span
            builder = builder.header(
                "uber-trace-id",
                format!("{:032x}:{:016x}:0:1", self.trace_id, span_id),
            );
        }
        if TRACE_HEADER_W3C {
            builder = builder.header(
                "traceparent",
                format!("00-{:032x}-{:016x}-01", self.trace_id, span_id),
            );
        }
        builder
    }
}

// ---------------------------------------------------------------------------
// HTTP plumbing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Auth {
    /// No credentials at all.
    Anonymous,
    /// `Authorization: Bearer <access token of the logged in user>`.
    Bearer,
    /// `x-access-token: <team token>`, the bearer token is deliberately not sent.
    Token(String),
}

fn auth_for(access: &TeamAccess) -> Auth {
    match access {
        TeamAccess::Admin => Auth::Bearer,
        TeamAccess::Token(token) => Auth::Token(token.to_string()),
    }
}

#[derive(Debug, Clone)]
struct Req {
    method: Method,
    path: String,
    query: Vec<(&'static str, String)>,
    auth: Auth,
    body: Option<String>,
}

impl Req {
    fn new(method: Method, path: String) -> Self {
        Req {
            method,
            path,
            query: Vec::new(),
            auth: Auth::Bearer,
            body: None,
        }
    }

    fn get(path: String) -> Self {
        Self::new(Method::GET, path)
    }

    fn post(path: String) -> Self {
        Self::new(Method::POST, path)
    }

    fn patch(path: String) -> Self {
        Self::new(Method::PATCH, path)
    }

    fn delete(path: String) -> Self {
        Self::new(Method::DELETE, path)
    }

    fn auth(mut self, auth: Auth) -> Self {
        self.auth = auth;
        self
    }

    fn query(mut self, key: &'static str, value: impl ToString) -> Self {
        self.query.push((key, value.to_string()));
        self
    }

    fn json<B: Serialize>(mut self, body: &B) -> Result<Self, String> {
        self.body = Some(
            serde_json::to_string(body)
                .map_err(|e| format!("Request body could not be serialized: {}", e))?,
        );
        Ok(self)
    }
}

#[derive(Debug, Clone)]
struct ApiError {
    /// `None` if the request never got a response (network error, missing login, bad body).
    status: Option<StatusCode>,
    message: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

impl ApiError {
    fn local(message: impl Into<String>) -> Self {
        ApiError {
            status: None,
            message: message.into(),
        }
    }

    /// Reads the error message of the backend (`{"error": "..."}` or plain text).
    async fn from_response(response: reqwest::Response) -> Self {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = match serde_json::from_str::<ErrorBody>(&body) {
            Ok(parsed) => parsed.error,
            Err(_) => body.trim().to_string(),
        };
        ApiError {
            status: Some(status),
            message,
        }
    }

    /// The backend answers `404` for several reasons and only the message tells them apart.
    fn is_not_found_with(&self, needle: &str) -> bool {
        self.status == Some(StatusCode::NOT_FOUND) && self.message.contains(needle)
    }

    fn is_not_found(&self) -> bool {
        self.status == Some(StatusCode::NOT_FOUND)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.status, self.message.is_empty()) {
            (Some(status), true) => write!(f, "Request failed: {}", status),
            (Some(status), false) => write!(f, "Request failed: {} - {}", status, self.message),
            (None, _) => write!(f, "{}", self.message),
        }
    }
}

impl From<ApiError> for String {
    fn from(error: ApiError) -> String {
        error.to_string()
    }
}

async fn sleep_ms(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let scheduled = web_sys::window()
            .map(|window| {
                window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
                    .is_ok()
            })
            .unwrap_or(false);
        if !scheduled {
            let _ = resolve.call0(&js_sys::Object::new());
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

// ---------------------------------------------------------------------------
// Wire types (exactly the schemas of swagger.yml)
// ---------------------------------------------------------------------------

fn to_naive(value: DateTime<Utc>) -> NaiveDateTime {
    value.naive_utc()
}

fn to_utc(value: NaiveDateTime) -> DateTime<Utc> {
    Utc.from_utc_datetime(&value)
}

#[derive(Debug, Deserialize)]
struct ListResponse<T> {
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct PageResponse<T> {
    data: Vec<T>,
    pagination: PaginationData,
}

impl<T> PageResponse<T> {
    fn map<U>(self, convert: impl FnMut(T) -> Result<U, String>) -> Result<Page<U>, String> {
        let items = self
            .data
            .into_iter()
            .map(convert)
            .collect::<Result<Vec<U>, String>>()?;
        Ok(Page {
            items,
            pagination: self.pagination,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ProjectMetaResponse {
    id: Uuid,
    name: String,
    created: DateTime<Utc>,
    edited: DateTime<Utc>,
    occur: DateTime<Utc>,
    #[serde(default)]
    admin_notification_email: Option<String>,
}

impl ProjectMetaResponse {
    fn into_meta(self) -> CookAndRunMetaData {
        CookAndRunMetaData {
            id: self.id,
            name: self.name,
            created: to_naive(self.created),
            edited: to_naive(self.edited),
            occur: to_naive(self.occur),
            is_in_cloud: true,
            admin_notification_email: self.admin_notification_email,
        }
    }
}

#[derive(Debug, Serialize)]
struct ProjectCreateRequest<'a> {
    name: &'a str,
    #[serde(rename = "userId")]
    user_id: &'a str,
    admin_notification_email: &'a str,
}

#[derive(Debug, Serialize)]
struct ProjectMetaUpdateRequest<'a> {
    name: &'a str,
    occur: DateTime<Utc>,
    admin_notification_email: &'a str,
}

fn members_from_wire(team_id: Uuid, members: Option<i32>) -> Result<Option<u16>, String> {
    members
        .map(|value| {
            u16::try_from(value)
                .map_err(|_| format!("Team {} has an invalid member count: {}", team_id, value))
        })
        .transpose()
}

#[derive(Debug, Deserialize)]
struct TeamResponse {
    id: Uuid,
    name: String,
    address: AddressData,
    mail: Option<String>,
    phone: Option<String>,
    members: Option<i32>,
    diets: Option<String>,
    created_by_user: Option<String>,
    created: DateTime<Utc>,
    edited: DateTime<Utc>,
    status: TeamStatus,
    canceled_at: Option<DateTime<Utc>>,
    cancel_reason: Option<String>,
    email_verified_at: Option<DateTime<Utc>>,
    last_route_hash: Option<String>,
    access_token: String,
}

impl TeamResponse {
    fn into_team(self, note_list: Vec<NoteData>) -> Result<TeamData, String> {
        let access_token = Uuid::parse_str(&self.access_token)
            .map_err(|e| format!("Team {} has an invalid access token: {}", self.id, e))?;
        Ok(TeamData {
            id: self.id,
            name: self.name,
            created: to_naive(self.created),
            edited: to_naive(self.edited),
            address: self.address,
            mail: self.mail,
            phone: self.phone,
            members: members_from_wire(self.id, self.members)?,
            diets: self.diets,
            status: self.status,
            cloud: Some(TeamCloudData {
                access_token,
                created_by_user: self.created_by_user,
                canceled_at: self.canceled_at.map(to_naive),
                cancel_reason: self.cancel_reason,
                email_verified_at: self.email_verified_at.map(to_naive),
                last_route_hash: self.last_route_hash,
            }),
            note_list,
        })
    }
}

#[derive(Debug, Deserialize)]
struct TeamMetaResponse {
    id: Uuid,
    name: String,
    mail: bool,
    phone: bool,
    members: Option<i32>,
    diets: bool,
    created_by_user: Option<String>,
    created: DateTime<Utc>,
    edited: DateTime<Utc>,
    status: TeamStatus,
    canceled_at: Option<DateTime<Utc>>,
    email_verified: bool,
}

impl TeamMetaResponse {
    fn into_meta(self) -> Result<TeamMetaData, String> {
        Ok(TeamMetaData {
            id: self.id,
            name: self.name,
            mail: self.mail,
            phone: self.phone,
            members: members_from_wire(self.id, self.members)?,
            diets: self.diets,
            created_by_user: self.created_by_user,
            created: to_naive(self.created),
            edited: to_naive(self.edited),
            status: self.status,
            canceled_at: self.canceled_at.map(to_naive),
            email_verified: self.email_verified,
        })
    }
}

#[derive(Debug, Serialize)]
struct TeamCreateRequest<'a> {
    name: &'a str,
    #[serde(rename = "userId", skip_serializing_if = "Option::is_none")]
    user_id: Option<&'a str>,
    access_token: Uuid,
    address: &'a AddressData,
    mail: &'a Option<String>,
    phone: &'a Option<String>,
    members: &'a Option<u16>,
    diets: &'a Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum TeamUpdateStatus {
    Active,
    Review,
}

#[derive(Debug, Serialize)]
struct TeamUpdateRequest<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<TeamUpdateStatus>,
    address: &'a AddressData,
    mail: &'a Option<String>,
    phone: &'a Option<String>,
    members: &'a Option<u16>,
    diets: &'a Option<String>,
}

#[derive(Debug, Serialize)]
struct TeamCancelRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: &'a Option<String>,
}

#[derive(Debug, Deserialize)]
struct NoteResponse {
    id: Uuid,
    headline: String,
    content: String,
    created: DateTime<Utc>,
}

impl NoteResponse {
    fn into_note(self) -> NoteData {
        NoteData {
            id: self.id,
            headline: self.headline,
            content: self.content,
            created: to_naive(self.created),
        }
    }
}

#[derive(Debug, Deserialize)]
struct PlanResponse {
    hosting_list: Vec<HostingData>,
    #[serde(default)]
    stale_since: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct PlanUpdateRequest<'a> {
    hosting_list: &'a [HostingData],
}

#[derive(Debug, Deserialize)]
struct ShareConfigResponse {
    invite_text: String,
    require_email_verification: bool,
    default_needs_check: bool,
    required_fields: Vec<RequiredField>,
    max_teams: Option<u32>,
    registration_deadline: Option<DateTime<Utc>>,
    edit_deadline: Option<DateTime<Utc>>,
    review_trigger_fields: Vec<RequiredField>,
    notify_admin_on_review: bool,
    notify_admin_on_create: bool,
    notify_admin_on_cancel: bool,
    created: DateTime<Utc>,
}

impl ShareConfigResponse {
    fn into_config(self) -> ShareTeamConfig {
        ShareTeamConfig {
            invite_text: self.invite_text,
            require_email_verification: self.require_email_verification,
            default_needs_check: self.default_needs_check,
            required_fields: self.required_fields,
            max_teams: self.max_teams,
            registration_deadline: self.registration_deadline.map(to_naive),
            edit_deadline: self.edit_deadline.map(to_naive),
            review_trigger_fields: self.review_trigger_fields,
            notify_admin_on_review: self.notify_admin_on_review,
            notify_admin_on_create: self.notify_admin_on_create,
            notify_admin_on_cancel: self.notify_admin_on_cancel,
            created: to_naive(self.created),
        }
    }
}

#[derive(Debug, Serialize)]
struct ShareConfigRequest {
    invite_text: String,
    require_email_verification: bool,
    default_needs_check: bool,
    required_fields: Vec<RequiredField>,
    max_teams: Option<u32>,
    registration_deadline: Option<DateTime<Utc>>,
    edit_deadline: Option<DateTime<Utc>>,
    review_trigger_fields: Vec<RequiredField>,
    notify_admin_on_review: bool,
    notify_admin_on_create: bool,
    notify_admin_on_cancel: bool,
}

impl From<&ShareTeamConfigCreate> for ShareConfigRequest {
    fn from(config: &ShareTeamConfigCreate) -> Self {
        ShareConfigRequest {
            invite_text: config.invite_text.clone(),
            require_email_verification: config.require_email_verification,
            default_needs_check: config.default_needs_check,
            required_fields: config.required_fields.clone(),
            max_teams: config.max_teams,
            registration_deadline: config.registration_deadline.map(to_utc),
            edit_deadline: config.edit_deadline.map(to_utc),
            review_trigger_fields: config.review_trigger_fields.clone(),
            notify_admin_on_review: config.notify_admin_on_review,
            notify_admin_on_create: config.notify_admin_on_create,
            notify_admin_on_cancel: config.notify_admin_on_cancel,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct AuditChangesResponse {
    /// `[field, new value, old value]`
    #[serde(default)]
    changed_fields: Vec<(String, String, String)>,
}

#[derive(Debug, Deserialize)]
struct AuditLogEntryResponse {
    id: Uuid,
    actor_type: AuditActorType,
    actor_label: Option<String>,
    action: AuditAction,
    #[serde(default)]
    changes: AuditChangesResponse,
    created_at: DateTime<Utc>,
}

impl AuditLogEntryResponse {
    fn into_entry(self) -> AuditLogEntry {
        AuditLogEntry {
            id: self.id,
            actor_type: self.actor_type,
            actor_label: self.actor_label,
            action: self.action,
            changed_fields: self
                .changes
                .changed_fields
                .into_iter()
                .map(|(field, new_value, old_value)| AuditChangedField {
                    field,
                    new_value,
                    old_value,
                })
                .collect(),
            created_at: to_naive(self.created_at),
        }
    }
}

#[derive(Debug, Serialize)]
struct RouteMailRequest<'a> {
    sent_to_team_ids: &'a [Uuid],
}

fn project_path(project_id: Uuid) -> String {
    format!("/project/{}", project_id)
}

fn team_path(project_id: Uuid, team_id: Uuid) -> String {
    format!("/project/{}/team/{}", project_id, team_id)
}

// ---------------------------------------------------------------------------
// CloudStorage
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct CloudStorage {
    base_url: String,
    /// `None` for anonymous (participant) connections.
    auth_state: Option<AuthState>,
    client: reqwest::Client,
}

impl CloudStorage {
    /// Connection for a logged in user (project owner).
    pub async fn new(auth_state: AuthState, base_url: String) -> Result<Self, String> {
        let cloud_storage = CloudStorage {
            base_url,
            auth_state: Some(auth_state),
            client: reqwest::Client::new(),
        };
        cloud_storage.require_session()?;
        cloud_storage.check_health().await?;
        Ok(cloud_storage)
    }

    /// Connection without login. Only the participant operations work.
    pub async fn new_anonymous(base_url: String) -> Result<Self, String> {
        let cloud_storage = CloudStorage {
            base_url,
            auth_state: None,
            client: reqwest::Client::new(),
        };
        cloud_storage.check_health().await?;
        Ok(cloud_storage)
    }

    fn session(&self) -> Option<SessionData> {
        match self.auth_state.as_ref() {
            Some(AuthState::LoggedIn(_, _, session_data)) => Some(session_data.clone()),
            _ => None,
        }
    }

    fn require_session(&self) -> Result<SessionData, String> {
        self.session().ok_or_else(|| "No auth data!".to_string())
    }

    async fn check_health(&self) -> Result<(), String> {
        let health = self
            .health()
            .await
            .map_err(|e| format!("Health check failed: {}", e))?;
        if health.status == HealthStatus::Up {
            return Ok(());
        }
        let failed: Vec<String> = health
            .checks
            .iter()
            .filter(|check| check.status != HealthStatus::Up)
            .map(|check| check.name.clone())
            .collect();
        Err(format!("Health check failed: {}", failed.join(", ")))
    }

    // ----- request execution -----------------------------------------------------------------

    /// Sends the request, adds the trace headers and repeats it after `429` (honoring `Retry-After`).
    async fn execute(
        &self,
        trace: &TraceContext,
        req: &Req,
    ) -> Result<reqwest::Response, ApiError> {
        let base = format!("{}{}", self.base_url.trim_end_matches('/'), req.path);
        // Query-Parameter werden direkt in die URL geschrieben, da `RequestBuilder::query`
        // im wasm-Build von reqwest nicht verfügbar ist.
        let url = if req.query.is_empty() {
            base
        } else {
            reqwest::Url::parse_with_params(&base, &req.query)
                .map_err(|e| ApiError::local(format!("Invalid request URL: {}", e)))?
                .to_string()
        };
        let bearer = match &req.auth {
            Auth::Bearer => Some(
                self.session()
                    .ok_or_else(|| ApiError::local("No auth data!"))?
                    .access_token,
            ),
            _ => None,
        };

        let mut attempt: u32 = 0;
        loop {
            let mut builder = self.client.request(req.method.clone(), &url);
            builder = trace.apply(builder);
            match &req.auth {
                Auth::Anonymous => {}
                Auth::Bearer => {
                    if let Some(token) = &bearer {
                        builder = builder.bearer_auth(token);
                    }
                }
                Auth::Token(token) => {
                    builder = builder.header("x-access-token", token.as_str());
                }
            }
            if let Some(body) = &req.body {
                builder = builder
                    .header(CONTENT_TYPE, "application/json")
                    .body(body.clone());
            }

            let response = builder
                .send()
                .await
                .map_err(|e| ApiError::local(format!("Request error: {}", e)))?;

            if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES {
                // `Retry-After` is only readable if the backend exposes it via CORS, otherwise back off.
                let wait_ms = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u32>().ok())
                    .map(|seconds| seconds.clamp(1, 10) * 1000)
                    .unwrap_or_else(|| (500u32 << attempt).min(8000));
                console::warn_1(
                    &format!(
                        "Rate limited on {} {}, retrying in {} ms",
                        req.method, req.path, wait_ms
                    )
                    .into(),
                );
                sleep_ms(wait_ms).await;
                attempt += 1;
                continue;
            }
            return Ok(response);
        }
    }

    async fn call(&self, trace: &TraceContext, req: Req) -> Result<reqwest::Response, ApiError> {
        let response = self.execute(trace, &req).await?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(ApiError::from_response(response).await)
        }
    }

    async fn call_unit(&self, trace: &TraceContext, req: Req) -> Result<(), ApiError> {
        self.call(trace, req).await.map(|_| ())
    }

    async fn call_json<T: DeserializeOwned>(
        &self,
        trace: &TraceContext,
        req: Req,
    ) -> Result<T, ApiError> {
        let response = self.call(trace, req).await?;
        response
            .json::<T>()
            .await
            .map_err(|e| ApiError::local(format!("Response could not be parsed: {}", e)))
    }

    // ----- shared building blocks ------------------------------------------------------------

    async fn fetch_meta(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
    ) -> Result<CookAndRunMetaData, String> {
        let response: ProjectMetaResponse = self
            .call_json(
                trace,
                Req::get(format!("{}/metadata", project_path(project_id))),
            )
            .await?;
        Ok(response.into_meta())
    }

    async fn fetch_meta_page(
        &self,
        trace: &TraceContext,
        page: u32,
        limit: u32,
        sort: ProjectSort,
    ) -> Result<Page<CookAndRunMetaData>, String> {
        let session = self.require_session()?;
        let response: PageResponse<ProjectMetaResponse> = self
            .call_json(
                trace,
                Req::get("/project".to_string())
                    .query("userId", &session.user.sub)
                    .query("page", page)
                    .query("limit", limit)
                    .query("sort", sort.as_query()),
            )
            .await?;
        response.map(|meta| Ok(meta.into_meta()))
    }

    async fn fetch_courses(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
    ) -> Result<Vec<CourseData>, String> {
        let response: ListResponse<CourseData> = self
            .call_json(
                trace,
                Req::get(format!("{}/courses", project_path(project_id))),
            )
            .await?;
        Ok(response.data)
    }

    async fn fetch_team(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
    ) -> Result<TeamData, String> {
        let response: TeamResponse = self
            .call_json(
                trace,
                Req::get(team_path(project_id, team_id)).auth(auth_for(access)),
            )
            .await?;
        response.into_team(Vec::new())
    }

    async fn fetch_notes(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
        team_id: Uuid,
    ) -> Result<Vec<NoteData>, String> {
        let response: ListResponse<NoteResponse> = self
            .call_json(
                trace,
                Req::get(format!("{}/notes", team_path(project_id, team_id))),
            )
            .await?;
        Ok(response
            .data
            .into_iter()
            .map(NoteResponse::into_note)
            .collect())
    }

    async fn fetch_team_meta_page(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
        page: u32,
        limit: u32,
        sort: TeamSort,
    ) -> Result<Page<TeamMetaData>, String> {
        let response: PageResponse<TeamMetaResponse> = self
            .call_json(
                trace,
                Req::get(format!("{}/teams", project_path(project_id)))
                    .query("page", page)
                    .query("limit", limit)
                    .query("sort", sort.as_query()),
            )
            .await?;
        response.map(TeamMetaResponse::into_meta)
    }

    async fn fetch_all_team_metas(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
    ) -> Result<Vec<TeamMetaData>, String> {
        let mut all = Vec::new();
        let mut page = 1;
        loop {
            let result = self
                .fetch_team_meta_page(trace, project_id, page, PAGE_LIMIT, TeamSort::CreatedAsc)
                .await?;
            let has_next = result.pagination.has_next;
            all.extend(result.items);
            if !has_next || page >= MAX_PAGES {
                return Ok(all);
            }
            page += 1;
        }
    }

    /// The list endpoint only returns the short form, so the full data costs one request per team
    /// (two with notes). Requests that hit the rate limit are retried automatically.
    async fn fetch_teams(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
        with_notes: bool,
    ) -> Result<Vec<TeamData>, String> {
        let metas = self.fetch_all_team_metas(trace, project_id).await?;
        let mut teams = Vec::with_capacity(metas.len());
        for meta in metas {
            let mut team = self
                .fetch_team(trace, project_id, meta.id, &TeamAccess::Admin)
                .await?;
            if with_notes {
                team.note_list = self.fetch_notes(trace, project_id, meta.id).await?;
            }
            teams.push(team);
        }
        Ok(teams)
    }

    /// `404` with the given message part means "not set"; any other `404` (project unknown) is an error.
    async fn fetch_point(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
        segment: &str,
        missing: &str,
    ) -> Result<Option<MeetingPointData>, String> {
        let req = Req::get(format!("{}/{}", project_path(project_id), segment));
        match self.call_json::<MeetingPointData>(trace, req).await {
            Ok(point) => Ok(Some(point)),
            Err(e) if e.is_not_found_with(missing) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn put_point(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
        segment: &str,
        missing: &str,
        point: &Option<MeetingPointData>,
    ) -> Result<(), String> {
        let path = format!("{}/{}", project_path(project_id), segment);
        match point {
            Some(point) => {
                point.validate()?;
                self.call_unit(trace, Req::patch(path).json(point)?).await?;
                Ok(())
            }
            None => match self.call_unit(trace, Req::delete(path)).await {
                Ok(()) => Ok(()),
                // Removing a point that is not set is not an error for the caller.
                Err(e) if e.is_not_found_with(missing) => Ok(()),
                Err(e) => Err(e.into()),
            },
        }
    }

    async fn fetch_plan(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
    ) -> Result<Option<PlanData>, String> {
        let req = Req::get(format!("{}/plan", project_path(project_id)));
        match self.call_json::<PlanResponse>(trace, req).await {
            Ok(plan) => Ok(Some(PlanData {
                hosting_list: plan.hosting_list,
                stale_since: plan.stale_since.map(to_naive),
            })),
            // "no plan" and "unknown project" cannot be told apart by the backend
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn fetch_plan_config(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
    ) -> Result<Option<PlanConfigData>, String> {
        let req = Req::get(format!("{}/plan_config", project_path(project_id)));
        match self.call_json::<PlanConfigData>(trace, req).await {
            Ok(config) => Ok(Some(config)),
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn patch_team(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
        team: &TeamUpdate,
    ) -> Result<(), String> {
        team.validate()?;
        // Admins decide the status, for participants the backend ignores (and we do not send) it.
        let status = match access {
            TeamAccess::Admin => Some(if team.needs_check {
                TeamUpdateStatus::Review
            } else {
                TeamUpdateStatus::Active
            }),
            TeamAccess::Token(_) => None,
        };
        let body = TeamUpdateRequest {
            name: &team.name,
            status,
            address: &team.address,
            mail: &team.mail,
            phone: &team.phone,
            members: &team.members,
            diets: &team.diets,
        };
        self.call_unit(
            trace,
            Req::patch(team_path(project_id, team_id))
                .auth(auth_for(access))
                .json(&body)?,
        )
        .await?;
        Ok(())
    }

    // =========================================================================================
    // Cloud only API (no local counterpart)
    // =========================================================================================

    // ----- health ----------------------------------------------------------------------------

    /// `503` carries a body as well, so a failing check is returned as data, not as error.
    async fn fetch_health(&self, path: &str) -> Result<HealthData, String> {
        let trace = TraceContext::new();
        let req = Req::get(path.to_string()).auth(Auth::Anonymous);
        let response = self.execute(&trace, &req).await?;
        let status = response.status();
        if status.is_success() || status == StatusCode::SERVICE_UNAVAILABLE {
            response
                .json::<HealthData>()
                .await
                .map_err(|e| format!("Response could not be parsed: {}", e))
        } else {
            Err(ApiError::from_response(response).await.into())
        }
    }

    pub async fn health(&self) -> Result<HealthData, String> {
        self.fetch_health("/health").await
    }

    pub async fn health_live(&self) -> Result<HealthData, String> {
        self.fetch_health("/health/live").await
    }

    pub async fn health_ready(&self) -> Result<HealthData, String> {
        self.fetch_health("/health/ready").await
    }

    // ----- paged lists -----------------------------------------------------------------------

    pub async fn select_cook_and_run_meta_page(
        &self,
        page: u32,
        limit: u32,
        sort: ProjectSort,
    ) -> Result<Page<CookAndRunMetaData>, String> {
        let trace = TraceContext::new();
        self.fetch_meta_page(&trace, page, limit, sort).await
    }

    pub async fn select_team_meta_page(
        &self,
        project_id: Uuid,
        page: u32,
        limit: u32,
        sort: TeamSort,
    ) -> Result<Page<TeamMetaData>, String> {
        let trace = TraceContext::new();
        self.fetch_team_meta_page(&trace, project_id, page, limit, sort)
            .await
    }

    /// Change log of a team, newest first (the backend clamps `limit` to 1-200).
    pub async fn select_team_audit_log(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        page: u32,
        limit: u32,
    ) -> Result<Page<AuditLogEntry>, String> {
        let trace = TraceContext::new();
        let response: PageResponse<AuditLogEntryResponse> = self
            .call_json(
                &trace,
                Req::get(format!("{}/audit-log", team_path(project_id, team_id)))
                    .query("page", page)
                    .query("limit", limit),
            )
            .await?;
        response.map(|entry| Ok(entry.into_entry()))
    }

    // ----- share configuration ---------------------------------------------------------------

    pub async fn select_cook_and_run_share_config(
        &self,
        project_id: Uuid,
    ) -> Result<Option<ShareTeamConfig>, String> {
        let trace = TraceContext::new();
        let req = Req::get(format!("{}/share_team_config", project_path(project_id)));
        match self.call_json::<ShareConfigResponse>(&trace, req).await {
            Ok(config) => Ok(Some(config.into_config())),
            Err(e) if e.is_not_found_with("Sharing configuration") => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Upsert: an existing configuration is replaced without error.
    pub async fn create_cook_and_run_share_config(
        &self,
        project_id: Uuid,
        share_config: &ShareTeamConfigCreate,
    ) -> Result<(), String> {
        share_config.validate()?;
        let trace = TraceContext::new();
        let body = ShareConfigRequest::from(share_config);
        self.call_unit(
            &trace,
            Req::post(format!("{}/share_team_config", project_path(project_id))).json(&body)?,
        )
        .await?;
        Ok(())
    }

    /// Upsert as well (the backend treats `POST` and `PATCH` identically).
    pub async fn update_cook_and_run_share_config(
        &self,
        project_id: Uuid,
        share_config: &ShareTeamConfigCreate,
    ) -> Result<(), String> {
        share_config.validate()?;
        let trace = TraceContext::new();
        let body = ShareConfigRequest::from(share_config);
        self.call_unit(
            &trace,
            Req::patch(format!("{}/share_team_config", project_path(project_id))).json(&body)?,
        )
        .await?;
        Ok(())
    }

    pub async fn delete_cook_and_run_share_config(&self, project_id: Uuid) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::delete(format!("{}/share_team_config", project_path(project_id))),
        )
        .await?;
        Ok(())
    }

    // ----- plan ------------------------------------------------------------------------------

    /// Declares a stale plan as still valid (no content change).
    pub async fn confirm_plan(&self, project_id: Uuid) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::post(format!("{}/plan/confirm", project_path(project_id))),
        )
        .await?;
        Ok(())
    }

    /// Queues the "route updated" mail for 1 to 128 distinct teams of the project.
    pub async fn send_route_mails(
        &self,
        project_id: Uuid,
        team_ids: &[Uuid],
    ) -> Result<(), String> {
        if team_ids.is_empty() || team_ids.len() > 128 {
            return Err(format!(
                "Route mails can be sent to 1 to 128 teams at once (got {})",
                team_ids.len()
            ));
        }
        let distinct: HashSet<&Uuid> = team_ids.iter().collect();
        if distinct.len() != team_ids.len() {
            return Err("Route mail team ids must be distinct".to_string());
        }

        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::post(format!(
                "{}/plan/send-route-mails",
                project_path(project_id)
            ))
            .json(&RouteMailRequest {
                sent_to_team_ids: team_ids,
            })?,
        )
        .await?;
        Ok(())
    }

    // ----- participant / team self-service ---------------------------------------------------

    /// Public registration: anonymous, the project needs a share configuration.
    pub async fn create_team_as_participant(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        team: &TeamRegistration,
    ) -> Result<(), String> {
        team.validate()?;
        let trace = TraceContext::new();
        let body = TeamCreateRequest {
            name: &team.name,
            user_id: None,
            access_token: team.access_token,
            address: &team.address,
            mail: &team.mail,
            phone: &team.phone,
            members: &team.members,
            diets: &team.diets,
        };
        self.call_unit(
            &trace,
            Req::post(team_path(project_id, team_id))
                .auth(Auth::Anonymous)
                .json(&body)?,
        )
        .await?;
        Ok(())
    }

    /// Team data (without notes) as admin (bearer) or as participant (team token).
    pub async fn select_team_with_access(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
    ) -> Result<TeamData, String> {
        let trace = TraceContext::new();
        self.fetch_team(&trace, project_id, team_id, access).await
    }

    pub async fn update_team_with_access(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
        team: &TeamUpdate,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.patch_team(&trace, project_id, team_id, access, team)
            .await
    }

    /// Confirms the email address (team token only).
    pub async fn verify_team_email(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        access_token: Uuid,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::post(format!("{}/verify", team_path(project_id, team_id)))
                .auth(Auth::Token(access_token.to_string())),
        )
        .await?;
        Ok(())
    }

    /// A participant cancels its team (team token only). The team is not deleted.
    pub async fn cancel_team(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        access_token: Uuid,
        reason: Option<String>,
    ) -> Result<(), String> {
        validate_cancel_reason(&reason)?;
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::post(format!("{}/cancel", team_path(project_id, team_id)))
                .auth(Auth::Token(access_token.to_string()))
                .json(&TeamCancelRequest { reason: &reason })?,
        )
        .await?;
        Ok(())
    }

    pub async fn resend_team_verification(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::post(format!(
                "{}/resend-verification",
                team_path(project_id, team_id)
            ))
            .auth(auth_for(access)),
        )
        .await?;
        Ok(())
    }

    /// Personal plan of a team. `409` while the plan is stale, `404` if there is no plan.
    pub async fn select_team_plan(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
    ) -> Result<TeamPlanData, String> {
        let trace = TraceContext::new();
        let plan: TeamPlanData = self
            .call_json(
                &trace,
                Req::get(format!("{}/plan", team_path(project_id, team_id))).auth(auth_for(access)),
            )
            .await?;
        Ok(plan)
    }
}

// ---------------------------------------------------------------------------
// Storage (shared with the local storage)
// ---------------------------------------------------------------------------

impl Storage for CloudStorage {
    // ----- project ---------------------------------------------------------------------------

    async fn create_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        cook_and_run: &CookAndRunCreate,
    ) -> Result<(), String> {
        cook_and_run.validate()?;
        let email = cook_and_run
            .admin_notification_email
            .as_deref()
            .ok_or_else(|| {
                "An admin notification email is required to create a project in the cloud"
                    .to_string()
            })?;
        let session = self.require_session()?;

        let trace = TraceContext::new();
        let body = ProjectCreateRequest {
            name: &cook_and_run.name,
            user_id: &session.user.sub,
            admin_notification_email: email,
        };
        self.call_unit(
            &trace,
            Req::post(project_path(cook_and_run_id)).json(&body)?,
        )
        .await?;
        Ok(())
    }

    async fn delete_cook_and_run(&mut self, id: Uuid) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(&trace, Req::delete(project_path(id)))
            .await?;
        Ok(())
    }

    async fn select_cook_and_run(&self, id: Uuid) -> Result<CookAndRunData, String> {
        // One trace for the whole composite load: metadata, courses, teams (+notes), points, plan, plan config.
        let trace = TraceContext::new();
        let meta = self.fetch_meta(&trace, id).await?;
        let course_list = self.fetch_courses(&trace, id).await?;
        let team_list = self.fetch_teams(&trace, id, true).await?;
        let start_point = self
            .fetch_point(&trace, id, "start_point", "Starting point")
            .await?;
        let end_point = self
            .fetch_point(&trace, id, "end_point", "Ending point")
            .await?;
        let top_plan = self.fetch_plan(&trace, id).await?;
        let plan_config = self.fetch_plan_config(&trace, id).await?;

        Ok(CookAndRunData {
            id: meta.id,
            name: meta.name,
            created: meta.created,
            edited: meta.edited,
            occur: meta.occur,
            is_in_cloud: true,
            admin_notification_email: meta.admin_notification_email,
            team_list,
            course_list,
            start_point,
            end_point,
            top_plan,
            plan_config,
        })
    }

    async fn select_cook_and_run_meta(&self, id: Uuid) -> Result<CookAndRunMetaData, String> {
        let trace = TraceContext::new();
        self.fetch_meta(&trace, id).await
    }

    async fn select_cook_and_run_meta_list(&self) -> Result<Vec<CookAndRunMetaData>, String> {
        let trace = TraceContext::new();
        let mut all = Vec::new();
        let mut page = 1;
        loop {
            let result = self
                .fetch_meta_page(&trace, page, PAGE_LIMIT, ProjectSort::CreatedDesc)
                .await?;
            let has_next = result.pagination.has_next;
            all.extend(result.items);
            if !has_next || page >= MAX_PAGES {
                return Ok(all);
            }
            page += 1;
        }
    }

    async fn update_meta_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        cook_and_run_meta: &CookAndRunMetaUpdate,
    ) -> Result<(), String> {
        cook_and_run_meta.validate()?;
        let trace = TraceContext::new();

        // The backend overwrites all three fields, so a missing email is taken over from the project.
        let email = match &cook_and_run_meta.admin_notification_email {
            Some(email) => email.clone(),
            None => self
                .fetch_meta(&trace, cook_and_run_id)
                .await?
                .admin_notification_email
                .ok_or_else(|| {
                    "The project has no admin notification email, please provide one".to_string()
                })?,
        };

        let body = ProjectMetaUpdateRequest {
            name: &cook_and_run_meta.name,
            occur: to_utc(cook_and_run_meta.occur),
            admin_notification_email: &email,
        };
        self.call_unit(
            &trace,
            Req::patch(format!("{}/metadata", project_path(cook_and_run_id))).json(&body)?,
        )
        .await?;
        Ok(())
    }

    // ----- start / end point -----------------------------------------------------------------

    async fn select_cook_and_run_start_point(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<MeetingPointData>, String> {
        let trace = TraceContext::new();
        self.fetch_point(&trace, cook_and_run_id, "start_point", "Starting point")
            .await
    }

    async fn update_start_point_in_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        start_point: &Option<MeetingPointData>,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.put_point(
            &trace,
            cook_and_run_id,
            "start_point",
            "Starting point",
            start_point,
        )
        .await
    }

    async fn select_cook_and_run_end_point(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<MeetingPointData>, String> {
        let trace = TraceContext::new();
        self.fetch_point(&trace, cook_and_run_id, "end_point", "Ending point")
            .await
    }

    async fn update_end_point_in_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        end_point: &Option<MeetingPointData>,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.put_point(
            &trace,
            cook_and_run_id,
            "end_point",
            "Ending point",
            end_point,
        )
        .await
    }

    // ----- courses ---------------------------------------------------------------------------

    async fn select_cook_and_run_course_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<CourseData>, String> {
        let trace = TraceContext::new();
        self.fetch_courses(&trace, cook_and_run_id).await
    }

    async fn create_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
        course: &CourseCreate,
    ) -> Result<(), String> {
        course.validate()?;
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::post(format!(
                "{}/course/{}",
                project_path(cook_and_run_id),
                course_id
            ))
            .json(course)?,
        )
        .await?;
        Ok(())
    }

    async fn update_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
        course: &CourseUpdate,
    ) -> Result<(), String> {
        course.validate()?;
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::patch(format!(
                "{}/course/{}",
                project_path(cook_and_run_id),
                course_id
            ))
            .json(course)?,
        )
        .await?;
        Ok(())
    }

    async fn delete_course_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        course_id: Uuid,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::delete(format!(
                "{}/course/{}",
                project_path(cook_and_run_id),
                course_id
            )),
        )
        .await?;
        Ok(())
    }

    // ----- teams -----------------------------------------------------------------------------

    async fn select_cook_and_run_team_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<TeamData>, String> {
        let trace = TraceContext::new();
        self.fetch_teams(&trace, cook_and_run_id, false).await
    }

    async fn select_cook_and_run_team_meta_list(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Vec<TeamMetaData>, String> {
        let trace = TraceContext::new();
        self.fetch_all_team_metas(&trace, cook_and_run_id).await
    }

    async fn select_cook_and_run_team(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<TeamData, String> {
        let trace = TraceContext::new();
        let mut team = self
            .fetch_team(&trace, cook_and_run_id, team_id, &TeamAccess::Admin)
            .await?;
        team.note_list = self.fetch_notes(&trace, cook_and_run_id, team_id).await?;
        Ok(team)
    }

    async fn create_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        team: &TeamCreate,
    ) -> Result<(), String> {
        team.validate()?;
        let session = self.require_session()?;
        let trace = TraceContext::new();

        let body = TeamCreateRequest {
            name: &team.name,
            user_id: Some(&session.user.sub),
            // The backend wants the token from the client; the admin can read it back with the team.
            access_token: Uuid::new_v4(),
            address: &team.address,
            mail: &team.mail,
            phone: &team.phone,
            members: &team.members,
            diets: &team.diets,
        };
        self.call_unit(
            &trace,
            Req::post(team_path(cook_and_run_id, team_id)).json(&body)?,
        )
        .await?;

        if team.needs_check {
            // Teams created by the project owner start as `active`, `review` is set with an update.
            let update = TeamUpdate {
                name: team.name.clone(),
                address: team.address.clone(),
                mail: team.mail.clone(),
                phone: team.phone.clone(),
                members: team.members,
                diets: team.diets.clone(),
                needs_check: true,
            };
            self.patch_team(
                &trace,
                cook_and_run_id,
                team_id,
                &TeamAccess::Admin,
                &update,
            )
            .await?;
        }
        Ok(())
    }

    async fn update_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        team: &TeamUpdate,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.patch_team(&trace, cook_and_run_id, team_id, &TeamAccess::Admin, team)
            .await
    }

    async fn delete_team_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(&trace, Req::delete(team_path(cook_and_run_id, team_id)))
            .await?;
        Ok(())
    }

    // ----- notes -----------------------------------------------------------------------------

    async fn select_team_note_list_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
    ) -> Result<Vec<NoteData>, String> {
        let trace = TraceContext::new();
        self.fetch_notes(&trace, cook_and_run_id, team_id).await
    }

    async fn create_team_note_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        note_id: Uuid,
        note_data: &NoteCreate,
    ) -> Result<(), String> {
        note_data.validate()?;
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::post(format!(
                "{}/note/{}",
                team_path(cook_and_run_id, team_id),
                note_id
            ))
            .json(note_data)?,
        )
        .await?;
        Ok(())
    }

    async fn delete_team_note_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        team_id: Uuid,
        note_id: Uuid,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::delete(format!(
                "{}/note/{}",
                team_path(cook_and_run_id, team_id),
                note_id
            )),
        )
        .await?;
        Ok(())
    }

    // ----- plan ------------------------------------------------------------------------------

    async fn select_plan_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<PlanData>, String> {
        let trace = TraceContext::new();
        self.fetch_plan(&trace, cook_and_run_id).await
    }

    async fn update_plan_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        plan: &PlanData,
    ) -> Result<(), String> {
        plan.validate()?;
        let trace = TraceContext::new();
        // Only the hostings are sent; the backend clears its stale marker when a plan is saved.
        let body = PlanUpdateRequest {
            hosting_list: &plan.hosting_list,
        };
        self.call_unit(
            &trace,
            Req::patch(format!("{}/plan", project_path(cook_and_run_id))).json(&body)?,
        )
        .await?;
        Ok(())
    }

    async fn delete_plan_of_cook_and_run(&mut self, cook_and_run_id: Uuid) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::delete(format!("{}/plan", project_path(cook_and_run_id))),
        )
        .await?;
        Ok(())
    }

    // ----- plan config -----------------------------------------------------------------------

    async fn select_plan_config_of_cook_and_run(
        &self,
        cook_and_run_id: Uuid,
    ) -> Result<Option<PlanConfigData>, String> {
        let trace = TraceContext::new();
        self.fetch_plan_config(&trace, cook_and_run_id).await
    }

    async fn update_plan_config_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
        plan_config: &PlanConfigData,
    ) -> Result<(), String> {
        plan_config.validate()?;
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::patch(format!("{}/plan_config", project_path(cook_and_run_id)))
                .json(plan_config)?,
        )
        .await?;
        Ok(())
    }

    async fn delete_plan_config_of_cook_and_run(
        &mut self,
        cook_and_run_id: Uuid,
    ) -> Result<(), String> {
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::delete(format!("{}/plan_config", project_path(cook_and_run_id))),
        )
        .await?;
        Ok(())
    }
}
