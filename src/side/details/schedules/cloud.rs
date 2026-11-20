//! The designs of a cloud project: what is stored in the backend, and the actions of the editor
//! on it (save, delete, use for the teams, reload after a conflict).
//!
//! The editor keeps all designs of the project in memory (`DesignLibrary`) like for a local
//! project. Next to it, `CloudState` knows for every design its id, the `ETag` it was read or
//! saved with, and a copy of what is stored, so that "unsaved changes" is simply "differs from
//! the copy". Nothing is sent while the user edits: the designs are saved on request, pictures
//! first, and a save that finds a newer version (`412`) asks what to do.

use dioxus::prelude::*;
use uuid::Uuid;

use super::Designer;
use crate::schedule_design::{DesignLibrary, ScheduleDesign, MAX_IMAGES};
use crate::storage::design::{CloudLibrary, DesignErrorKind};
use crate::design_wire::MAX_IMAGE_BYTES;

/// What the backend stores for the pictures of a whole project (all designs together).
pub(super) const PROJECT_IMAGE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CloudEntry {
    pub id: Uuid,
    /// `None`: the design is not in the cloud yet.
    pub etag: Option<String>,
    /// The design as it is stored; `None` while it is not.
    pub saved: Option<ScheduleDesign>,
}

/// Parallel to `DesignLibrary::designs`.
#[derive(Clone, Debug, PartialEq, Default)]
pub(super) struct CloudState {
    pub entries: Vec<CloudEntry>,
    /// The design the teams see.
    pub active: Option<Uuid>,
}

impl CloudState {
    pub fn unsaved() -> CloudEntry {
        CloudEntry { id: Uuid::new_v4(), etag: None, saved: None }
    }
}

/// The editor state of a project that was loaded from the cloud: the open design is the active
/// one (or the first).
pub(super) fn editor_state(loaded: CloudLibrary) -> (DesignLibrary, CloudState) {
    let open = loaded
        .active
        .and_then(|id| loaded.designs.iter().position(|design| design.id == id))
        .unwrap_or(0);
    let entries = loaded
        .designs
        .iter()
        .map(|design| CloudEntry {
            id: design.id,
            etag: design.etag.clone(),
            saved: Some(design.design.clone()),
        })
        .collect();
    let library = DesignLibrary {
        designs: loaded.designs.into_iter().map(|design| design.design).collect(),
        active: open,
    };
    (library, CloudState { entries, active: loaded.active })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::side::details) enum SaveOutcome {
    Saved,
    /// Someone else saved in the meantime: the conflict dialog is open.
    Conflict,
    Failed,
}

impl Designer {
    fn index_of(&self, id: Uuid) -> Option<usize> {
        self.cloud.peek().entries.iter().position(|entry| entry.id == id)
    }

    /// Id of the design that is open in the editor.
    pub fn open_id(&self) -> Option<Uuid> {
        let index = self.library.peek().active;
        self.cloud.peek().entries.get(index).map(|entry| entry.id)
    }

    /// `true` if the design differs from what is stored (or is not stored yet).
    pub fn is_dirty(&self, index: usize) -> bool {
        let library = self.library.read();
        let cloud = self.cloud.read();
        match (library.designs.get(index), cloud.entries.get(index)) {
            (Some(design), Some(entry)) => entry.saved.as_ref() != Some(design),
            _ => false,
        }
    }

    pub fn is_saved(&self, index: usize) -> bool {
        self.cloud.read().entries.get(index).is_some_and(|entry| entry.etag.is_some())
    }

    pub fn is_used_by_teams(&self, index: usize) -> bool {
        let cloud = self.cloud.read();
        cloud.entries.get(index).is_some_and(|entry| cloud.active == Some(entry.id))
    }

    /// Distinct uploaded pictures of all designs of the project and their size (the backend limits
    /// both for the project, not per design).
    pub fn project_pictures(&self) -> (usize, usize) {
        let library = self.library.read();
        let mut seen = std::collections::HashMap::new();
        for design in &library.designs {
            for image in design.uploaded_images() {
                let bytes = match &image.remote {
                    Some(remote) => remote.bytes as usize,
                    None => image.data_url.len() / 4 * 3,
                };
                seen.insert(image.id, bytes);
            }
        }
        (seen.len(), seen.values().sum())
    }

    /// Why a picture of `bytes` bytes cannot be added to the project, if it cannot.
    pub fn picture_quota_error(&self, bytes: usize) -> Option<String> {
        if bytes > MAX_IMAGE_BYTES {
            return Some("A picture can be 2 MiB at most. Use a smaller one.".to_string());
        }
        let (count, used) = self.project_pictures();
        if count >= MAX_IMAGES {
            return Some(format!(
                "A project can store {MAX_IMAGES} pictures in all its designs together. Remove one first."
            ));
        }
        if used + bytes > PROJECT_IMAGE_BYTES {
            return Some("The pictures of the project would be more than 10 MiB. Remove one first.".to_string());
        }
        None
    }

    /// Saves the open design (and with it its pictures).
    pub async fn save_open(self) -> SaveOutcome {
        match self.open_id() {
            Some(id) => self.save_by_id(id).await,
            None => SaveOutcome::Failed,
        }
    }

    async fn save_by_id(self, id: Uuid) -> SaveOutcome {
        let mut library = self.library;
        let mut cloud = self.cloud;
        let Some(index) = self.index_of(id) else { return SaveOutcome::Failed };

        // Clean the design first, so that what is compared later is what is sent.
        library.write().designs[index].sanitize();
        let mut snapshot = library.peek().designs[index].clone();
        let etag = cloud.peek().entries[index].etag.clone();
        let before: Vec<Uuid> = snapshot.images.iter().map(|image| image.id).collect();

        let result = {
            let backend = self.storage.read().await;
            backend.save_design(self.project, id, &mut snapshot, etag.as_deref()).await
        };

        let Some(index) = self.index_of(id) else { return SaveOutcome::Failed };
        match result {
            Ok(new_etag) => {
                // What the backend told about the pictures goes into the open design as well; edits
                // made while it was saving stay (and keep the design "changed").
                {
                    let mut library = library.write();
                    let design = &mut library.designs[index];
                    for (position, image) in snapshot.images.iter().enumerate() {
                        if let Some(old) = before.get(position).copied().filter(|old| *old != image.id) {
                            design.reassign_image_id(old, image.id);
                        }
                        if let Some(target) = design.images.iter_mut().find(|target| target.id == image.id) {
                            target.remote = image.remote.clone();
                        }
                    }
                }
                let mut cloud = cloud.write();
                cloud.entries[index].etag = Some(new_etag);
                cloud.entries[index].saved = Some(snapshot);
                SaveOutcome::Saved
            }
            Err(e) if e.is(DesignErrorKind::Conflict) => {
                let mut conflict = self.conflict;
                conflict.set(Some(id));
                SaveOutcome::Conflict
            }
            Err(e) if e.is(DesignErrorKind::NotFound) && etag.is_some() => {
                // Deleted on another device: the next save creates it again.
                cloud.write().entries[index].etag = None;
                self.toasts.error(
                    "The design is gone from the cloud",
                    "It was deleted on another device. Save again to store it anew.",
                );
                SaveOutcome::Failed
            }
            Err(e) => {
                self.toasts.error("The design could not be saved", e.friendly());
                SaveOutcome::Failed
            }
        }
    }

    /// Saves and tells the user.
    pub async fn save_open_with_toast(self) {
        let name = self.active().map(|design| design.name).unwrap_or_default();
        if self.save_open().await == SaveOutcome::Saved {
            self.toasts.success("Design saved", name);
        }
    }

    /// Conflict, option 1: throw the changes away and take what is stored now.
    pub async fn reload_conflicting(self) {
        let mut conflict = self.conflict;
        let Some(id) = *conflict.peek() else { return };
        let result = { self.storage.read().await.load_design(self.project, id).await };
        match result {
            Ok((loaded, _missing)) => {
                if let Some(index) = self.index_of(id) {
                    let mut library = self.library;
                    let mut cloud = self.cloud;
                    library.write().designs[index] = loaded.design.clone();
                    let mut cloud = cloud.write();
                    cloud.entries[index].etag = loaded.etag;
                    cloud.entries[index].saved = Some(loaded.design);
                }
                conflict.set(None);
                let mut selected = self.selected;
                selected.set(None);
                self.toasts.info("Design reloaded", "Your changes were replaced by the saved version.");
            }
            Err(e) if e.is(DesignErrorKind::NotFound) => {
                if let Some(index) = self.index_of(id) {
                    let mut cloud = self.cloud;
                    cloud.write().entries[index].etag = None;
                }
                conflict.set(None);
                self.toasts.error("The design is gone from the cloud", "Save again to store it anew.");
            }
            Err(e) => self.toasts.error("The saved version could not be loaded", e.friendly()),
        }
    }

    /// Conflict, option 2: keep the changes and save over the newer version.
    pub async fn overwrite_conflicting(self) {
        let mut conflict = self.conflict;
        let Some(id) = *conflict.peek() else { return };
        let result = { self.storage.read().await.load_design(self.project, id).await };
        match result {
            Ok((loaded, _)) => {
                if let Some(index) = self.index_of(id) {
                    let mut cloud = self.cloud;
                    cloud.write().entries[index].etag = loaded.etag;
                }
                conflict.set(None);
                if self.save_by_id(id).await == SaveOutcome::Saved {
                    self.toasts.success("Design saved", "The version from the other device was replaced.");
                }
            }
            Err(e) => self.toasts.error("The saved version could not be checked", e.friendly()),
        }
    }

    /// Deletes the open design from the cloud (and from the editor).
    pub async fn delete_open_design(self) {
        let Some(id) = self.open_id() else { return };
        let stored = self.cloud.peek().entries.iter().any(|entry| entry.id == id && entry.etag.is_some());
        if stored {
            let result = { self.storage.read().await.delete_design(self.project, id).await };
            if let Err(e) = result {
                if !e.is(DesignErrorKind::NotFound) {
                    return self.toasts.error("The design could not be deleted", e.friendly());
                }
            }
        }
        let Some(index) = self.index_of(id) else { return };
        let mut library = self.library;
        let mut cloud = self.cloud;
        {
            let mut library = library.write();
            library.designs.remove(index);
            library.active = library.active.saturating_sub(1).min(library.designs.len().saturating_sub(1));
        }
        {
            let mut cloud = cloud.write();
            cloud.entries.remove(index);
            // The backend switches the active design off when it is deleted.
            if cloud.active == Some(id) {
                cloud.active = None;
            }
        }
        let mut selected = self.selected;
        selected.set(None);
    }

    /// Makes the design (or none) the one the teams see.
    pub async fn set_used_by_teams(self, id: Option<Uuid>) {
        let result = { self.storage.read().await.set_active_design(self.project, id).await };
        match result {
            Ok(()) => {
                let mut cloud = self.cloud;
                cloud.write().active = id;
                match id {
                    Some(_) => self.toasts.success("Design in use", "Teams now see the saved version of this design."),
                    None => self.toasts.info("No design in use", "Teams do not see their plan until a design is in use."),
                }
            }
            Err(e) => self.toasts.error("The design could not be switched", e.friendly()),
        }
    }
}

/// Turns a `blob:` URL (a picture loaded from the cloud) into a `data:` URL, so that the design can
/// be written to a file.
async fn blob_to_data_url(url: &str) -> Result<String, String> {
    let url = serde_json::to_string(url).map_err(|e| e.to_string())?;
    let js = format!(
        r#"
        try {{
            const response = await fetch({url});
            const blob = await response.blob();
            return await new Promise((resolve, reject) => {{
                const reader = new FileReader();
                reader.onload = () => resolve({{ data: reader.result }});
                reader.onerror = () => reject(reader.error);
                reader.readAsDataURL(blob);
            }});
        }} catch (e) {{
            return {{ error: String(e && e.message ? e.message : e) }};
        }}
        "#
    );
    let value = dioxus::document::eval(&js).await.map_err(|e| e.to_string())?;
    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
        return Err(error.to_string());
    }
    value
        .get("data")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "The picture could not be read.".to_string())
}

/// The design with all pictures inline (`data:` URLs), the form of a design file and of the
/// browser storage. Pictures of the cloud are `blob:` URLs in the editor.
pub(super) async fn with_inline_pictures(mut design: ScheduleDesign) -> Result<ScheduleDesign, String> {
    for image in &mut design.images {
        image.remote = None;
        if image.data_url.starts_with("blob:") {
            image.data_url = blob_to_data_url(&image.data_url).await?;
        } else if image.builtin_key().is_none() && image.data_url.is_empty() {
            return Err(format!("The picture \"{}\" could not be loaded from the cloud.", image.name));
        }
    }
    Ok(design)
}
