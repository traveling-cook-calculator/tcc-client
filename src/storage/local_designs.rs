//! The designs of a local project live in the browser storage (`localStorage`, one entry per
//! project). Designs of a cloud project are stored in the cloud (see `cloud.rs`).
//!
//! Pictures are scaled down before they are stored, because the browser storage holds only a few
//! megabytes.

use uuid::Uuid;

use crate::schedule_design::{DesignLibrary, ScheduleDesign};

/// More than this many bytes of one project do not fit into the browser storage reliably.
pub const STORAGE_BUDGET: usize = 3_500_000;

fn key(cook_and_run_id: Uuid) -> String {
    format!("tcc_schedule_designs_v1_{cook_and_run_id}")
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

pub fn load(cook_and_run_id: Uuid) -> DesignLibrary {
    let Some(json) = storage().and_then(|s| s.get_item(&key(cook_and_run_id)).ok().flatten()) else {
        return DesignLibrary::default();
    };
    let mut library: DesignLibrary = serde_json::from_str(&json).unwrap_or_default();
    library.sanitize();
    library
}

/// The design that is used for the schedule sheets of this project, if there is one.
pub fn active_design(cook_and_run_id: Uuid) -> Option<ScheduleDesign> {
    load(cook_and_run_id).active_design().cloned()
}

pub fn save(cook_and_run_id: Uuid, library: &DesignLibrary) -> Result<(), String> {
    let storage = storage().ok_or("The browser storage is not available.")?;
    let json = serde_json::to_string(library).map_err(|e| e.to_string())?;
    storage.set_item(&key(cook_and_run_id), &json).map_err(|_| {
        "The browser storage is full. Remove pictures you do not need, or use smaller ones.".to_string()
    })
}

/// Removes the designs of a project (it was deleted or moved to the cloud).
pub fn remove(cook_and_run_id: Uuid) {
    if let Some(storage) = storage() {
        let _ = storage.remove_item(&key(cook_and_run_id));
    }
}
