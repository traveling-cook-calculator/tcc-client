//! The JSON a design has on the wire (see the contract "Pläne in der Cloud").
//!
//! It is the format of the browser storage with one difference: `images[]` carries no Base64 any
//! more. An entry is either a graphic of the client (`builtin`, with its key) or a picture that is
//! stored in the project (`upload`, with the same id):
//!
//! ```json
//! { "id": "…", "name": "Pot",      "source": { "type": "builtin", "key": "pot" } }
//! { "id": "…", "name": "logo.png", "source": { "type": "upload" } }
//! ```
//!
//! Answers of the backend add `content_type`, `width`, `height`, `bytes` and `sha256` to `upload`.
//! They are read-only: they are not sent back.

use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::schedule_design::{
    builtin_key, DesignImage, RemoteImage, ScheduleDesign, MAX_ALL_IMAGES, MAX_ELEMENTS,
    MAX_IMAGES, MAX_NAME_LEN,
};

/// Largest design JSON the backend takes (`413` above).
pub const MAX_DESIGN_BYTES: usize = 256 * 1024;
/// Largest picture the backend takes (`413` above).
pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;

/// Rules of the backend that can be checked without asking it. The messages name the field first,
/// like the answers of the backend.
pub fn validate(design: &ScheduleDesign) -> Result<(), String> {
    if design.name.trim().is_empty() {
        return Err("name: must not be empty".to_string());
    }
    if design.name.chars().count() > MAX_NAME_LEN {
        return Err(format!("name: must have at most {MAX_NAME_LEN} characters"));
    }
    if design.elements.len() > MAX_ELEMENTS {
        return Err(format!("elements: a design can have at most {MAX_ELEMENTS} elements"));
    }
    if design.images.len() > MAX_ALL_IMAGES {
        return Err(format!("images: a design can have at most {MAX_ALL_IMAGES} entries"));
    }
    if design.uploaded_images().count() > MAX_IMAGES {
        return Err(format!("images: a design can have at most {MAX_IMAGES} uploaded pictures"));
    }
    Ok(())
}

/// The design as the backend wants it. Texts, colors and numbers are cleaned first (the same
/// rules apply to everything that is stored), so a design that is fine in the editor is accepted.
pub fn to_wire(design: &ScheduleDesign) -> Result<String, String> {
    let mut design = design.clone();
    design.sanitize();
    validate(&design)?;

    let mut value = serde_json::to_value(&design).map_err(|e| e.to_string())?;
    let images: Vec<Value> = design
        .images
        .iter()
        .map(|image| {
            let source = match image.builtin_key() {
                Some(key) => json!({ "type": "builtin", "key": key }),
                None => json!({ "type": "upload" }),
            };
            json!({ "id": image.id, "name": image.name, "source": source })
        })
        .collect();
    value["images"] = Value::Array(images);

    let text = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    if text.len() > MAX_DESIGN_BYTES {
        return Err(format!(
            "The design is too big ({} KiB, at most {} KiB). Remove elements or texts.",
            text.len() / 1024 + 1,
            MAX_DESIGN_BYTES / 1024
        ));
    }
    Ok(text)
}

/// A design from the backend. Uploaded pictures have no `data_url` yet (the pictures are loaded
/// separately), but their [`RemoteImage`].
pub fn from_wire(text: &str) -> Result<ScheduleDesign, String> {
    let mut value: Value =
        serde_json::from_str(text).map_err(|e| format!("The design could not be read: {e}"))?;
    let entries = match value.get("images") {
        Some(Value::Array(entries)) => entries.clone(),
        Some(Value::Null) | None => Vec::new(),
        Some(_) => return Err("The design could not be read: images is not a list".to_string()),
    };

    let mut local_images = Vec::with_capacity(entries.len());
    let mut remote: Vec<(Uuid, RemoteImage)> = Vec::new();
    for entry in &entries {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or("The design could not be read: a picture has no valid id")?;
        let name = entry.get("name").and_then(Value::as_str).unwrap_or_default();
        let source = entry.get("source").unwrap_or(&Value::Null);
        let data_url = match source.get("type").and_then(Value::as_str) {
            Some("builtin") => {
                let key = source.get("key").and_then(Value::as_str).unwrap_or_default();
                format!("builtin:{key}")
            }
            Some("upload") => {
                remote.push((id, remote_image(source)));
                String::new()
            }
            other => {
                return Err(format!(
                    "The design could not be read: unknown picture source {}",
                    other.unwrap_or("(none)")
                ))
            }
        };
        let mut object = Map::new();
        object.insert("id".into(), json!(id));
        object.insert("name".into(), json!(name));
        object.insert("data_url".into(), json!(data_url));
        local_images.push(Value::Object(object));
    }
    value["images"] = Value::Array(local_images);

    let mut design: ScheduleDesign =
        serde_json::from_value(value).map_err(|e| format!("The design could not be read: {e}"))?;
    for (id, meta) in remote {
        if let Some(image) = design.images.iter_mut().find(|image| image.id == id) {
            image.remote = Some(meta);
        }
    }
    Ok(design)
}

fn remote_image(source: &Value) -> RemoteImage {
    let number = |key: &str| source.get(key).and_then(Value::as_u64).unwrap_or(0);
    let text = |key: &str| source.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    RemoteImage {
        content_type: text("content_type"),
        width: number("width") as u32,
        height: number("height") as u32,
        bytes: number("bytes"),
        sha256: text("sha256"),
    }
}

/// The metadata the backend answers an upload with.
pub fn remote_from_upload_answer(text: &str) -> Result<RemoteImage, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| format!("The answer of the upload could not be read: {e}"))?;
    let meta = remote_image(&value);
    if meta.sha256.is_empty() {
        return Err("The answer of the upload has no checksum".to_string());
    }
    Ok(meta)
}

/// What a design is worth for the quota of the project: its uploaded pictures.
pub fn uploaded_bytes(design: &ScheduleDesign) -> u64 {
    design
        .images
        .iter()
        .filter(|image| builtin_key(&image.data_url).is_none())
        .map(|image| image.remote.as_ref().map_or(0, |remote| remote.bytes))
        .sum()
}

/// `true` if the design has pictures that still have to be uploaded.
pub fn has_pending_uploads(design: &ScheduleDesign) -> bool {
    design
        .images
        .iter()
        .any(|image| image.builtin_key().is_none() && image.remote.is_none())
}

/// Convenience for tests and migration: an uploaded picture entry.
pub fn uploaded_image(id: Uuid, name: &str, data_url: &str) -> DesignImage {
    DesignImage::new(id, name, data_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule_design::{ElementKind, Preset};

    const PNG: &str = "data:image/png;base64,iVBORw0KGgo=";

    #[test]
    fn builtin_and_upload_entries_have_the_wire_form() {
        let mut design = Preset::Sunny.build();
        let id = Uuid::new_v4();
        design.images.push(uploaded_image(id, "logo.png", PNG));
        let wire: Value = serde_json::from_str(&to_wire(&design).unwrap()).unwrap();
        let images = wire["images"].as_array().unwrap();

        let pot = images.iter().find(|i| i["name"] == "Pot").unwrap();
        assert_eq!(pot["source"], json!({ "type": "builtin", "key": "pot" }));
        let logo = images.iter().find(|i| i["id"] == json!(id)).unwrap();
        assert_eq!(logo["source"], json!({ "type": "upload" }));
        // No Base64 and no read-only fields are sent.
        assert!(!to_wire(&design).unwrap().contains("base64"));
        assert!(logo.get("data_url").is_none());
    }

    #[test]
    fn a_design_from_the_backend_gets_its_pictures_back() {
        let id = Uuid::new_v4();
        let text = format!(
            r##"{{ "version": 1, "name": "Mine", "ratio": 1.25, "background_color": "#ffffff",
                "background_image": null, "elements": [],
                "images": [
                  {{ "id": "{id}", "name": "logo.png", "source": {{ "type": "upload",
                     "content_type": "image/png", "width": 640, "height": 480, "bytes": 52311,
                     "sha256": "abc" }} }},
                  {{ "id": "{}", "name": "Pot", "source": {{ "type": "builtin", "key": "pot" }} }}
                ] }}"##,
            Uuid::new_v4()
        );
        let design = from_wire(&text).unwrap();
        let upload = design.images.iter().find(|i| i.id == id).unwrap();
        assert!(upload.data_url.is_empty());
        let remote = upload.remote.as_ref().unwrap();
        assert_eq!((remote.width, remote.height, remote.bytes), (640, 480, 52311));
        assert_eq!(remote.sha256, "abc");
        let pot = design.images.iter().find(|i| i.name == "Pot").unwrap();
        assert_eq!(pot.data_url, "builtin:pot");
        assert!(pot.remote.is_none());
    }

    #[test]
    fn presets_survive_the_trip_over_the_wire() {
        for preset in Preset::ALL {
            let design = preset.build();
            let again = from_wire(&to_wire(&design).unwrap()).unwrap();
            assert_eq!(design, again, "{}", design.name);
        }
    }

    #[test]
    fn unknown_sources_are_refused() {
        let text = r##"{ "name": "x", "ratio": 1.0, "background_color": "#fff", "elements": [],
            "images": [ { "id": "00000000-0000-0000-0000-000000000001", "name": "a",
                          "source": { "type": "url" } } ] }"##;
        assert!(from_wire(text).is_err());
    }

    #[test]
    fn texts_are_cleaned_before_they_are_sent() {
        let mut design = ScheduleDesign::blank("A\r\nB");
        design.elements.push(crate::schedule_design::Element::text(
            "t\tx",
            "one\r\ntwo\rthree\u{202E}\u{0}",
            0.0,
            0.0,
            10.0,
            10.0,
            3.0,
        ));
        design.background_color = "#FFFFFF".into();
        let wire: Value = serde_json::from_str(&to_wire(&design).unwrap()).unwrap();
        assert_eq!(wire["name"], "A B");
        assert_eq!(wire["background_color"], "#ffffff");
        assert_eq!(wire["elements"][0]["name"], "t x");
        assert_eq!(wire["elements"][0]["kind"]["text"], "one\ntwo\nthree");
    }

    #[test]
    fn the_limits_of_the_backend_are_checked_up_front() {
        let mut design = ScheduleDesign::blank("x");
        for n in 0..=MAX_IMAGES {
            design.images.push(uploaded_image(Uuid::new_v4(), &format!("p{n}"), PNG));
        }
        let error = validate(&design).unwrap_err();
        assert!(error.starts_with("images:"), "{error}");

        // Too much text for 256 KiB.
        let mut big = ScheduleDesign::blank("big");
        for _ in 0..60 {
            let mut element = crate::schedule_design::Element::text("t", "x", 0.0, 0.0, 1.0, 1.0, 3.0);
            if let ElementKind::Text(style) = &mut element.kind {
                style.text = "x".repeat(2000);
                style.missing = crate::schedule_design::Missing::Text("y".repeat(40));
            }
            big.elements.push(element);
        }
        // 60 * 2 KB is far below the limit: fine.
        assert!(to_wire(&big).is_ok());
    }

    #[test]
    fn pending_uploads_are_recognized() {
        let mut design = Preset::Sunny.build();
        assert!(!has_pending_uploads(&design));
        design.images.push(uploaded_image(Uuid::new_v4(), "a", PNG));
        assert!(has_pending_uploads(&design));
        design.images.last_mut().unwrap().remote = Some(RemoteImage {
            content_type: "image/png".into(),
            width: 1,
            height: 1,
            bytes: 10,
            sha256: "x".into(),
        });
        assert!(!has_pending_uploads(&design));
        assert_eq!(uploaded_bytes(&design), 10);
    }

    #[test]
    fn the_answer_of_an_upload_is_read() {
        let text = r#"{"id":"00000000-0000-0000-0000-000000000001","content_type":"image/png","width":640,"height":480,"bytes":52311,"sha256":"deadbeef"}"#;
        let meta = remote_from_upload_answer(text).unwrap();
        assert_eq!(meta.width, 640);
        assert_eq!(meta.sha256, "deadbeef");
        assert!(remote_from_upload_answer("{}").is_err());
    }
}
