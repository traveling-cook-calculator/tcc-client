//! The team's plan, as computed by the backend for its token.

use dioxus::prelude::*;
use uuid::Uuid;

use super::super::lang::Lang;
use crate::data::Storage;
use crate::side::details::plan::PlanContent;
use crate::side::details::run_schedule::Schedule;
use crate::storage::{Language, PlanConfigData, TeamAccess};
use crate::ui::*;

/// Mounted only while the tab is open, so nothing is requested before the team looks at it.
#[component]
pub fn PlanTab(
    storage: Storage,
    cook_and_run_id: Uuid,
    team_id: Uuid,
    access_token: Uuid,
    lang: Lang,
) -> Element {
    let t = lang.txt();
    let plan = use_resource(move || async move {
        let backend = storage.read().await;
        let team_plan = backend.select_team_plan(cook_and_run_id, team_id, &TeamAccess::Token(access_token)).await?;
        // Title, date and the meeting points only dress up the sheet; it works without them
        // (they may not be readable with a team token).
        let config = backend.select_plan_config_of_cook_and_run(cook_and_run_id).await.ok().flatten();
        let start = backend.select_cook_and_run_start_point(cook_and_run_id).await.ok().flatten();
        let end = backend.select_cook_and_run_end_point(cook_and_run_id).await.ok().flatten();
        Ok::<_, String>((team_plan, config, start, end))
    });

    match &*plan.read_unchecked() {
        None => rsx! {
            div { class: "space-y-3 px-4 sm:px-0", "aria-busy": "true",
                Skeleton { class: "h-8 w-48" }
                Skeleton { class: "h-40 w-full" }
                Skeleton { class: "h-40 w-full" }
                span { class: "sr-only", "Loading…" }
            }
        },
        Some(Err(details)) => rsx! {
            EmptyState { title: t.plan_unavailable_title, text: t.plan_unavailable_body,
                p { class: "text-xs text-zinc-400 break-words max-w-xs", "{details}" }
            }
        },
        Some(Ok((team_plan, config, start, end))) => {
            // The sheet follows the visitor's language, not the organizer's.
            let mut config = config.clone().unwrap_or_else(|| PlanConfigData {
                title: "Cook & Run".to_string(),
                description: String::new(),
                ..PlanConfigData::default()
            });
            config.language = match lang {
                Lang::De => Language::German,
                Lang::En => Language::English,
            };
            rsx! {
                PlanContent { config, schedule: Schedule::from_team_plan(team_plan, start, end) }
            }
        }
    }
}
