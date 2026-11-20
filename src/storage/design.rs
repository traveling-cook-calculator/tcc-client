//! Designs ("Laufzettel") of a cloud project: the types of the API, its errors, and the small
//! helpers for pictures (bytes <-> data URL <-> `blob:` URL) and for the cache of what was loaded.
//!
//! The requests are in `cloud.rs`, the format of the JSON in `design_wire.rs`.

use std::{cell::RefCell, collections::HashMap, fmt};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::NaiveDateTime;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use web_sys::{js_sys, Blob, BlobPropertyBag, Url};

use crate::schedule_design::ScheduleDesign;

// ---------------------------------------------------------------------------
// Types of the API
// ---------------------------------------------------------------------------

/// One line of `GET /project/{id}/designs` (no content, no pictures).
#[derive(Debug, Clone, PartialEq)]
pub struct DesignMeta {
    pub id: Uuid,
    pub name: String,
    pub ratio: f32,
    pub revision: u32,
    pub updated_at: NaiveDateTime,
    pub approx_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DesignList {
    /// The design the teams see, if there is one.
    pub active: Option<Uuid>,
    pub designs: Vec<DesignMeta>,
}

/// A design with the `ETag` it was read with (`"3"` for the owner, a hash for a team).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedDesign {
    pub design: ScheduleDesign,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DesignFetch {
    Modified(LoadedDesign),
    /// `304`: the copy the caller has is still the current one.
    NotModified,
}

/// A picture as the backend stores it (it is re-encoded: WebP becomes PNG).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    pub bytes: Vec<u8>,
    pub content_type: String,
    /// The `ETag` of a picture is its SHA-256.
    pub sha256: Option<String>,
}

/// A design of a project in the cloud, as the editor holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudDesign {
    pub id: Uuid,
    /// `None`: not stored in the cloud yet.
    pub etag: Option<String>,
    pub design: ScheduleDesign,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CloudLibrary {
    pub active: Option<Uuid>,
    pub designs: Vec<CloudDesign>,
    /// Pictures that could not be loaded (the designs show them as empty).
    pub missing_images: usize,
}

/// How loaded pictures are handed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMode {
    /// `blob:` URLs (one per distinct picture), for showing.
    Blob,
    /// `data:` URLs, for moving a design to the browser storage.
    Data,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignErrorKind {
    /// `412`: someone else saved in the meantime.
    Conflict,
    /// `428`: `If-Match` was forgotten (a bug of the client).
    PreconditionRequired,
    /// `404`: unknown design / picture, or (team routes) no design is active.
    NotFound,
    /// `409`: the team canceled; no data is available for it any more.
    TeamCanceled,
    /// `409`: the picture id is taken by a different picture.
    ImageIdTaken,
    /// `409`: the picture is still used by a design.
    ImageInUse,
    /// `409`: the design id exists.
    AlreadyExists,
    /// `400`: 8 designs, 12 pictures or 10 MiB of pictures are reached.
    LimitReached,
    /// `413`
    TooLarge,
    /// `415`
    UnsupportedImage,
    /// `422`: a rule was broken; the message starts with the path of the field.
    Invalid,
    /// `429` after all retries.
    RateLimited,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesignError {
    pub kind: DesignErrorKind,
    pub message: String,
}

impl DesignError {
    pub fn new(kind: DesignErrorKind, message: impl Into<String>) -> Self {
        DesignError { kind, message: message.into() }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::new(DesignErrorKind::Other, message)
    }

    pub fn is(&self, kind: DesignErrorKind) -> bool {
        self.kind == kind
    }

    /// A sentence for a toast: what the backend answered, in plain words.
    pub fn friendly(&self) -> String {
        match self.kind {
            DesignErrorKind::Conflict => {
                "The design was changed on another device or tab in the meantime.".to_string()
            }
            DesignErrorKind::TeamCanceled => "This team has canceled its participation.".to_string(),
            DesignErrorKind::TooLarge => {
                "The design or a picture is too big (design: 256 KiB, picture: 2 MiB).".to_string()
            }
            DesignErrorKind::UnsupportedImage => {
                "Only PNG, JPEG and WebP pictures can be stored, and they must not be animated.".to_string()
            }
            DesignErrorKind::RateLimited => {
                "Too many requests. Please wait a moment and try again.".to_string()
            }
            DesignErrorKind::LimitReached => self.message.clone(),
            _ => self.message.clone(),
        }
    }
}

impl fmt::Display for DesignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.friendly())
    }
}

impl From<DesignError> for String {
    fn from(error: DesignError) -> String {
        error.friendly()
    }
}

impl From<String> for DesignError {
    fn from(message: String) -> Self {
        DesignError::other(message)
    }
}

/// Which kind of error is behind a status and the message of the backend.
pub fn classify(status: Option<u16>, message: &str) -> DesignErrorKind {
    let has = |needle: &str| message.to_lowercase().contains(needle);
    match status {
        Some(412) => DesignErrorKind::Conflict,
        Some(428) => DesignErrorKind::PreconditionRequired,
        Some(404) => DesignErrorKind::NotFound,
        Some(409) if has("canceled its participation") => DesignErrorKind::TeamCanceled,
        Some(409) if has("different content") => DesignErrorKind::ImageIdTaken,
        Some(409) if has("still used") => DesignErrorKind::ImageInUse,
        Some(409) if has("already exists") => DesignErrorKind::AlreadyExists,
        Some(400) if has("limit reached") => DesignErrorKind::LimitReached,
        Some(413) => DesignErrorKind::TooLarge,
        Some(415) => DesignErrorKind::UnsupportedImage,
        Some(422) => DesignErrorKind::Invalid,
        Some(429) => DesignErrorKind::RateLimited,
        _ => DesignErrorKind::Other,
    }
}

/// `true` for the error text of a team that canceled (`409`), for places that only have a string.
pub fn is_team_canceled_message(message: &str) -> bool {
    message.to_lowercase().contains("canceled its participation")
}

// ---------------------------------------------------------------------------
// Pictures
// ---------------------------------------------------------------------------

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The bytes and the MIME type of a `data:image/...;base64,...` URL.
pub fn data_url_to_bytes(data_url: &str) -> Result<(Vec<u8>, String), String> {
    let rest = data_url.strip_prefix("data:").ok_or("The picture is not a data URL")?;
    let (header, payload) = rest.split_once(',').ok_or("The picture is not a data URL")?;
    let mime = header.strip_suffix(";base64").ok_or("The picture is not Base64 encoded")?;
    let bytes = STANDARD
        .decode(payload.trim())
        .map_err(|e| format!("The picture could not be decoded: {e}"))?;
    Ok((bytes, mime.to_string()))
}

pub fn bytes_to_data_url(bytes: &[u8], content_type: &str) -> String {
    format!("data:{content_type};base64,{}", STANDARD.encode(bytes))
}

thread_local! {
    /// `sha256 -> blob: URL`. Pictures are immutable and addressed by their hash, so a URL made
    /// once is good for the whole session (it is never revoked: the pictures are small).
    static BLOB_URLS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// `key -> (ETag, design)`: the last design read for a key (see `cache_key_*`).
    static DESIGNS: RefCell<HashMap<String, (String, ScheduleDesign)>> = RefCell::new(HashMap::new());
}

pub fn cached_blob_url(sha256: &str) -> Option<String> {
    if sha256.is_empty() {
        return None;
    }
    BLOB_URLS.with(|urls| urls.borrow().get(sha256).cloned())
}

/// A `blob:` URL for the bytes; the same hash gives the same URL.
pub fn blob_url(bytes: &[u8], content_type: &str, sha256: &str) -> Result<String, String> {
    if let Some(url) = cached_blob_url(sha256) {
        return Ok(url);
    }
    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array);
    let options = BlobPropertyBag::new();
    options.set_type(content_type);
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|e| format!("The picture could not be prepared: {e:?}"))?;
    let url = Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("The picture could not be prepared: {e:?}"))?;
    if !sha256.is_empty() {
        BLOB_URLS.with(|urls| urls.borrow_mut().insert(sha256.to_string(), url.clone()));
    }
    Ok(url)
}

pub fn cache_key_owner(project_id: Uuid, design_id: Uuid) -> String {
    format!("owner/{project_id}/{design_id}")
}

pub fn cache_key_team(project_id: Uuid, team_id: Uuid) -> String {
    format!("team/{project_id}/{team_id}")
}

pub fn cached_design(key: &str) -> Option<(String, ScheduleDesign)> {
    DESIGNS.with(|designs| designs.borrow().get(key).cloned())
}

pub fn remember_design(key: &str, etag: &str, design: &ScheduleDesign) {
    DESIGNS.with(|designs| {
        designs.borrow_mut().insert(key.to_string(), (etag.to_string(), design.clone()));
    });
}

pub fn forget_design(key: &str) {
    DESIGNS.with(|designs| {
        designs.borrow_mut().remove(key);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_urls_round_trip() {
        let bytes = vec![0u8, 1, 2, 250, 251, 252];
        let url = bytes_to_data_url(&bytes, "image/png");
        assert!(url.starts_with("data:image/png;base64,"));
        let (back, mime) = data_url_to_bytes(&url).unwrap();
        assert_eq!(back, bytes);
        assert_eq!(mime, "image/png");
        assert!(data_url_to_bytes("https://x/y.png").is_err());
        assert!(data_url_to_bytes("data:image/png,AAAA").is_err());
    }

    #[test]
    fn sha256_is_lower_case_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn errors_are_told_apart_by_status_and_message() {
        use DesignErrorKind::*;
        let cases = [
            (Some(412), "The resource was modified in the meantime (If-Match does not match)", Conflict),
            (Some(428), "The If-Match header is required", PreconditionRequired),
            (Some(404), "No active design is set for this project", NotFound),
            (Some(409), "Team has canceled its participation; no data is available", TeamCanceled),
            (Some(409), "Design image with id x already exists with different content", ImageIdTaken),
            (Some(409), "Design image with id x is still used by a design", ImageInUse),
            (Some(409), "Design with id x already exists", AlreadyExists),
            (Some(400), "Design image limit reached for project x", LimitReached),
            (Some(400), "The request body is not valid JSON", Other),
            (Some(413), "", TooLarge),
            (Some(415), "Only PNG, JPEG and WebP images are accepted", UnsupportedImage),
            (Some(422), "elements[3].x: must be a number between -100 and 200", Invalid),
            (Some(429), "Too many requests; retry after 3 seconds", RateLimited),
            (None, "Request error", Other),
        ];
        for (status, message, expected) in cases {
            assert_eq!(classify(status, message), expected, "{status:?} {message}");
        }
    }

    #[test]
    fn a_canceled_team_is_recognized_in_a_plain_message() {
        assert!(is_team_canceled_message(
            "Request failed: 409 Conflict - Team has canceled its participation; no data is available"
        ));
        assert!(!is_team_canceled_message("Request failed: 404"));
    }
}
