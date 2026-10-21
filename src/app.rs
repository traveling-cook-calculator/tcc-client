//! App bootstrap: creates the contexts every page relies on and wires up the
//! start-up flow
//!
//!   storage ─┐
//!   config ──┼─▶ auth ──▶ cloud connection ──▶ `Storage::invalidate()`
//!   consent ─┘
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

    let consent = use_consent_provider();
    let config = use_config_provider(consent);
    let auth = use_auth_provider(config.config);
    use_cloud_sync(storage, auth, config.config);

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

/// Keeps the app-wide cloud connection in line with the login state.
/// Waiting for both inputs is normal during start-up and is not an error.
///
/// * logged in → full connection (own projects)
/// * logged out / unavailable → no cloud connection (local projects only). A connection needs a
///   session, so trying one would always fail; participants of a share link use their own
///   anonymous connection (see `side/details/share_team`).
/// * login in progress → leave everything as it is
fn use_cloud_sync(
    storage: Storage,
    auth: Signal<Option<AuthState>>,
    config: Signal<Option<Config>>,
) {
    let toasts = use_toasts();

    use_effect(move || {
        let (Some(auth), Some(config)) = (auth.read().clone(), config.read().clone()) else {
            return;
        };
        spawn(async move {
            match auth {
                AuthState::Loading(..) => {}
                AuthState::LoggedIn(..) => {
                    if let Err(e) = storage.connect_cloud(auth, config.auth.audience).await {
                        console::warn_1(&format!("Cloud connection unavailable: {e}").into());
                        toasts.error("Cloud connection failed", e);
                    }
                }
                _ => storage.disconnect_cloud().await,
            }
        });
    });
}
