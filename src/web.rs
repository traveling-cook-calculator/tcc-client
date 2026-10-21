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
