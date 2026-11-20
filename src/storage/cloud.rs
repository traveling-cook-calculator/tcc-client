use std::{collections::HashSet, fmt};

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use reqwest::{header::CONTENT_TYPE, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;
use web_sys::console;

use crate::{
    design_wire,
    keycloak::{AuthState, SessionData},
    schedule_design::{RemoteImage, ScheduleDesign},
    storage::{
        design::{
            self, CloudDesign, CloudLibrary, DesignError, DesignErrorKind, DesignFetch, DesignList,
            DesignMeta, ImageData, ImageMode, LoadedDesign,
        },
        validate_cancel_reason, AddressData, AuditAction, AuditActorType, AuditChangedField,
        AuditLogEntry, CookAndRunCreate, CookAndRunData, CookAndRunMetaData, CookAndRunMetaUpdate,
        CourseCreate, CourseData, CourseUpdate, HealthData, HealthStatus, HostingData,
        MeetingPointData, NoteCreate, NoteData, Page, PaginationData, PlanData,
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
    /// `Authorization: Bearer <access token of the logged in user>`. Without a login the request
    /// is not sent at all ("No auth data!").
    Bearer,
    /// `x-access-token: <team token>`, the bearer token is deliberately not sent.
    Token(String),
}

/// The bearer token a request carries. `Bearer` needs a login; the other variants never send it.
fn bearer_token(auth: &Auth, access_token: Option<String>) -> Result<Option<String>, &'static str> {
    match auth {
        Auth::Bearer => access_token.map(Some).ok_or("No auth data!"),
        Auth::Anonymous | Auth::Token(_) => Ok(None),
    }
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
    /// Raw bytes (a picture); sent as `application/octet-stream`.
    raw: Option<Vec<u8>>,
    /// Extra headers such as `If-Match`.
    headers: Vec<(&'static str, String)>,
}

impl Req {
    fn new(method: Method, path: String) -> Self {
        Req {
            method,
            path,
            query: Vec::new(),
            auth: Auth::Bearer,
            body: None,
            raw: None,
            headers: Vec::new(),
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

    fn header(mut self, name: &'static str, value: impl ToString) -> Self {
        self.headers.push((name, value.to_string()));
        self
    }

    fn raw(mut self, bytes: Vec<u8>) -> Self {
        self.raw = Some(bytes);
        self
    }

    fn json_text(mut self, body: String) -> Self {
        self.body = Some(body);
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
    /// The organiser's id: only in the organiser's view of a team.
    #[serde(default)]
    created_by_user: Option<String>,
    created: DateTime<Utc>,
    edited: DateTime<Utc>,
    status: TeamStatus,
    canceled_at: Option<DateTime<Utc>>,
    cancel_reason: Option<String>,
    email_verified_at: Option<DateTime<Utc>>,
    /// Not part of what a team gets to see of itself (the caller knows its token).
    #[serde(default)]
    last_route_hash: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    verification_resends_left: Option<u8>,
}

impl TeamResponse {
    /// `known_token`: the token the caller used (a team's own view does not repeat it).
    fn into_team(
        self,
        note_list: Vec<NoteData>,
        known_token: Option<Uuid>,
    ) -> Result<TeamData, String> {
        let access_token = match (&self.access_token, known_token) {
            (Some(token), _) => Uuid::parse_str(token)
                .map_err(|e| format!("Team {} has an invalid access token: {}", self.id, e))?,
            (None, Some(token)) => token,
            (None, None) => return Err(format!("Team {} came without an access token", self.id)),
        };
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
                verification_resends_left: self.verification_resends_left,
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
    /// Bot trap (the field is called `website` on the wire); only bots fill it.
    #[serde(rename = "website", skip_serializing_if = "Option::is_none")]
    honeypot: Option<&'a str>,
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
    #[serde(default)]
    allowed_fields: Option<Vec<RequiredField>>,
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
            allowed_fields: self
                .allowed_fields
                .unwrap_or_else(|| self.required_fields.clone()),
            required_fields: self.required_fields,
            max_teams: self.max_teams,
            registration_deadline: self.registration_deadline.map(to_naive),
            edit_deadline: self.edit_deadline.map(to_naive),
            review_trigger_fields: self.review_trigger_fields,
            notify_admin_on_review: self.notify_admin_on_review,
            notify_admin_on_create: self.notify_admin_on_create,
            notify_admin_on_cancel: self.notify_admin_on_cancel,
            created: to_naive(self.created),
            registration_full: false,
            plan_published: false,
        }
    }
}

/// `GET .../share_team_config/public`: the organizer's configuration without `notify_admin_*`.
/// (`project_name` is part of the response and ignored here.)
#[derive(Debug, Deserialize)]
struct PublicShareConfigResponse {
    invite_text: String,
    require_email_verification: bool,
    default_needs_check: bool,
    /// Older servers: what is required was what was shown.
    #[serde(default)]
    allowed_fields: Option<Vec<RequiredField>>,
    required_fields: Vec<RequiredField>,
    max_teams: Option<u32>,
    registration_deadline: Option<DateTime<Utc>>,
    edit_deadline: Option<DateTime<Utc>>,
    review_trigger_fields: Vec<RequiredField>,
    #[serde(default)]
    registration_full: bool,
    #[serde(default)]
    plan_published: bool,
    created: DateTime<Utc>,
}

impl PublicShareConfigResponse {
    fn into_config(self) -> ShareTeamConfig {
        ShareTeamConfig {
            invite_text: self.invite_text,
            require_email_verification: self.require_email_verification,
            default_needs_check: self.default_needs_check,
            allowed_fields: self
                .allowed_fields
                .unwrap_or_else(|| self.required_fields.clone()),
            required_fields: self.required_fields,
            registration_full: self.registration_full,
            plan_published: self.plan_published,
            max_teams: self.max_teams,
            registration_deadline: self.registration_deadline.map(to_naive),
            edit_deadline: self.edit_deadline.map(to_naive),
            review_trigger_fields: self.review_trigger_fields,
            // Not sent to guests; the participant pages never read them.
            notify_admin_on_review: false,
            notify_admin_on_create: false,
            notify_admin_on_cancel: false,
            created: to_naive(self.created),
        }
    }
}

fn public_share_config_path(project_id: Uuid) -> String {
    format!("{}/share_team_config/public", project_path(project_id))
}

#[derive(Debug, Serialize)]
struct ShareConfigRequest {
    invite_text: String,
    require_email_verification: bool,
    default_needs_check: bool,
    allowed_fields: Vec<RequiredField>,
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
            allowed_fields: config.allowed_fields.clone(),
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

#[derive(Debug, Deserialize)]
struct DesignMetaResponse {
    id: Uuid,
    name: String,
    ratio: f32,
    revision: u32,
    updated_at: DateTime<Utc>,
    #[serde(default)]
    approx_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct DesignListResponse {
    #[serde(default)]
    active: Option<Uuid>,
    data: Vec<DesignMetaResponse>,
}

/// `design_id` is mandatory in the body, `null` switches the design off.
#[derive(Debug, Serialize)]
struct ActiveDesignRequest {
    design_id: Option<Uuid>,
}

/// Where the pictures of a design are read from.
#[derive(Clone, Copy)]
enum ImageSource<'a> {
    Owner(Uuid),
    Team(Uuid, Uuid, &'a TeamAccess),
}

fn design_error(error: ApiError) -> DesignError {
    let kind = design::classify(error.status.map(|status| status.as_u16()), &error.message);
    let message = if error.message.is_empty() { error.to_string() } else { error.message };
    DesignError::new(kind, message)
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

    /// Swaps in a renewed session (new access token) without reconnecting. Anonymous
    /// (participant) connections stay anonymous.
    pub fn update_auth(&mut self, auth_state: AuthState) {
        if self.auth_state.is_some() {
            self.auth_state = Some(auth_state);
        }
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
        let bearer = bearer_token(&req.auth, self.session().map(|session| session.access_token))
            .map_err(ApiError::local)?;

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
            for (name, value) in &req.headers {
                builder = builder.header(*name, value.as_str());
            }
            if let Some(body) = &req.body {
                builder = builder
                    .header(CONTENT_TYPE, "application/json")
                    .body(body.clone());
            } else if let Some(bytes) = &req.raw {
                builder = builder
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(bytes.clone());
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
        let known_token = match access {
            TeamAccess::Token(token) => Some(*token),
            TeamAccess::Admin => None,
        };
        response.into_team(Vec::new(), known_token)
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

    /// The full configuration, for the organizer only (login required, project owner).
    /// Guests use [`Self::select_public_share_config`].
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

    /// The configuration as the registration page and the team page need it, **without a login**
    /// (`GET .../share_team_config/public`). The `notify_admin_*` flags are organizer-only and come
    /// back as `false`; do not use the result to edit the configuration.
    pub async fn select_public_share_config(
        &self,
        project_id: Uuid,
    ) -> Result<Option<ShareTeamConfig>, String> {
        let trace = TraceContext::new();
        let req = Req::get(public_share_config_path(project_id)).auth(Auth::Anonymous);
        match self.call_json::<PublicShareConfigResponse>(&trace, req).await {
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
            honeypot: team.honeypot.as_deref(),
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

    /// Confirms the email address: needs the team token AND the secret of the mail link.
    pub async fn verify_team_email(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        access_token: Uuid,
        secret: Uuid,
    ) -> Result<(), String> {
        #[derive(Serialize)]
        struct VerifyRequest {
            secret: String,
        }
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::post(format!("{}/verify", team_path(project_id, team_id)))
                .auth(Auth::Token(access_token.to_string()))
                .json(&VerifyRequest {
                    secret: secret.to_string(),
                })?,
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

    /// The data stored about the team, exactly as the API returns it (JSON text, for the download).
    pub async fn export_team(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        access_token: Uuid,
    ) -> Result<String, String> {
        let trace = TraceContext::new();
        let response = self
            .call(
                &trace,
                Req::get(team_path(project_id, team_id)).auth(Auth::Token(access_token.to_string())),
            )
            .await?;
        response
            .text()
            .await
            .map_err(|e| format!("Response could not be read: {}", e))
    }

    /// Everything stored about the logged in user (JSON text, Art. 15 and 20 GDPR).
    pub async fn export_account_data(&self) -> Result<String, String> {
        let trace = TraceContext::new();
        let response = self.call(&trace, Req::get("/me/export".to_string())).await?;
        response
            .text()
            .await
            .map_err(|e| format!("Response could not be read: {}", e))
    }

    /// Deletes all projects of the logged in user with everything that belongs to them (Art. 17
    /// GDPR). Returns the number of deleted projects. Repeating the call is safe.
    pub async fn erase_account_data(&self) -> Result<u64, String> {
        #[derive(Deserialize)]
        struct Erased {
            deleted_projects: u64,
        }
        let trace = TraceContext::new();
        let erased: Erased = self
            .call_json(&trace, Req::delete("/me".to_string()).query("confirm", "true"))
            .await?;
        Ok(erased.deleted_projects)
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

    // ----- designs ("Laufzettel") ------------------------------------------------------------
    //
    // See `design.rs` for the types and `design_wire.rs` for the JSON. Owners use the `Bearer`
    // routes under `/project/{id}`, teams the read-only routes under `/team/{team_id}/design` with
    // their token (the owner's JWT works there as well, for the preview).

    pub async fn select_designs(&self, project_id: Uuid) -> Result<DesignList, DesignError> {
        let trace = TraceContext::new();
        self.fetch_design_list(&trace, project_id).await
    }

    async fn fetch_design_list(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
    ) -> Result<DesignList, DesignError> {
        let response: DesignListResponse = self
            .call_json(trace, Req::get(format!("{}/designs", project_path(project_id))))
            .await
            .map_err(design_error)?;
        Ok(DesignList {
            active: response.active,
            designs: response
                .data
                .into_iter()
                .map(|meta| DesignMeta {
                    id: meta.id,
                    name: meta.name,
                    ratio: meta.ratio,
                    revision: meta.revision,
                    updated_at: to_naive(meta.updated_at),
                    approx_bytes: meta.approx_bytes,
                })
                .collect(),
        })
    }

    /// `GET` of a design. `if_none_match` is the `ETag` of the copy the caller already has.
    async fn fetch_design_at(
        &self,
        trace: &TraceContext,
        path: String,
        auth: Auth,
        if_none_match: Option<&str>,
    ) -> Result<DesignFetch, DesignError> {
        let mut req = Req::get(path).auth(auth);
        if let Some(etag) = if_none_match {
            req = req.header("If-None-Match", etag);
        }
        let response = self.execute(trace, &req).await.map_err(design_error)?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(DesignFetch::NotModified);
        }
        if !response.status().is_success() {
            return Err(design_error(ApiError::from_response(response).await));
        }
        let etag = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let text = response
            .text()
            .await
            .map_err(|e| DesignError::other(format!("Response could not be read: {e}")))?;
        let design = design_wire::from_wire(&text).map_err(DesignError::other)?;
        Ok(DesignFetch::Modified(LoadedDesign { design, etag }))
    }

    /// The design as the owner stored it, without pictures (their `data_url` is empty).
    #[allow(dead_code)] // part of the design API; the editor goes through `save_design`/`load_*`
    pub async fn select_design(
        &self,
        project_id: Uuid,
        design_id: Uuid,
        if_none_match: Option<&str>,
    ) -> Result<DesignFetch, DesignError> {
        let trace = TraceContext::new();
        self.fetch_design_at(
            &trace,
            format!("{}/design/{}", project_path(project_id), design_id),
            Auth::Bearer,
            if_none_match,
        )
        .await
    }

    async fn fetch_image_at(
        &self,
        trace: &TraceContext,
        path: String,
        auth: Auth,
    ) -> Result<ImageData, DesignError> {
        let response = self
            .call(trace, Req::get(path).auth(auth))
            .await
            .map_err(design_error)?;
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let sha256 = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(|etag| etag.trim().trim_start_matches("W/").trim_matches('"').to_string())
            .filter(|etag| !etag.is_empty());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| DesignError::other(format!("The picture could not be read: {e}")))?
            .to_vec();
        Ok(ImageData { bytes, content_type, sha256 })
    }

    #[allow(dead_code)] // part of the design API; the editor goes through `save_design`/`load_*`
    pub async fn select_design_image(
        &self,
        project_id: Uuid,
        image_id: Uuid,
    ) -> Result<ImageData, DesignError> {
        let trace = TraceContext::new();
        self.fetch_image_at(
            &trace,
            format!("{}/design_image/{}", project_path(project_id), image_id),
            Auth::Bearer,
        )
        .await
    }

    /// Stores a picture. Repeating it with the same id and bytes is fine (same answer).
    #[allow(dead_code)] // part of the design API; the editor goes through `save_design`/`load_*`
    pub async fn upload_design_image(
        &self,
        project_id: Uuid,
        image_id: Uuid,
        bytes: Vec<u8>,
    ) -> Result<RemoteImage, DesignError> {
        let trace = TraceContext::new();
        self.upload_image(&trace, project_id, image_id, bytes).await
    }

    async fn upload_image(
        &self,
        trace: &TraceContext,
        project_id: Uuid,
        image_id: Uuid,
        bytes: Vec<u8>,
    ) -> Result<RemoteImage, DesignError> {
        if bytes.len() > design_wire::MAX_IMAGE_BYTES {
            return Err(DesignError::new(
                DesignErrorKind::TooLarge,
                "A picture is larger than 2 MiB. Use a smaller picture.",
            ));
        }
        let response = self
            .call(
                trace,
                Req::post(format!("{}/design_image/{}", project_path(project_id), image_id))
                    .raw(bytes),
            )
            .await
            .map_err(design_error)?;
        let text = response
            .text()
            .await
            .map_err(|e| DesignError::other(format!("Response could not be read: {e}")))?;
        design_wire::remote_from_upload_answer(&text).map_err(DesignError::other)
    }

    pub async fn delete_design(&self, project_id: Uuid, design_id: Uuid) -> Result<(), DesignError> {
        let trace = TraceContext::new();
        self.call_unit(
            &trace,
            Req::delete(format!("{}/design/{}", project_path(project_id), design_id)),
        )
        .await
        .map_err(design_error)?;
        design::forget_design(&design::cache_key_owner(project_id, design_id));
        Ok(())
    }

    /// The design the teams see; `None` shows them nothing (the classic sheet).
    pub async fn set_active_design(
        &self,
        project_id: Uuid,
        design_id: Option<Uuid>,
    ) -> Result<(), DesignError> {
        let trace = TraceContext::new();
        let body = ActiveDesignRequest { design_id };
        self.call_unit(
            &trace,
            Req::patch(format!("{}/active_design", project_path(project_id)))
                .json(&body)
                .map_err(DesignError::other)?,
        )
        .await
        .map_err(design_error)
    }

    /// Saves a design: first the pictures that are not in the project yet, then the design
    /// itself (`POST` if `etag` is `None`, otherwise `PATCH` with `If-Match`). Returns the new
    /// `ETag`. `design` is updated with what the backend told about the pictures; a picture whose
    /// id is taken by a different picture gets a new id.
    pub async fn save_design(
        &self,
        project_id: Uuid,
        design_id: Uuid,
        design: &mut ScheduleDesign,
        etag: Option<&str>,
    ) -> Result<String, DesignError> {
        let trace = TraceContext::new();
        design.sanitize();
        design_wire::validate(design).map_err(|e| DesignError::new(DesignErrorKind::Invalid, e))?;

        // 1. Pictures first. One at a time: the backend decodes only two at once per pod.
        let pending: Vec<Uuid> = design
            .images
            .iter()
            .filter(|image| image.builtin_key().is_none() && image.remote.is_none())
            .map(|image| image.id)
            .collect();
        for original in pending {
            let mut id = original;
            let image = design
                .images
                .iter()
                .find(|image| image.id == id)
                .cloned()
                .ok_or_else(|| DesignError::other("A picture of the design disappeared"))?;
            let (bytes, _) = design::data_url_to_bytes(&image.data_url).map_err(|_| {
                DesignError::new(
                    DesignErrorKind::Invalid,
                    format!("images: the picture \"{}\" is not available. Remove it or add it again.", image.name),
                )
            })?;
            let mut attempts = 0;
            loop {
                match self.upload_image(&trace, project_id, id, bytes.clone()).await {
                    Ok(remote) => {
                        if let Some(entry) = design.images.iter_mut().find(|entry| entry.id == id) {
                            entry.remote = Some(remote);
                        }
                        break;
                    }
                    // The id belongs to a different picture: take a new one, rewrite the uses.
                    Err(e) if e.is(DesignErrorKind::ImageIdTaken) && attempts < 2 => {
                        attempts += 1;
                        let new_id = Uuid::new_v4();
                        design.reassign_image_id(id, new_id);
                        id = new_id;
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // 2. The design.
        let body = design_wire::to_wire(design).map_err(|e| DesignError::new(DesignErrorKind::Invalid, e))?;
        let path = format!("{}/design/{}", project_path(project_id), design_id);
        let new_etag = match etag {
            None => {
                self.call_unit(&trace, Req::post(path.clone()).json_text(body))
                    .await
                    .map_err(design_error)?;
                None
            }
            Some(etag) => {
                let response = self
                    .call(&trace, Req::patch(path.clone()).json_text(body).header("If-Match", etag))
                    .await
                    .map_err(design_error)?;
                response
                    .headers()
                    .get("etag")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string)
            }
        };
        design::forget_design(&design::cache_key_owner(project_id, design_id));
        match new_etag {
            Some(etag) => Ok(etag),
            // A new design (or an answer without the header): ask for it.
            None => match self.fetch_design_at(&trace, path, Auth::Bearer, None).await? {
                DesignFetch::Modified(LoadedDesign { etag: Some(etag), .. }) => Ok(etag),
                _ => Err(DesignError::other("The backend sent no ETag for the saved design")),
            },
        }
    }

    /// Loads the pictures of `design` (the ones that are stored in the cloud) into `data_url`.
    /// Returns how many could not be loaded.
    async fn fill_images(
        &self,
        trace: &TraceContext,
        source: ImageSource<'_>,
        design: &mut ScheduleDesign,
        mode: ImageMode,
    ) -> usize {
        let wanted: Vec<(Uuid, RemoteImage)> = design
            .images
            .iter()
            .filter_map(|image| image.remote.clone().map(|remote| (image.id, remote)))
            .collect();
        let mut missing = 0;
        for (id, remote) in wanted {
            let known = (mode == ImageMode::Blob)
                .then(|| design::cached_blob_url(&remote.sha256))
                .flatten();
            let url = match known {
                Some(url) => Some(url),
                None => {
                    let path = match source {
                        ImageSource::Owner(project_id) => {
                            format!("{}/design_image/{}", project_path(project_id), id)
                        }
                        ImageSource::Team(project_id, team_id, _) => {
                            format!("{}/design/images/{}", team_path(project_id, team_id), id)
                        }
                    };
                    let auth = match source {
                        ImageSource::Owner(_) => Auth::Bearer,
                        ImageSource::Team(_, _, access) => auth_for(access),
                    };
                    match self.fetch_image_at(trace, path, auth).await {
                        Ok(data) => {
                            let sha = data.sha256.clone().unwrap_or_else(|| remote.sha256.clone());
                            match mode {
                                ImageMode::Blob => {
                                    design::blob_url(&data.bytes, &data.content_type, &sha).ok()
                                }
                                ImageMode::Data => {
                                    Some(design::bytes_to_data_url(&data.bytes, &data.content_type))
                                }
                            }
                        }
                        Err(e) => {
                            console::warn_1(&format!("Design picture {id} could not be loaded: {e}").into());
                            None
                        }
                    }
                }
            };
            match url {
                Some(url) => {
                    if let Some(image) = design.images.iter_mut().find(|image| image.id == id) {
                        image.data_url = url;
                    }
                }
                None => missing += 1,
            }
        }
        missing
    }

    /// One design of the owner with its pictures (for reloading after a conflict).
    pub async fn load_design(
        &self,
        project_id: Uuid,
        design_id: Uuid,
        mode: ImageMode,
    ) -> Result<(CloudDesign, usize), DesignError> {
        let trace = TraceContext::new();
        let path = format!("{}/design/{}", project_path(project_id), design_id);
        match self.fetch_design_at(&trace, path, Auth::Bearer, None).await? {
            DesignFetch::Modified(mut loaded) => {
                let missing = self
                    .fill_images(&trace, ImageSource::Owner(project_id), &mut loaded.design, mode)
                    .await;
                Ok((CloudDesign { id: design_id, etag: loaded.etag, design: loaded.design }, missing))
            }
            DesignFetch::NotModified => Err(DesignError::other("Unexpected answer: not modified")),
        }
    }

    /// Every design of the project with its pictures, and which one is active.
    pub async fn load_design_library(
        &self,
        project_id: Uuid,
        mode: ImageMode,
    ) -> Result<CloudLibrary, DesignError> {
        let trace = TraceContext::new();
        let list = self.fetch_design_list(&trace, project_id).await?;
        let mut library = CloudLibrary { active: list.active, ..CloudLibrary::default() };
        for meta in list.designs {
            let path = format!("{}/design/{}", project_path(project_id), meta.id);
            match self.fetch_design_at(&trace, path, Auth::Bearer, None).await {
                Ok(DesignFetch::Modified(mut loaded)) => {
                    library.missing_images += self
                        .fill_images(&trace, ImageSource::Owner(project_id), &mut loaded.design, mode)
                        .await;
                    library.designs.push(CloudDesign {
                        id: meta.id,
                        etag: loaded.etag,
                        design: loaded.design,
                    });
                }
                // Deleted since the list was read.
                Err(e) if e.is(DesignErrorKind::NotFound) => {}
                Ok(DesignFetch::NotModified) => {}
                Err(e) => return Err(e),
            }
        }
        if library.active.is_some_and(|id| library.designs.iter().all(|design| design.id != id)) {
            library.active = None;
        }
        Ok(library)
    }

    /// The design the teams see, for the owner's own view of the plan. `None` if none is active.
    pub async fn load_active_design(
        &self,
        project_id: Uuid,
    ) -> Result<Option<ScheduleDesign>, DesignError> {
        let trace = TraceContext::new();
        let list = self.fetch_design_list(&trace, project_id).await?;
        let Some(design_id) = list.active else { return Ok(None) };
        let key = design::cache_key_owner(project_id, design_id);
        let cached = design::cached_design(&key);
        let path = format!("{}/design/{}", project_path(project_id), design_id);
        match self
            .fetch_design_at(&trace, path, Auth::Bearer, cached.as_ref().map(|(etag, _)| etag.as_str()))
            .await
        {
            Ok(DesignFetch::NotModified) => Ok(cached.map(|(_, design)| design)),
            Ok(DesignFetch::Modified(mut loaded)) => {
                self.fill_images(&trace, ImageSource::Owner(project_id), &mut loaded.design, ImageMode::Blob)
                    .await;
                if let Some(etag) = &loaded.etag {
                    design::remember_design(&key, etag, &loaded.design);
                }
                Ok(Some(loaded.design))
            }
            Err(e) if e.is(DesignErrorKind::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The design a team sees: the active one. `Ok(None)` if there is none (`404`); a team that
    /// canceled gets `DesignErrorKind::TeamCanceled`. Sends the `ETag` of the last copy.
    pub async fn load_team_design(
        &self,
        project_id: Uuid,
        team_id: Uuid,
        access: &TeamAccess,
    ) -> Result<Option<ScheduleDesign>, DesignError> {
        let trace = TraceContext::new();
        let key = design::cache_key_team(project_id, team_id);
        let cached = design::cached_design(&key);
        let path = format!("{}/design", team_path(project_id, team_id));
        match self
            .fetch_design_at(&trace, path, auth_for(access), cached.as_ref().map(|(etag, _)| etag.as_str()))
            .await
        {
            Ok(DesignFetch::NotModified) => match cached {
                Some((_, design)) => Ok(Some(design)),
                None => Err(DesignError::other("Unexpected answer: not modified")),
            },
            Ok(DesignFetch::Modified(mut loaded)) => {
                self.fill_images(
                    &trace,
                    ImageSource::Team(project_id, team_id, access),
                    &mut loaded.design,
                    ImageMode::Blob,
                )
                .await;
                if let Some(etag) = &loaded.etag {
                    design::remember_design(&key, etag, &loaded.design);
                }
                Ok(Some(loaded.design))
            }
            Err(e) if e.is(DesignErrorKind::NotFound) => {
                design::forget_design(&key);
                Ok(None)
            }
            Err(e) => Err(e),
        }
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
        // One trace for the whole composite load: metadata, courses, teams (+notes), points, plan.
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
            honeypot: None,
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
}

#[cfg(test)]
mod auth_tests {
    use super::*;

    #[test]
    fn a_guest_needs_no_login_for_the_public_share_config() {
        // The registration page of the share link has no session: nothing may be required.
        assert_eq!(bearer_token(&Auth::Anonymous, None), Ok(None));
    }

    #[test]
    fn the_public_share_config_has_its_own_path() {
        let project = Uuid::from_u128(1);
        let path = public_share_config_path(project);
        assert!(path.ends_with("/share_team_config/public"), "{path}");
        assert!(path.starts_with(&project_path(project)), "{path}");
    }

    #[test]
    fn the_public_share_config_parses_without_the_organizer_flags() {
        let body = r#"{
            "project_name": "Dinner",
            "invite_text": "Hi",
            "require_email_verification": true,
            "default_needs_check": false,
            "required_fields": ["mail", "diets"],
            "max_teams": 12,
            "registration_deadline": "2026-12-01T10:00:00Z",
            "edit_deadline": null,
            "review_trigger_fields": ["members"],
            "created": "2026-11-01T08:30:00Z"
        }"#;
        let config = serde_json::from_str::<PublicShareConfigResponse>(body).unwrap().into_config();
        assert_eq!(config.invite_text, "Hi");
        assert!(config.require_email_verification && !config.default_needs_check);
        assert_eq!(config.required_fields, vec![RequiredField::Mail, RequiredField::Diets]);
        assert_eq!(config.review_trigger_fields, vec![RequiredField::Members]);
        assert_eq!(config.max_teams, Some(12));
        assert!(config.registration_deadline.is_some() && config.edit_deadline.is_none());
        assert!(!config.notify_admin_on_review && !config.notify_admin_on_create && !config.notify_admin_on_cancel);
    }

    #[test]
    fn owner_requests_still_need_a_login() {
        assert_eq!(bearer_token(&Auth::Bearer, None), Err("No auth data!"));
        assert_eq!(bearer_token(&Auth::Bearer, Some("jwt".into())), Ok(Some("jwt".to_string())));
    }

    #[test]
    fn team_token_and_anonymous_requests_never_send_the_bearer_token() {
        assert_eq!(bearer_token(&Auth::Token("t".into()), Some("jwt".into())), Ok(None));
        assert_eq!(bearer_token(&Auth::Anonymous, Some("jwt".into())), Ok(None));
    }
}
