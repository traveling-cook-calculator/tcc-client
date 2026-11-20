pub(crate) mod address;
pub mod calculate;
pub mod courses;
pub mod design_notice;
pub mod overview;
pub mod plan;
pub(crate) mod plan_editor;
pub(crate) mod plan_bundle;
pub(crate) mod run_schedule;
pub mod schedules;
pub mod startend;
pub mod teams;

use dioxus::prelude::*;
use uuid::Uuid;

use crate::Route;

// ─────────────────────────────────────────────
//  Sidebar / Menu
// ─────────────────────────────────────────────

#[component]
pub fn Menu(cook_and_run_id: Uuid) -> Element {
    let current = use_route::<Route>();

    // (label, target). Order = order in the menu.
    let items = [
        ("Overview", Route::Overview { cook_and_run_id }),
        ("Teams", Route::Teams { cook_and_run_id, team: String::new() }),
        ("Start & End", Route::StartEnd { cook_and_run_id }),
        ("Courses", Route::Courses { cook_and_run_id }),
        ("Calculation", Route::Calculate { cook_and_run_id }),
        ("Schedules", Route::Schedules { cook_and_run_id }),
    ];

    rsx! {
        div { class: "flex flex-1 min-w-0 flex-col md:flex-row",
            nav { class: "w-full md:w-64 shrink-0 bg-page px-4 py-4 md:py-6 shadow-md md:rounded-r-2xl flex flex-col",
                h2 { class: "text-lg font-semibold text-ink-brown px-2 mb-3 md:mb-5 tracking-wide",
                    "Menu"
                }
                ul { class: "flex flex-wrap gap-1 md:flex-col md:flex-nowrap",
                    for (label, target) in items {
                        SidebarLink {
                            key: "{label}",
                            label,
                            active: is_active(&target, &current),
                            target,
                        }
                    }
                }
            }
            Outlet::<Route> {}
        }
    }
}

/// A team's route page belongs to "Teams" even though it has no menu entry of its own.
fn is_active(target: &Route, current: &Route) -> bool {
    target == current
        || matches!(
            (target, current),
            (Route::Teams { .. }, Route::Plan { .. } | Route::Teams { .. })
        )
}

#[component]
fn SidebarLink(label: &'static str, target: Route, active: bool) -> Element {
    let tone = if active {
        "bg-brand text-white shadow-sm"
    } else {
        "text-ink-brown hover:text-brand-hover hover:bg-sand"
    };
    rsx! {
        li { class: "md:w-full",
            Link {
                to: target,
                class: "block text-left w-full px-4 py-2 rounded-xl text-sm font-medium \
                        transition-colors duration-150 {tone}",
                "{label}"
            }
        }
    }
}
