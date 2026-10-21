mod address_connector;
mod app;
mod data;
mod footer;
mod layout;
mod side;
mod state;
mod time;
mod ui;
mod web;

// Logic without UI lives in the library crate (`lib.rs`) and is compiled once. Re-exported here
// so `crate::storage::…` paths keep working everywhere.
pub use tcc::{calculator, config, keycloak, storage};

use crate::layout::Wrapper;
use crate::ui::{button_class, ButtonVariant, Card, Centered};
use dioxus::prelude::*;
use side::{
    Calculate, Callback, CookieSettings, Courses, Dashboard, Impressum, Menu, Overview, Plan,
    Privacy, ShareRegisterPage, StartEnd, TeamPage, TeamVerifyPage, Teams,
};
use uuid::Uuid;

pub use crate::keycloak::AuthState;
// Toast API lives in the UI kit; re-exported so existing
// `crate::{ToastMessage, trigger_error_toast}` imports keep working.
pub use crate::ui::toast::{trigger_error_toast, ToastMessage};

fn main() {
    dioxus::launch(app::App);
}

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
enum Route {
    // ── Haupt-App (mit Header + Auth-Button + Footer) ──
    #[layout(Wrapper)]
        #[route("/")]
        Home {},
        #[route("/callback?:code&:state")]
        Callback { code: String, state: String },
        #[nest("/cook-and-run")]
            #[route("/")]
            Dashboard {},
            #[route("/:cook_and_run_id/share")]
            ShareRegisterPage { cook_and_run_id: Uuid },
            // Personal link of a team; the secret token is part of the path.
            #[route("/:cook_and_run_id/team/:team_id/:access_token")]
            TeamPage { cook_and_run_id: Uuid, team_id: Uuid, access_token: Uuid },
            // Target of the link in the verification mail: confirms the email address, then shows the team.
            #[route("/:cook_and_run_id/team/:team_id/:access_token/verify")]
            TeamVerifyPage { cook_and_run_id: Uuid, team_id: Uuid, access_token: Uuid },
            #[nest("/:cook_and_run_id")]
                #[layout(Menu)]
                    #[route("/overview")]
                    Overview { cook_and_run_id: Uuid },
                    #[route("/teams")]
                    Teams { cook_and_run_id: Uuid },
                    #[route("/startend")]
                    StartEnd { cook_and_run_id: Uuid },
                    #[route("/courses")]
                    Courses { cook_and_run_id: Uuid },
                    #[route("/calculate")]
                    Calculate { cook_and_run_id: Uuid },
                    #[route("/plan/:team_id")]
                    Plan { cook_and_run_id: Uuid, team_id: Uuid },
                #[end_layout]
            #[end_nest]
            #[route("/impressum")]
            Impressum {},
            #[route("/datenschutz")]
            Privacy {},
            #[route("/cookies")]
            CookieSettings {},
        #[end_nest]
    #[end_layout]
    #[route("/:..route")]
    NotFound { route: Vec<String> },
}

impl Route {
    /// Route as an OAuth `state` string: a plain URL path. Uses the `Display`
    /// that `#[derive(Routable)]` generates (replaces 100 lines of hand-written
    /// `"overview.<uuid>"` formatting and parsing).
    /// The callback and 404 routes map to the start page to avoid redirect loops.
    pub fn to_state(&self) -> String {
        match self {
            Route::Callback { .. } | Route::NotFound { .. } => Route::Home {}.to_string(),
            route => route.to_string(),
        }
    }

    /// Inverse of [`Route::to_state`]. Unknown or legacy-format values end up on the dashboard.
    pub fn from_state(state: &str) -> Route {
        match state.parse::<Route>() {
            Ok(Route::Callback { .. }) | Ok(Route::NotFound { .. }) | Err(_) => Route::Dashboard {},
            Ok(route) => route,
        }
    }
}

// ─────────────────────────────────────────────
//  Home page
// ─────────────────────────────────────────────

#[component]
fn Home() -> Element {
    rsx! {
        Centered {
            div { class: "text-center mb-10",
                p { class: "text-[11px] font-semibold tracking-[0.15em] uppercase text-amber-600 mb-2",
                    "Cook & Run"
                }
                h1 { class: "text-4xl font-bold text-zinc-900 mb-3", "Traveling Cook Calculator" }
                p { class: "text-base text-zinc-500 max-w-sm mx-auto leading-relaxed",
                    "Plan your cooking and running events with ease."
                }
            }
            Card { class: "w-full max-w-sm",
                div { class: "flex justify-center",
                    Link { to: Route::Dashboard {}, class: button_class(ButtonVariant::Primary), "Get Started" }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Not found page
// ─────────────────────────────────────────────

#[component]
fn NotFound(route: Vec<String>) -> Element {
    rsx! {
        Centered {
            Card { title: "Page not found", class: "w-full max-w-sm",
                p { class: "text-sm text-zinc-500", "The page you're looking for doesn't exist." }
                p { class: "text-xs text-zinc-400 font-mono bg-zinc-50 rounded-lg px-3 py-1.5 border border-zinc-100 break-all",
                    "/{route.join(\"/\")}"
                }
                Link { to: Route::Dashboard {}, class: button_class(ButtonVariant::Primary), "Back to Dashboard" }
            }
        }
    }
}
