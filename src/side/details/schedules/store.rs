//! Two things the browser has to do for the schedule designer: scale an uploaded picture down, and
//! read a design file. The designs of a local project are kept by `storage::local_designs`, those
//! of a cloud project by the backend.

use crate::schedule_design::is_safe_data_url;
pub use crate::storage::local_designs::{active_design, load, save, STORAGE_BUDGET};

/// A picture the user picked, already scaled down.
pub struct PickedImage {
    pub name: String,
    pub data_url: String,
    /// Size of the scaled picture, to give the element the right proportions.
    pub width: u32,
    pub height: u32,
}

/// Reads the file chosen in `<input type="file" id=input_id>`, scales it to at most `max_side`
/// pixels and returns it as a data URL. `Ok(None)` if no file is chosen.
pub async fn read_image(input_id: &str, max_side: u32) -> Result<Option<PickedImage>, String> {
    let id = serde_json::to_string(input_id).map_err(|e| e.to_string())?;
    let js = format!(
        r#"
        try {{
            const input = document.getElementById({id});
            const file = input && input.files && input.files[0];
            if (!file) {{ return {{ none: true }}; }}
            const url = URL.createObjectURL(file);
            const img = new Image();
            await new Promise((resolve, reject) => {{
                img.onload = resolve;
                img.onerror = () => reject(new Error('The browser cannot read this file as a picture.'));
                img.src = url;
            }});
            const scale = Math.min(1, {max_side} / Math.max(img.naturalWidth, img.naturalHeight));
            const width = Math.max(1, Math.round(img.naturalWidth * scale));
            const height = Math.max(1, Math.round(img.naturalHeight * scale));
            const canvas = document.createElement('canvas');
            canvas.width = width;
            canvas.height = height;
            canvas.getContext('2d').drawImage(img, 0, 0, width, height);
            URL.revokeObjectURL(url);
            const transparent = ['image/png', 'image/webp', 'image/gif'].includes(file.type);
            let data = canvas.toDataURL(transparent ? 'image/png' : 'image/jpeg', 0.85);
            if (transparent && data.length > 1200000) {{
                // Too big as PNG: keep the picture, give up the transparency.
                const flat = document.createElement('canvas');
                flat.width = width;
                flat.height = height;
                const context = flat.getContext('2d');
                context.fillStyle = '#ffffff';
                context.fillRect(0, 0, width, height);
                context.drawImage(img, 0, 0, width, height);
                data = flat.toDataURL('image/jpeg', 0.85);
            }}
            input.value = '';
            return {{ name: file.name, data: data, width: width, height: height }};
        }} catch (e) {{
            return {{ error: String(e && e.message ? e.message : e) }};
        }}
        "#
    );
    let value = dioxus::document::eval(&js).await.map_err(|e| e.to_string())?;
    if value.get("none").is_some() {
        return Ok(None);
    }
    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
        return Err(error.to_string());
    }
    let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("picture").to_string();
    let data_url = value.get("data").and_then(|v| v.as_str()).ok_or("The picture could not be read.")?.to_string();
    if !is_safe_data_url(&data_url) {
        return Err("This kind of picture is not supported.".to_string());
    }
    let width = value.get("width").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as u32;
    let height = value.get("height").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as u32;
    Ok(Some(PickedImage { name: name.chars().take(60).collect(), data_url, width, height }))
}

/// Reads the text of the file chosen in `<input type="file" id=input_id>`.
pub async fn read_text(input_id: &str) -> Result<Option<String>, String> {
    let id = serde_json::to_string(input_id).map_err(|e| e.to_string())?;
    let js = format!(
        r#"
        try {{
            const input = document.getElementById({id});
            const file = input && input.files && input.files[0];
            if (!file) {{ return {{ none: true }}; }}
            if (file.size > 12000000) {{ input.value = ''; return {{ error: 'This file is too big for a design.' }}; }}
            const text = await file.text();
            input.value = '';
            return {{ text: text }};
        }} catch (e) {{
            return {{ error: String(e && e.message ? e.message : e) }};
        }}
        "#
    );
    let value = dioxus::document::eval(&js).await.map_err(|e| e.to_string())?;
    if value.get("none").is_some() {
        return Ok(None);
    }
    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
        return Err(error.to_string());
    }
    Ok(value.get("text").and_then(|v| v.as_str()).map(str::to_string))
}
