//! App bootstrap: creates the contexts every page relies on and wires up the
//! start-up flow
//!
//!   storage ─┐
//!   config ──┼─▶ auth ──▶ cloud connection ──▶ `Storage::invalidate()`
//!   consent ─┘      └───▶ session keep-alive (renews the token in the background)
//!
//! Each step is its own small hook, in dependency order.

use crate::config::Config;
use crate::data::Storage;
use crate::keycloak::AuthState;
use crate::layout::FatalScreen;
use crate::state::ConsentState;
use crate::storage::StorageManager;
use crate::ui::{use_toasts, use_toasts_provider};
use crate::Route;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use web_sys::console;

/// Loading state of `/config.json`, shared with the layout.
#[derive(Clone, Copy, PartialEq)]
pub struct ConfigState {
    config: Signal<Option<Config>>,
    error: Signal<Option<String>>,
}

impl ConfigState {
    pub fn is_ready(&self) -> bool {
        self.config.read().is_some()
    }
    pub fn error(&self) -> Option<String> {
        self.error.read().clone()
    }
}

#[component]
pub fn App() -> Element {
    // Runs once. A failing storage (e.g. blocked browser storage) shows an error
    // page instead of panicking into a blank screen.
    let storage = use_hook(|| StorageManager::new().map(Storage::new));

    match storage {
        Ok(storage) => rsx! { AppRoot { storage } },
        Err(error) => {
            console::error_1(&format!("Storage init failed: {error}").into());
            rsx! {
                FatalScreen {
                    error_text: "The local storage of your browser is not available. \
                                 Please allow site data and reload the page.",
                    error_details: error,
                }
            }
        }
    }
}

#[component]
fn AppRoot(storage: Storage) -> Element {
    use_context_provider(|| storage);
    use_toasts_provider();
    use_hook(crate::web::request_persistent_storage);

    let consent = use_consent_provider();
    let config = use_config_provider(consent);
    let auth = use_auth_provider(config.config);
    use_cloud_sync(storage, auth, config.config);
    use_session_keepalive(storage, auth);

    rsx! { Router::<Route> {} }
}

// ─────────────────────────────────────────────
//  Providers
// ─────────────────────────────────────────────

fn use_consent_provider() -> Signal<ConsentState> {
    let consent = use_signal(ConsentState::default);
    use_context_provider(|| consent);
    consent
}

/// Fetches `/config.json` once. The consent state is validated against the
/// configured `consent_version` *before* the config becomes visible, so the
/// banner never flashes up for users who already consented.
fn use_config_provider(mut consent: Signal<ConsentState>) -> ConfigState {
    let mut config = use_signal(|| None::<Config>);
    let mut error = use_signal(|| None::<String>);

    use_future(move || async move {
        match Config::fetch().await {
            Ok(loaded) => {
                consent.set(ConsentState::load(&loaded.legal.consent_version));
                config.set(Some(loaded));
            }
            Err(e) => {
                console::error_1(&format!("Error while loading config: {e}").into());
                error.set(Some(e));
            }
        }
    });

    // `Signal<Option<Config>>` stays available for the legal pages / footer.
    use_context_provider(|| config);
    let state = ConfigState { config, error };
    use_context_provider(|| state);
    state
}

/// Creates the auth state once the config is known.
fn use_auth_provider(config: Signal<Option<Config>>) -> Signal<Option<AuthState>> {
    let mut auth = use_signal(|| None::<AuthState>);
    use_context_provider(|| auth);

    use_effect(move || {
        let Some(config) = config.read().clone() else { return };
        spawn(async move {
            auth.set(Some(AuthState::new(config.auth).await));
        });
    });
    auth
}

/// What the cloud connection has to follow. Token renewals change the `AuthState` as well, but
/// they must not reconnect (health check + reloading every page), so only a change of *who* is
/// logged in counts.
#[derive(Clone, PartialEq)]
enum Login {
    /// Auth not ready yet, or a login is in progress.
    Pending,
    LoggedIn { user: String },
    LoggedOut,
}

/// Keeps the app-wide cloud connection in line with the login state.
/// Waiting for both inputs is normal during start-up and is not an error.
///
/// * logged in → full connection (own projects)
/// * logged out / unavailable → no cloud connection (local projects only). A connection needs a
///   session, so trying one would always fail; participants of a share link use their own
///   anonymous connection (see `side/participant`).
/// * login in progress → leave everything as it is
fn use_cloud_sync(
    storage: Storage,
    auth: Signal<Option<AuthState>>,
    config: Signal<Option<Config>>,
) {
    let toasts = use_toasts();

    // Only re-evaluated when the login *changes*, not when the same user gets a new token.
    let login = use_memo(move || match auth.read().as_ref() {
        None | Some(AuthState::Loading(..)) => Login::Pending,
        Some(AuthState::LoggedIn(_, _, session)) => Login::LoggedIn { user: session.user.sub.clone() },
        Some(_) => Login::LoggedOut,
    });

    use_effect(move || {
        let login = login();
        let Some(config) = config.read().clone() else { return };
        // `peek`: the current state is read, but must not trigger this effect again.
        let Some(state) = auth.peek().clone() else { return };
        spawn(async move {
            match login {
                Login::Pending => {}
                Login::LoggedIn { .. } => {
                    if let Err(e) = storage.connect_cloud(state, config.api_base_url()).await {
                        console::warn_1(&format!("Cloud connection unavailable: {e}").into());
                        toasts.error("Cloud connection failed", e);
                    }
                }
                Login::LoggedOut => storage.disconnect_cloud().await,
            }
        });
    });
}

// ─────────────────────────────────────────────
//  Session keep-alive
// ─────────────────────────────────────────────

/// How often the session is checked. The check itself is local (it only compares the expiry
/// time); Keycloak is contacted only when the token is about to run out.
const SESSION_CHECK_INTERVAL_MS: u32 = 30_000;

/// A token that is valid for less than this is renewed. Larger than the check interval, so a
/// token is always renewed while it is still valid (Keycloak's default lifetime is 5 minutes).
const SESSION_REFRESH_MARGIN_SECS: i64 = 90;

/// Renews the access token in the background, so that a page that stays open for hours keeps
/// working without errors.
///
/// The new token goes to two places: the `auth` signal (header, pages) and the open cloud
/// connection, which holds its own copy of the session and would otherwise keep sending the
/// expired token. A failed renewal (offline, Keycloak down) changes nothing and is retried at the
/// next check; only a rejected refresh token logs the user out.
fn use_session_keepalive(storage: Storage, mut auth: Signal<Option<AuthState>>) {
    let toasts = use_toasts();

    use_future(move || async move {
        loop {
            TimeoutFuture::new(SESSION_CHECK_INTERVAL_MS).await;

            let Some(current) = auth.peek().clone() else { continue };
            if current.access_token().is_none() {
                continue;
            }
            let Some(next) = current.keep_alive(SESSION_REFRESH_MARGIN_SECS).await else { continue };

            // The user may have logged out, or logged in again, while the request was running.
            if auth.peek().as_ref().and_then(AuthState::access_token) != current.access_token() {
                continue;
            }

            match &next {
                AuthState::LoggedIn(..) => storage.update_session(next.clone()).await,
                _ => toasts.info("Session expired", "Please log in again to use the cloud."),
            }
            auth.set(Some(next));
        }
    });
}
