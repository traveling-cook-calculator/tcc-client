//! Which design a sheet is drawn with, and the language switch of the sheets.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::schedule_design::ScheduleDesign;
use crate::side::details::schedules::store as local_store;
use crate::storage::{Language, StorageManager};
use crate::ui::SegmentedTabs;

/// The design that the teams of the project see: for a cloud project the active one from the
/// backend (pictures as `blob:` URLs, an unchanged design is not downloaded again), for a local
/// project the open one from the browser storage. `None`: no design, so no sheet.
///
/// Takes the storage the caller already holds (a second `read()` while a writer waits could block).
pub(crate) async fn load_active_design(
    backend: &StorageManager,
    cook_and_run_id: Uuid,
    is_cloud: bool,
) -> Result<Option<ScheduleDesign>, String> {
    if is_cloud {
        backend.load_active_design(cook_and_run_id).await.map_err(String::from)
    } else {
        Ok(local_store::active_design(cook_and_run_id))
    }
}

/// "Deutsch / English" for the sheets of the owner's pages.
#[component]
pub fn LanguageTabs(value: Language, onchange: EventHandler<Language>) -> Element {
    rsx! {
        SegmentedTabs {
            label: "Sheet language:",
            options: vec!["Deutsch", "English"],
            selected: if value == Language::German { 0 } else { 1 },
            onselect: move |index: usize| {
                onchange.call(if index == 0 { Language::German } else { Language::English })
            },
        }
    }
}
