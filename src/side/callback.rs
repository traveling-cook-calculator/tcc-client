//! Target of the Keycloak redirect (`/callback?code=…&state=…`).

use dioxus::prelude::*;

use crate::ui::{use_toasts, LoadingView};
use crate::{AuthState, Route};

#[component]
pub fn Callback(code: String, state: String) -> Element {
    let mut auth = use_context::<Signal<Option<AuthState>>>();
    let toasts = use_toasts();
    let navigator = use_navigator();
    let mut handled = use_signal(|| false);

    // Runs once, as soon as the auth state is known (configuration and discovery load first).
    // All hooks come first; there is no early return before them.
    use_effect(move || {
        let Some(current) = auth.read().clone() else { return };
        // Our own `auth.set(..)` below triggers this effect again; only act once.
        if *handled.peek() {
            return;
        }
        handled.set(true);

        let (code, state) = (code.clone(), state.clone());
        spawn(async move {
            // Where to go afterwards is part of the pending login.
            let return_to = match &current {
                AuthState::Loading(_, _, process) => Some(Route::from_state(&process.return_to)),
                _ => None,
            };

            match return_to {
                Some(route) => {
                    let next = current.callback(&code, &state).await;
                    if let AuthState::Error(_, _, message) = &next {
                        toasts.error("Login failed", message.clone());
                    }
                    auth.set(Some(next));
                    // `replace`: the back button must not return to a callback URL whose code is used up.
                    navigator.replace(route);
                }
                // The address was opened without a pending login (reload, bookmark, abandoned login):
                // there is nothing to process, so do not leave the user on a dead end.
                None => {
                    navigator.replace(Route::Dashboard {});
                }
            }
        });
    });

    rsx! { LoadingView { label: "Authenticating…" } }
}
