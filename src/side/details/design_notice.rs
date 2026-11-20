//! The organiser's reminder that teams only see their plan once a design for it is in use.
//!
//! A cloud project without an active design shows no plan to its teams (the server refuses to
//! hand it out) and cannot send route mails (they wear the colours of the design). The notice
//! appears on the Calculation page and the Schedules page; local projects have no teams that
//! look at a plan, so they never see it.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::data::use_storage;
use crate::ui::*;
use crate::Route;

/// `Some(true)` once a design is in use for the teams, `None` while it is not known yet (or
/// could not be read: no reminder is better than a wrong one).
pub(crate) fn use_design_in_use(cook_and_run_id: Uuid, is_cloud: bool) -> Option<bool> {
    let storage = use_storage();
    let designs = use_resource(move || async move {
        let _ = storage.revision();
        if !is_cloud {
            return None;
        }
        storage.read().await.select_designs(cook_and_run_id).await.ok().map(|list| list.active.is_some())
    });
    designs.read_unchecked().clone().flatten()
}

/// The reminder itself. `in_use`: what `use_design_in_use` said, or the page's own knowledge
/// (the Schedules page changes it without a reload).
#[component]
pub fn NoDesignNotice(
    cook_and_run_id: Uuid,
    in_use: Option<bool>,
    /// On the Schedules page the way to fix it is right there.
    #[props(default)]
    on_schedules_page: bool,
) -> Element {
    if in_use != Some(false) {
        return rsx! {};
    }
    rsx! {
        Notice { tone: NoticeTone::Attention, roomy: true, class: "space-y-2", title: "No design is in use for the plan yet",
            p {
                "Until a design is chosen, teams do not see their plan on their team page, and route mails cannot be sent. "
                if on_schedules_page {
                    "Create a design from a template, save it to the cloud and press \"Use for teams\"."
                } else {
                    "Create a design on the Schedules page, save it to the cloud and press \"Use for teams\"."
                }
            }
            if !on_schedules_page {
                div {
                    Link {
                        to: Route::Schedules { cook_and_run_id },
                        class: button_class(ButtonVariant::Secondary),
                        "Go to Schedules"
                    }
                }
            }
        }
    }
}
