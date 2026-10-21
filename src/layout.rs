//! App chrome: header, auth button, footer, toasts and cookie banner around
//! every routed page.

use crate::app::ConfigState;
use crate::footer::Footer;
use crate::keycloak::AuthState;
use crate::side::CookieBanner;
use crate::ui::{use_busy, ErrorView, LoadingView, Spinner, ToastHost};
use crate::Route;
use dioxus::prelude::*;
use web_sys::window;

pub const FAVICON: Asset = asset!("/assets/favicon.ico");
pub const PROFILE: Asset = asset!("/assets/profile.png");
pub const TAILWIND_CSS: Asset = asset!("/assets/output.css");
pub const LOGO: Asset = asset!("/assets/logo.png");

#[component]
pub fn Wrapper() -> Element {
    let route = use_route::<Route>();
    let config = use_context::<ConfigState>();
    let route_state = route.to_state();
    let ready = config.is_ready();
    // Participants of a share link do not need (and must not be nudged to) log in.
    let show_auth = ready
        && !matches!(
            route,
            Route::ShareRegisterPage { .. } | Route::TeamPage { .. } | Route::TeamVerifyPage { .. }
        );

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        ToastHost {}
        if ready {
            CookieBanner {}
        }

        div { class: "min-h-screen w-full flex flex-col bg-page",
            header { class: "sticky top-0 z-50 bg-surface border-b border-amber-200/60 shadow-sm",
                div { class: "max-w-7xl mx-auto px-6 py-3 flex justify-between items-center w-full",
                    Link { to: Route::Dashboard {}, class: "flex items-center gap-3",
                        img { src: LOGO, alt: "Cook & Run", class: "h-8 w-auto" }
                    }
                    if show_auth {
                        div { class: "flex items-center gap-3", AuthButton { route_state } }
                    }
                }
            }

            main { class: "flex flex-1 w-full",
                if let Some(details) = config.error() {
                    ErrorView {
                        error_text: "The application settings could not be loaded. \
                                     Please try again later or contact support.",
                        error_details: details,
                    }
                } else if ready {
                    Outlet::<Route> {}
                } else {
                    LoadingView {}
                }
            }

            if ready {
                Footer {}
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Auth button (one component instead of five copy-pasted click handlers)
// ─────────────────────────────────────────────

const AUTH_BTN: &str = "flex items-center gap-2 px-3 py-1.5 rounded-xl text-sm font-medium \
    text-zinc-600 bg-amber-50 border border-amber-200 \
    hover:bg-amber-100 hover:border-amber-300 \
    transition-colors duration-150 cursor-pointer \
    disabled:opacity-60 disabled:cursor-not-allowed";

#[component]
fn AuthButton(route_state: String) -> Element {
    let mut auth = use_context::<Signal<Option<AuthState>>>();
    let task = use_busy();

    let current = auth.read().clone();
    let (label, pending, disabled, avatar) = match &current {
        None => ("Loading...", true, true, false),
        Some(AuthState::Loading(..)) => ("Loading...", true, false, false),
        Some(AuthState::LoggedIn(..)) => ("Logout", false, false, true),
        Some(AuthState::LoggedOut(..)) | Some(AuthState::Error(..)) => ("Login", false, false, false),
        Some(AuthState::NotAvailable()) => ("Not available", false, true, false),
    };
    let pending = pending || task.is_busy();

    rsx! {
        button {
            r#type: "button",
            class: AUTH_BTN,
            disabled,
            onclick: move |_| {
                // Snapshot at click time; no signal guard is held while awaiting.
                let Some(state) = auth.peek().clone() else { return };
                if matches!(state, AuthState::NotAvailable()) {
                    return;
                }
                let route_state = route_state.clone();
                task.run(async move {
                    let (next, url) = if matches!(state, AuthState::LoggedIn(..)) {
                        state.logout().await
                    } else {
                        state.login(route_state).await
                    };
                    auth.set(Some(next));
                    if let (Some(win), Some(url)) = (window(), url) {
                        let _ = win.location().set_href(&url);
                    }
                });
            },
            if pending {
                Spinner { class: "w-3.5 h-3.5 text-amber-400" }
            }
            if avatar {
                img {
                    src: PROFILE,
                    alt: "Profile",
                    class: "h-6 w-6 rounded-full border border-amber-200",
                }
            }
            "{label}"
        }
    }
}

// ─────────────────────────────────────────────
//  Shown when the app cannot even start (outside the router)
// ─────────────────────────────────────────────

#[component]
pub fn FatalScreen(#[props(into)] error_text: String, #[props(into)] error_details: String) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div { class: "min-h-screen w-full flex flex-col bg-page",
            ErrorView { error_text, error_details }
        }
    }
}
