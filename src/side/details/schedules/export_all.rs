//! "Download all plans": every team's sheet as PNG, packed into one ZIP.
//!
//! The page keeps one off-screen sheet (`ExportSheet`) that this function fills with one team after
//! the other; `export` turns each of them into a picture and, at the end, into the ZIP.

use dioxus::prelude::*;
use uuid::Uuid;

use super::super::plan::team_schedule;
use super::super::run_schedule::export::{self, file_stem, unique_names};
use super::super::run_schedule::Schedule;
use crate::data::Storage;
use crate::storage::{MeetingPointData, PlanData};

/// The team that is drawn on the off-screen sheet right now.
#[derive(Clone, PartialEq)]
pub(super) struct ExportSheet {
    /// Changes with every team; the exporter waits for it to appear on the page.
    pub key: String,
    pub schedule: Schedule,
}

/// What `download_all` needs to know about the project (all of it is already loaded by the page).
pub(super) struct Source {
    pub cook_and_run_id: Uuid,
    pub project_name: String,
    pub german: bool,
    pub is_cloud: bool,
    pub plan: PlanData,
    pub start: Option<MeetingPointData>,
    pub end: Option<MeetingPointData>,
    /// Teams that take part, in the order of the files.
    pub teams: Vec<(Uuid, String)>,
    /// Width of the pictures in pixels.
    pub width: u32,
}

/// Returns how many sheets are in the ZIP and how many teams had no sheet (not part of the plan).
pub(super) async fn download_all(
    storage: Storage,
    source: Source,
    mut sheet: Signal<Option<ExportSheet>>,
    mut progress: Signal<Option<(usize, usize)>>,
) -> Result<(usize, usize), String> {
    export::start().await?;

    let total = source.teams.len();
    let stems: Vec<String> = source.teams.iter().map(|(_, name)| file_stem(&source.project_name, name)).collect();
    let file_names = unique_names(&stems);

    let (mut done, mut skipped) = (0, 0);
    for (index, ((team_id, _), file_name)) in source.teams.iter().zip(&file_names).enumerate() {
        progress.set(Some((index, total)));

        // The guard is only held while the sheet is loaded, never while it is drawn.
        let schedule = {
            let backend = storage.read().await;
            team_schedule(
                &backend,
                source.cook_and_run_id,
                *team_id,
                &source.plan,
                source.is_cloud,
                &source.start,
                &source.end,
            )
            .await?
        };
        let Some(schedule) = schedule else {
            skipped += 1;
            continue;
        };

        let key = format!("{index}-{team_id}");
        sheet.set(Some(ExportSheet { key: key.clone(), schedule }));
        export::add_sheet(&key, file_name, source.width).await?;
        done += 1;
    }
    progress.set(Some((total, total)));

    if done == 0 {
        return Err("No team is part of the plan yet. Calculate the plan first.".to_string());
    }
    let kind = if source.german { "Laufzettel" } else { "Schedules" };
    export::finish(&format!("{}_{kind}", source.project_name)).await?;
    Ok((done, skipped))
}
