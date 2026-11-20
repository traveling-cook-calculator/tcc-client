//! Small browser helpers (no `unwrap()` chains in pages).

use web_sys::wasm_bindgen::{JsCast, JsValue};
use web_sys::{js_sys, Blob, HtmlAnchorElement, Url};

fn js_err(what: &'static str) -> impl Fn(JsValue) -> String {
    move |e| format!("{what}: {e:?}")
}

/// Offers `content` to the user as a file download.
pub fn download_text(file_name: &str, content: &str) -> Result<(), String> {
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(content));
    let blob = Blob::new_with_str_sequence(&parts).map_err(js_err("Could not create file"))?;
    let url = Url::create_object_url_with_blob(&blob).map_err(js_err("Could not create download link"))?;

    let result = (|| {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| "No document available".to_string())?;
        let body = document.body().ok_or_else(|| "No document body available".to_string())?;
        let anchor = document
            .create_element("a")
            .map_err(js_err("Could not create download link"))?
            .dyn_into::<HtmlAnchorElement>()
            .map_err(|_| "Could not create download link".to_string())?;
        anchor.set_href(&url);
        anchor.set_download(file_name);
        body.append_child(&anchor).map_err(js_err("Could not start download"))?;
        anchor.click();
        let _ = body.remove_child(&anchor);
        Ok(())
    })();

    let _ = Url::revoke_object_url(&url);
    result
}

/// Asks the browser not to delete the app's data when it runs low on disk space (best effort; the
/// browser may ask the user or decide on its own). Local projects live in the browser only.
pub fn request_persistent_storage() {
    let _ = dioxus::document::eval(
        "if (navigator.storage && navigator.storage.persist) { navigator.storage.persist().catch(() => {}); }",
    );
}

/// `https://host` of the running app (for share links).
pub fn origin() -> Option<String> {
    web_sys::window()?.location().origin().ok()
}

/// Copies `text` to the clipboard. Goes through JS because `web-sys` has no clipboard
/// feature enabled; the text is JSON-encoded, so it cannot break out of the script.
pub async fn copy_text(text: &str) -> Result<(), String> {
    let quoted = serde_json::to_string(text).map_err(|e| e.to_string())?;
    let script = format!(
        "return await navigator.clipboard.writeText({quoted}).then(() => true).catch(e => String(e));"
    );
    match dioxus::document::eval(&script).await {
        Ok(serde_json::Value::Bool(true)) => Ok(()),
        Ok(serde_json::Value::String(reason)) => Err(reason),
        Ok(other) => Err(format!("Unexpected response: {other}")),
        Err(e) => Err(e.to_string()),
    }
}

// ─────────────────────────────────────────────
//  Links, session storage and local data
// ─────────────────────────────────────────────

use std::cell::RefCell;
use uuid::Uuid;

/// Key of the cookie consent. It is a record of the visitor's own choice, not project data, and
/// survives "delete my local data".
const CONSENT_KEY: &str = "cook_run_consent";
/// Set while the account deletion at the auth server is under way (see `keycloak.rs`).
const ACCOUNT_DELETION_KEY: &str = "tcc_account_deletion";

thread_local! {
    static INITIAL_HASH: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Remembers the URL fragment as it was when the app started. Must run before the router, which
/// may rewrite the address.
pub fn capture_initial_hash() {
    let hash = web_sys::window().and_then(|w| w.location().hash().ok()).unwrap_or_default();
    INITIAL_HASH.with(|cell| *cell.borrow_mut() = hash);
}

fn session_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok()?
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// The secret token of a team link. Taken from the fragment (`#token=…`) and kept in the session
/// storage of the tab, so a reload (which may lose the fragment) still works. Never persisted
/// beyond the tab.
pub fn team_token(team_id: Uuid) -> Option<Uuid> {
    let key = format!("tcc_team_token_{team_id}");
    let live = web_sys::window().and_then(|w| w.location().hash().ok()).unwrap_or_default();
    let initial = INITIAL_HASH.with(|cell| cell.borrow().clone());
    let from_link = crate::deeplink::token_from_fragment(&live).or_else(|| crate::deeplink::token_from_fragment(&initial));

    if let Some(token) = from_link {
        if let Some(storage) = session_storage() {
            let _ = storage.set_item(&key, &token.to_string());
        }
        return Some(token);
    }
    let stored = session_storage()?.get_item(&key).ok()??;
    Uuid::parse_str(&stored).ok().filter(|token| !token.is_nil())
}

/// The secret of the confirmation link in the invitation mail (`#token=…&verify=…`), if the page
/// was opened through it. Deliberately **not** kept in the session: it is only needed once, and
/// the link the guest keeps and shares with himself must not be able to confirm anything.
pub fn team_verify_secret() -> Option<Uuid> {
    let live = web_sys::window().and_then(|w| w.location().hash().ok()).unwrap_or_default();
    let initial = INITIAL_HASH.with(|cell| cell.borrow().clone());
    crate::deeplink::verify_secret_from_fragment(&live).or_else(|| crate::deeplink::verify_secret_from_fragment(&initial))
}

/// Takes `verify=…` out of the address (keeps `#token=…`): after the address was confirmed the
/// confirmation secret has no use, and the address bar should not keep showing it.
pub fn forget_verify_secret() {
    if let Some(window) = web_sys::window() {
        let hash = window.location().hash().unwrap_or_default();
        let kept: Vec<&str> = hash
            .trim_start_matches('#')
            .split('&')
            .filter(|pair| !pair.starts_with("verify="))
            .collect();
        let _ = window.location().set_hash(&kept.join("&"));
    }
    INITIAL_HASH.with(|cell| {
        let kept: Vec<String> = cell
            .borrow()
            .trim_start_matches('#')
            .split('&')
            .filter(|pair| !pair.starts_with("verify="))
            .map(str::to_string)
            .collect();
        *cell.borrow_mut() = format!("#{}", kept.join("&"));
    });
}


/// Value of a query parameter of the current address.
pub fn query_param(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search).ok()?.get(name)
}

pub fn set_account_deletion_pending(pending: bool) {
    if let Some(storage) = session_storage() {
        let _ = if pending {
            storage.set_item(ACCOUNT_DELETION_KEY, "1")
        } else {
            storage.remove_item(ACCOUNT_DELETION_KEY)
        };
    }
}

pub fn account_deletion_pending() -> bool {
    session_storage().and_then(|storage| storage.get_item(ACCOUNT_DELETION_KEY).ok().flatten()).is_some()
}

/// Removes everything this app stored in the browser (local projects, designs, session) except the
/// cookie consent. Returns the number of removed entries.
pub fn clear_local_data() -> usize {
    let mut removed = 0;
    if let Some(storage) = local_storage() {
        let keys: Vec<String> = (0..storage.length().unwrap_or(0))
            .filter_map(|index| storage.key(index).ok().flatten())
            .filter(|key| key != CONSENT_KEY)
            .collect();
        for key in keys {
            if storage.remove_item(&key).is_ok() {
                removed += 1;
            }
        }
    }
    if let Some(storage) = session_storage() {
        let _ = storage.clear();
    }
    removed
}

/// Loads `path` from scratch (drops all in-memory state).
pub fn reload_to(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}
