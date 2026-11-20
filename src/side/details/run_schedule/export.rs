//! Downloading the sheets: file names, and "download all" as one ZIP file.
//!
//! The sheets are pictures made in the browser (html-to-image). For "all" every team's sheet is
//! drawn once on a hidden, off-screen sheet (the page does that, see `schedules`), turned into a PNG
//! here and collected; at the end the PNGs are packed into one ZIP (no compression: PNGs are
//! compressed already) and offered as a single download. That needs no library and does not
//! trigger the browser's "allow multiple downloads" question.

use std::collections::HashMap;

use dioxus::document;

/// `id` of the off-screen sheet that is exported team by team.
pub const EXPORT_SHEET_ID: &str = "tcc-export-sheet";
/// `id` of the element around it; its `data-key` says which team it shows right now.
pub const EXPORT_HOST_ID: &str = "tcc-export-host";

const MAX_PART_CHARS: usize = 80;

/// Removes everything that is not allowed in file names (`/ \ : * ? " < > |`, control characters,
/// HTML tags from titles) and tidies the spaces. Never returns an empty text.
fn clean_part(text: &str, fallback: &str) -> String {
    let mut without_tags = String::with_capacity(text.len());
    let mut inside_tag = false;
    for c in text.chars() {
        match c {
            '<' => inside_tag = true,
            '>' if inside_tag => inside_tag = false,
            _ if !inside_tag => without_tags.push(c),
            _ => {}
        }
    }
    let replaced: String = without_tags
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let single_spaces = replaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = single_spaces.chars().take(MAX_PART_CHARS).collect();
    let tidy = short.trim_matches(|c: char| c == '.' || c == ' ' || c == '_').to_string();
    if tidy.is_empty() { fallback.to_string() } else { tidy }
}

/// The file name (without extension) of a team's sheet: `<project>_<team>`.
pub fn file_stem(project: &str, team: &str) -> String {
    format!("{}_{}", clean_part(project, "Cook & Run"), clean_part(team, "Team"))
}

/// Gives every name a unique one (`A`, `A (2)`, `A (3)`), keeping the order. Two teams with the same
/// name must not overwrite each other inside the ZIP.
pub fn unique_names(names: &[String]) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut taken: std::collections::HashSet<String> = names.iter().cloned().collect();
    let mut result = Vec::with_capacity(names.len());
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    for name in names {
        if used.insert(name.clone()) {
            result.push(name.clone());
            continue;
        }
        let counter = seen.entry(name.clone()).or_insert(1);
        loop {
            *counter += 1;
            let candidate = format!("{name} ({counter})");
            if !taken.contains(&candidate) && used.insert(candidate.clone()) {
                taken.insert(candidate.clone());
                result.push(candidate);
                break;
            }
        }
    }
    result
}

fn js_text(value: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

async fn run_script(script: &str) -> Result<(), String> {
    match document::eval(script).await {
        Ok(serde_json::Value::String(result)) if result == "ok" => Ok(()),
        Ok(serde_json::Value::String(reason)) => Err(reason),
        Ok(other) => Err(format!("Unexpected response: {other}")),
        Err(e) => Err(e.to_string()),
    }
}

/// Forgets the pictures of an earlier run.
pub async fn start() -> Result<(), String> {
    run_script("window.__tccExport = []; return 'ok';").await
}

/// Waits until the off-screen sheet shows the team with `key`, turns it into a PNG of `width`
/// pixels and remembers it as `<file_name>.png`.
pub async fn add_sheet(key: &str, file_name: &str, width: u32) -> Result<(), String> {
    let script = ADD_SHEET_JS
        .replace("__HOST__", &js_text(EXPORT_HOST_ID)?)
        .replace("__SHEET__", &js_text(EXPORT_SHEET_ID)?)
        .replace("__KEY__", &js_text(key)?)
        .replace("__NAME__", &js_text(&format!("{file_name}.png"))?)
        .replace("__WIDTH__", &width.to_string());
    run_script(&script).await
}

/// Packs everything collected into one ZIP and starts the download.
pub async fn finish(zip_name: &str) -> Result<(), String> {
    let script = ZIP_JS.replace("__ZIP__", &js_text(&format!("{}.zip", clean_part(zip_name, "plans")))?);
    run_script(&script).await
}

const ADD_SHEET_JS: &str = r#"
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const key = __KEY__;
let sheet = null;
// The page draws the sheet a moment after it was asked to: wait for the right team.
for (let i = 0; i < 200 && !sheet; i++) {
    const host = document.getElementById(__HOST__);
    if (host && host.dataset.key === key) { sheet = document.getElementById(__SHEET__); }
    if (!sheet) { await wait(25); }
}
if (!sheet) { return 'The sheet could not be prepared.'; }
for (let i = 0; i < 200 && typeof htmlToImage === 'undefined'; i++) { await wait(25); }
if (typeof htmlToImage === 'undefined') { return 'The image library is not loaded yet. Try again in a moment.'; }
try {
    if (document.fonts && document.fonts.ready) { await document.fonts.ready; }
    // Two frames and a short pause: pictures of the sheet are decoded and placed.
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    await wait(120);
    const blob = await htmlToImage.toBlob(sheet, {
        pixelRatio: __WIDTH__ / sheet.clientWidth,
        cacheBust: false,
        filter: (node) => !(node.dataset && node.dataset.noExport),
    });
    if (!blob) { return 'The image could not be created.'; }
    const data = new Uint8Array(await blob.arrayBuffer());
    window.__tccExport = window.__tccExport || [];
    window.__tccExport.push({ name: __NAME__, data });
    return 'ok';
} catch (e) {
    return String(e);
}
"#;

const ZIP_JS: &str = r#"
const files = window.__tccExport || [];
window.__tccExport = [];
if (files.length === 0) { return 'There is nothing to download.'; }
try {
    const encoder = new TextEncoder();
    const table = new Uint32Array(256);
    for (let n = 0; n < 256; n++) {
        let c = n;
        for (let k = 0; k < 8; k++) { c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1); }
        table[n] = c >>> 0;
    }
    const crc32 = (bytes) => {
        let crc = 0xFFFFFFFF;
        for (let i = 0; i < bytes.length; i++) { crc = table[(crc ^ bytes[i]) & 0xFF] ^ (crc >>> 8); }
        return (crc ^ 0xFFFFFFFF) >>> 0;
    };
    const now = new Date();
    const dosTime = (now.getHours() << 11) | (now.getMinutes() << 5) | (now.getSeconds() >> 1);
    const dosDate = ((now.getFullYear() - 1980) << 9) | ((now.getMonth() + 1) << 5) | now.getDate();

    const parts = [];
    const central = [];
    let offset = 0;
    for (const file of files) {
        const name = encoder.encode(file.name);
        const size = file.data.length;
        const crc = crc32(file.data);

        const local = new DataView(new ArrayBuffer(30));
        local.setUint32(0, 0x04034b50, true);
        local.setUint16(4, 20, true);
        local.setUint16(6, 0x0800, true);   // names are UTF-8
        local.setUint16(8, 0, true);        // stored, not compressed
        local.setUint16(10, dosTime, true);
        local.setUint16(12, dosDate, true);
        local.setUint32(14, crc, true);
        local.setUint32(18, size, true);
        local.setUint32(22, size, true);
        local.setUint16(26, name.length, true);
        local.setUint16(28, 0, true);
        parts.push(local.buffer, name, file.data);

        const entry = new DataView(new ArrayBuffer(46));
        entry.setUint32(0, 0x02014b50, true);
        entry.setUint16(4, 20, true);
        entry.setUint16(6, 20, true);
        entry.setUint16(8, 0x0800, true);
        entry.setUint16(10, 0, true);
        entry.setUint16(12, dosTime, true);
        entry.setUint16(14, dosDate, true);
        entry.setUint32(16, crc, true);
        entry.setUint32(20, size, true);
        entry.setUint32(24, size, true);
        entry.setUint16(28, name.length, true);
        entry.setUint32(42, offset, true);
        central.push(entry.buffer, name);

        offset += 30 + name.length + size;
    }
    let centralSize = 0;
    for (const part of central) { centralSize += part.byteLength; }
    const end = new DataView(new ArrayBuffer(22));
    end.setUint32(0, 0x06054b50, true);
    end.setUint16(8, files.length, true);
    end.setUint16(10, files.length, true);
    end.setUint32(12, centralSize, true);
    end.setUint32(16, offset, true);

    const blob = new Blob([...parts, ...central, end.buffer], { type: 'application/zip' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.download = __ZIP__;
    link.href = url;
    document.body.appendChild(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 30000);
    return 'ok';
} catch (e) {
    return String(e);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_name_is_project_and_team() {
        assert_eq!(file_stem("Sommerfest 2026", "Pasta Power"), "Sommerfest 2026_Pasta Power");
    }

    #[test]
    fn characters_that_file_systems_reject_are_replaced() {
        assert_eq!(file_stem("A/B: C?", "x\\y*\"z"), "A_B_ C_x_y__z");
        assert_eq!(file_stem("<b>Fest</b>", "  Team   Rot  "), "Fest_Team Rot");
    }

    #[test]
    fn empty_names_get_a_fallback_and_long_names_are_cut() {
        assert_eq!(file_stem("", "   "), "Cook & Run_Team");
        assert_eq!(file_stem("...", "Team"), "Cook & Run_Team");
        let long = "x".repeat(300);
        assert_eq!(file_stem(&long, "T").chars().count(), MAX_PART_CHARS + 2);
    }

    #[test]
    fn umlauts_and_emoji_stay() {
        assert_eq!(file_stem("Größenwahn", "Küchenfeen 🍳"), "Größenwahn_Küchenfeen 🍳");
    }

    #[test]
    fn equal_names_get_numbers_in_order() {
        let names: Vec<String> = ["A", "B", "A", "A", "B"].iter().map(|s| s.to_string()).collect();
        assert_eq!(unique_names(&names), vec!["A", "B", "A (2)", "A (3)", "B (2)"]);
    }

    #[test]
    fn a_number_never_collides_with_a_real_name() {
        let names: Vec<String> = ["A", "A (2)", "A"].iter().map(|s| s.to_string()).collect();
        let unique = unique_names(&names);
        assert_eq!(unique.len(), 3);
        let set: std::collections::HashSet<_> = unique.iter().collect();
        assert_eq!(set.len(), 3);
    }
}
