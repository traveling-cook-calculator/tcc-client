//! Property panels of the schedule designer: the selected element, the sheet itself (format,
//! background, pictures) and the palette of placeholders.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;

use super::super::run_schedule::builtin::image_src;
use super::{store, Designer};
use crate::schedule_design::{
    is_safe_color, placeholder_catalog, placeholders_in, Align, Element as DesignElement, ElementKind,
    Fit, Font, IconTint, ImageStyle, Missing, ScheduleDesign, ShapeStyle, TextStyle, VAlign,
    BUILTIN_GRAPHICS, FORMATS, GUEST_ITEM_KEYS, MAX_IMAGES,
};
use crate::schedule_text::{block_catalog, insert_block};
use crate::ui::*;

/// A color of the design: the swatch of the browser and the `#rrggbb` text. What is typed only
/// counts once it is a color the sheet can draw (`is_safe_color`).
#[component]
fn DesignColorField(#[props(into)] label: String, value: String, onchange: EventHandler<String>) -> Element {
    let valid = is_safe_color(&value);
    rsx! {
        ColorField {
            label,
            value,
            valid,
            onchange,
            ontext: move |text: String| {
                if is_safe_color(&text) {
                    onchange.call(text);
                }
            },
        }
    }
}

// ─────────────────────────────────────────────
//  Small controls
// ─────────────────────────────────────────────

/// The lines of the icon for aligning text, like in a presentation program.
#[component]
fn LinesIcon(path: &'static str) -> Element {
    rsx! {
        svg { width: "18", height: "18", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round",
            path { d: "{path}" }
        }
    }
}

const ALIGN_LEFT: &str = "M3 5h18M3 10h11M3 15h18M3 20h11";
const ALIGN_CENTER: &str = "M3 5h18M7 10h10M3 15h18M7 20h10";
const ALIGN_RIGHT: &str = "M3 5h18M10 10h11M3 15h18M10 20h11";
const VALIGN_TOP: &str = "M4 3h16M8 8h8v8H8z";
const VALIGN_MIDDLE: &str = "M8 8h8v8H8zM3 12h3M18 12h3";
const VALIGN_BOTTOM: &str = "M4 21h16M8 8h8v8H8z";

// ─────────────────────────────────────────────
//  The selected element
// ─────────────────────────────────────────────

#[component]
pub(super) fn Inspector(element: DesignElement, pictures: Vec<(Uuid, String)>, courses: usize, german: bool) -> Element {
    let ds = use_context::<Designer>();
    let id = element.id;
    let title = match &element.kind {
        ElementKind::Text(_) => "Text box",
        ElementKind::Shape(_) => "Shape",
        ElementKind::Image(_) => "Picture",
        ElementKind::List(_) => "Element",
    };

    rsx! {
        Card { title: "{title}",
            div { class: "flex flex-wrap items-center gap-1",
                ToolButton { title: "Bring forward", on: false, onclick: move |_| ds.edit(|d| d.move_element(id, true)), "↑" }
                ToolButton { title: "Send backward", on: false, onclick: move |_| ds.edit(|d| d.move_element(id, false)), "↓" }
                ToolButton { title: "Duplicate (Ctrl+D)", on: false, onclick: move |_| ds.duplicate_element(id), "⧉" }
                ToolButton { title: "Delete", on: false, onclick: move |_| ds.delete_element(id), "🗑" }
                span { class: "ml-auto",
                    TextButton {
                        label: "Done",
                        onclick: move |_| {
                            let mut selected = ds.selected;
                            selected.set(None);
                        },
                    }
                }
            }
            match element.kind.clone() {
                ElementKind::Text(style) => rsx! { TextPanel { id, style, courses, german } },
                ElementKind::Shape(style) => rsx! { ShapePanel { id, style } },
                ElementKind::Image(style) => rsx! { PicturePanel { id, style, pictures } },
                ElementKind::List(_) => rsx! {},
            }
            DisclosureList {
                Disclosure { title: "More",
                    Slider { label: "Opacity", unit: "", value: element.opacity, min: 0.0, max: 1.0, step: 0.05,
                        onchange: move |v| ds.edit_element(id, |el| el.opacity = v) }
                    Field { label: "Show only if filled",
                        TextInput {
                            value: element.show_if.clone().unwrap_or_default(),
                            placeholder: "e.g. team_diets (empty: always)",
                            oninput: move |e: Event<FormData>| {
                                let key = e.value().trim().trim_matches(['{', '}']).to_string();
                                ds.edit_element(id, |el| el.show_if = if key.is_empty() { None } else { Some(key) });
                            },
                        }
                    }
                    Hint { "Hides this element for teams for which that value is empty, for example a symbol next to the diets." }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Text box
// ─────────────────────────────────────────────

/// Where the cursor is in the text field (UTF-16 units, what the browser reports).
async fn cursor_in_editor(fallback: usize) -> usize {
    let value = document::eval(
        "const el = document.getElementById('design-text-editor'); return el ? [el.selectionStart, el.selectionEnd] : null;",
    )
    .await
    .unwrap_or(serde_json::Value::Null);
    value.get(0).and_then(|v| v.as_u64()).map_or(fallback, |v| v as usize)
}

#[component]
fn TextPanel(id: Uuid, style: TextStyle, courses: usize, german: bool) -> Element {
    let ds = use_context::<Designer>();
    let mut with_icons = use_signal(|| true);
    let groups = block_catalog(courses, german, with_icons());
    let known: Vec<String> = placeholder_catalog(3, 5)
        .into_iter()
        .flat_map(|group| group.items.into_iter().map(|(key, _)| key))
        .chain(GUEST_ITEM_KEYS.iter().map(|(key, _)| key.to_string()))
        .collect();
    let unknown: Vec<String> = placeholders_in(&style.text).into_iter().filter(|key| !known.contains(key)).collect();
    let unknown_text = unknown.iter().map(|k| format!("{{{k}}}")).collect::<Vec<_>>().join(", ");
    let has_icons = style.text.contains("{icon:");
    let has_guests = style.text.contains("{#guests}");
    let missing_text = match &style.missing {
        Missing::Text(text) => text.clone(),
        _ => "–".to_string(),
    };

    // Puts a block at the cursor, on a line of its own.
    let insert = move |snippet: String| {
        spawn(async move {
            let text = ds.text_of(id).unwrap_or_default();
            let cursor = cursor_in_editor(text.encode_utf16().count()).await;
            let (new_text, new_cursor) = insert_block(&text, cursor, &snippet);
            ds.edit_text(id, |s| s.text = new_text);
            TimeoutFuture::new(80).await;
            let _ = document::eval(&format!(
                "const el = document.getElementById('design-text-editor'); if (el) {{ el.focus(); el.setSelectionRange({new_cursor}, {new_cursor}); }}"
            ))
            .await;
        });
    };

    rsx! {
        div { class: "space-y-3",
            // Font, size, color
            div { class: "flex items-center gap-2",
                Select {
                    class: "min-w-0 flex-1",
                    value: style.font.key().to_string(),
                    options: Font::ALL.iter().map(|font| (font.key(), font.label())).collect(),
                    onchange: move |key: String| {
                        if let Some(font) = Font::from_key(&key) {
                            ds.edit_text(id, |s| s.font = font);
                        }
                    },
                }
                ColorInput {
                    title: "Text color",
                    value: style.color.clone(),
                    onchange: move |color: String| ds.edit_text(id, |s| s.color = color),
                }
            }
            // Like in a presentation program: size, style, alignment
            div { class: "flex flex-wrap items-center gap-0.5",
                ToolButton { title: "Smaller", on: false, onclick: move |_| ds.edit_text(id, |s| s.size = (s.size - 0.3).max(0.8)), "A−" }
                span { class: "w-10 text-center font-mono text-xs text-zinc-600", "{style.size:.1}" }
                ToolButton { title: "Larger", on: false, onclick: move |_| ds.edit_text(id, |s| s.size = (s.size + 0.3).min(40.0)), "A+" }
                span { class: "mx-1 h-5 w-px bg-amber-200" }
                ToolButton { title: "Bold", on: style.bold, onclick: move |_| ds.edit_text(id, |s| s.bold = !s.bold), b { "B" } }
                ToolButton { title: "Italic", on: style.italic, onclick: move |_| ds.edit_text(id, |s| s.italic = !s.italic), i { "I" } }
                ToolButton { title: "Capital letters", on: style.uppercase, onclick: move |_| ds.edit_text(id, |s| s.uppercase = !s.uppercase), "AA" }
            }
            div { class: "flex flex-wrap items-center gap-0.5",
                ToolButton { title: "Align left", on: style.align == Align::Left, onclick: move |_| ds.edit_text(id, |s| s.align = Align::Left), LinesIcon { path: ALIGN_LEFT } }
                ToolButton { title: "Center", on: style.align == Align::Center, onclick: move |_| ds.edit_text(id, |s| s.align = Align::Center), LinesIcon { path: ALIGN_CENTER } }
                ToolButton { title: "Align right", on: style.align == Align::Right, onclick: move |_| ds.edit_text(id, |s| s.align = Align::Right), LinesIcon { path: ALIGN_RIGHT } }
                span { class: "mx-1 h-5 w-px bg-amber-200" }
                ToolButton { title: "Align to the top", on: style.valign == VAlign::Top, onclick: move |_| ds.edit_text(id, |s| s.valign = VAlign::Top), LinesIcon { path: VALIGN_TOP } }
                ToolButton { title: "Align to the middle", on: style.valign == VAlign::Middle, onclick: move |_| ds.edit_text(id, |s| s.valign = VAlign::Middle), LinesIcon { path: VALIGN_MIDDLE } }
                ToolButton { title: "Align to the bottom", on: style.valign == VAlign::Bottom, onclick: move |_| ds.edit_text(id, |s| s.valign = VAlign::Bottom), LinesIcon { path: VALIGN_BOTTOM } }
            }

            // The text itself
            div { class: "space-y-1",
                TextArea {
                    id: "design-text-editor",
                    rows: 5,
                    value: style.text.clone(),
                    oninput: move |e: Event<FormData>| {
                        let text = e.value();
                        ds.edit_text(id, |s| s.text = text);
                    },
                }
                if !unknown.is_empty() {
                    FieldError { error: format!("Unknown placeholder: {unknown_text}") }
                }
            }

            // Text blocks: only here, where a text is edited
            div { class: "space-y-1.5",
                div { class: "flex items-center justify-between",
                    Eyebrow { text: "Insert text block" }
                    Checkbox { label: "with icons", checked: with_icons(), onchange: move |on| with_icons.set(on) }
                }
                DisclosureList {
                    for group in groups {
                        Disclosure { key: "{group.title}", title: group.title.to_string(),
                            if !group.note.is_empty() {
                                Hint { "{group.note}" }
                            }
                            div { class: "flex flex-col gap-1",
                                for block in group.blocks {
                                    {
                                        let snippet = block.snippet.clone();
                                        rsx! {
                                            ChipButton { key: "{block.label}", label: block.label.to_string(), onclick: move |_| insert(snippet.clone()) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // How values that are not set look
            div { class: "space-y-1.5",
                Eyebrow { text: "If a value is missing", class: "block" }
                div { class: "flex flex-wrap items-center gap-1.5",
                    ToggleButton { label: "Hide the line", on: style.missing == Missing::Hide,
                        onclick: move |_| ds.edit_text(id, |s| s.missing = Missing::Hide) }
                    ToggleButton { label: "Leave it empty", on: style.missing == Missing::Empty,
                        onclick: move |_| ds.edit_text(id, |s| s.missing = Missing::Empty) }
                    ToggleButton { label: "Show", on: matches!(style.missing, Missing::Text(_)),
                        onclick: {
                            let missing_text = missing_text.clone();
                            move |_| {
                                let text = missing_text.clone();
                                ds.edit_text(id, |s| s.missing = Missing::Text(text));
                            }
                        } }
                    if matches!(style.missing, Missing::Text(_)) {
                        div { class: "w-28",
                            TextInput {
                                value: missing_text.clone(),
                                oninput: move |e: Event<FormData>| {
                                    let text: String = e.value().chars().take(40).collect();
                                    ds.edit_text(id, |s| s.missing = Missing::Text(text));
                                },
                            }
                        }
                    }
                }
                Hint {
                    match style.missing {
                        Missing::Hide => "A line whose values are all missing is left out, with its icon. The lines below move up.",
                        Missing::Empty => "The line stays and the value is empty. The layout does not move.",
                        Missing::Text(_) => "Missing values are replaced by this text.",
                    }
                }
            }

            // Options that only matter for some texts
            div { class: "flex flex-wrap gap-1.5",
                ToggleButton { label: "Shrink text to fit the box", on: style.shrink, onclick: move |_| ds.edit_text(id, |s| s.shrink = !style.shrink) }
            }
            if has_icons {
                Field { label: "Icon color",
                    Select {
                        value: style.icon_tint.key().to_string(),
                        options: IconTint::ALL.iter().map(|tint| (tint.key(), tint.label())).collect(),
                        onchange: move |key: String| {
                            if let Some(tint) = IconTint::from_key(&key) {
                                ds.edit_text(id, |s| s.icon_tint = tint);
                            }
                        },
                    }
                }
            }
            if has_guests {
                Slider { label: "Space between guests", unit: " em", value: style.block_gap, min: 0.0, max: 3.0, step: 0.05,
                    onchange: move |v| ds.edit_text(id, |s| s.block_gap = v) }
            }
            DisclosureList {
                Disclosure { title: "Spacing",
                    Slider { label: "Line height", unit: "", value: style.line_height, min: 0.8, max: 2.5, step: 0.05,
                        onchange: move |v| ds.edit_text(id, |s| s.line_height = v) }
                    Slider { label: "Letter spacing", unit: "%", value: style.letter_spacing, min: -5.0, max: 30.0, step: 0.5,
                        onchange: move |v| ds.edit_text(id, |s| s.letter_spacing = v) }
                }
            }
            Hint { "Tip: **text** is bold. Lines with {{icon:phone}} get a small icon (also: person, mail, pin, warning)." }
        }
    }
}

// ─────────────────────────────────────────────
//  Shape and picture
// ─────────────────────────────────────────────

#[component]
fn ShapePanel(id: Uuid, style: ShapeStyle) -> Element {
    let ds = use_context::<Designer>();
    rsx! {
        div { class: "space-y-3",
            DesignColorField { label: "Fill", value: style.fill.clone(),
                onchange: move |v: String| ds.edit_element(id, |el| { if let ElementKind::Shape(s) = &mut el.kind { s.fill = v; } }) }
            DesignColorField { label: "Border color", value: style.border_color.clone(),
                onchange: move |v: String| ds.edit_element(id, |el| { if let ElementKind::Shape(s) = &mut el.kind { s.border_color = v; } }) }
            Slider { label: "Border width", unit: "%", value: style.border_width, min: 0.0, max: 5.0, step: 0.05,
                onchange: move |v| ds.edit_element(id, |el| { if let ElementKind::Shape(s) = &mut el.kind { s.border_width = v; } }) }
            Slider { label: "Rounded corners", unit: "%", value: style.radius, min: 0.0, max: 50.0, step: 0.5,
                onchange: move |v| ds.edit_element(id, |el| { if let ElementKind::Shape(s) = &mut el.kind { s.radius = v; } }) }
        }
    }
}

#[component]
fn PicturePanel(id: Uuid, style: ImageStyle, pictures: Vec<(Uuid, String)>) -> Element {
    let ds = use_context::<Designer>();
    rsx! {
        div { class: "space-y-3",
            Field { label: "Picture",
                ChoiceSelect {
                    value: style.image.map(|picture| picture.to_string()).unwrap_or_default(),
                    options: std::iter::once((String::new(), "— none —".to_string()))
                        .chain(pictures.into_iter().map(|(picture, name)| (picture.to_string(), name)))
                        .collect::<Vec<_>>(),
                    onchange: move |value: String| {
                        let picked = value.parse::<Uuid>().ok();
                        ds.edit_element(id, |el| { if let ElementKind::Image(s) = &mut el.kind { s.image = picked; } });
                    },
                }
            }
            div { class: "flex flex-wrap gap-1.5",
                ToggleButton { label: "Fill (crop)", on: style.fit == Fit::Cover,
                    onclick: move |_| ds.edit_element(id, |el| { if let ElementKind::Image(s) = &mut el.kind { s.fit = Fit::Cover; } }) }
                ToggleButton { label: "Fit inside", on: style.fit == Fit::Contain,
                    onclick: move |_| ds.edit_element(id, |el| { if let ElementKind::Image(s) = &mut el.kind { s.fit = Fit::Contain; } }) }
            }
            Slider { label: "Rounded corners", unit: "%", value: style.radius, min: 0.0, max: 50.0, step: 0.5,
                onchange: move |v| ds.edit_element(id, |el| { if let ElementKind::Image(s) = &mut el.kind { s.radius = v; } }) }
        }
    }
}

// ─────────────────────────────────────────────
//  Insert bar (always visible) / sheet settings (nothing selected)
// ─────────────────────────────────────────────

/// Text boxes, shapes, pictures and graphics that can be put on the sheet.
///
/// A horizontal bar above the sheet that stays in place while an element is selected (the
/// inspector on the right replaces only the sheet settings). The graphics are grouped; the chips
/// choose the group and the pictures scroll sideways.
#[component]
pub(super) fn InsertPanel() -> Element {
    let ds = use_context::<Designer>();
    let toasts = ds.toasts;
    let upload_task = use_busy();
    let mut groups: Vec<&'static str> = Vec::new();
    for graphic in BUILTIN_GRAPHICS {
        if !groups.contains(&graphic.group) {
            groups.push(graphic.group);
        }
    }
    let mut group = use_signal(|| if groups.contains(&"Kitchen") { "Kitchen" } else { groups.first().copied().unwrap_or("") });
    let shown_group = group();

    rsx! {
        // Sticky next to the sheet on wide screens; on a phone the bar scrolls with the page.
        Card { class: "lg:sticky lg:top-[3.75rem] z-20",
            div { class: "flex flex-wrap items-center gap-2",
                Eyebrow { text: "Insert", class: "mr-1" }
                Button { label: "Aa  Text box", variant: ButtonVariant::Secondary, onclick: move |_| ds.add_text_box() }
                Button { label: "▭  Shape", variant: ButtonVariant::Secondary, onclick: move |_| ds.add_shape() }
                label {
                    r#for: "design-image-input",
                    class: button_class(ButtonVariant::Secondary),
                    if upload_task.is_busy() { "Adding…" } else { "▣  Picture" }
                }
                span { class: "hidden sm:block h-6 w-px bg-amber-200 mx-1" }
                for name in groups.iter().copied() {
                    ToggleButton { key: "{name}", label: name, on: name == shown_group, onclick: move |_| group.set(name) }
                }
            }
            FileInput {
                id: "design-image-input",
                accept: "image/png,image/jpeg,image/webp,image/gif",
                onpick: move |_| {
                    upload_task.run(async move {
                        match store::read_image("design-image-input", 1400).await {
                            Ok(Some(picked)) => ds.add_picture(picked),
                            Ok(None) => {}
                            Err(e) => toasts.error("The picture could not be added", e),
                        }
                    });
                },
            }
            div { class: "flex gap-2 overflow-x-auto pb-1",
                for graphic in BUILTIN_GRAPHICS.iter().filter(|graphic| graphic.group == shown_group) {
                    {
                        let key = graphic.key;
                        let src = image_src(&format!("builtin:{key}"));
                        let dark_tile = key.ends_with("_light");
                        rsx! {
                            TileButton {
                                key: "{key}",
                                title: graphic.label,
                                onclick: move |_| ds.add_graphic(key),
                                img {
                                    class: if dark_tile { "h-12 w-full rounded bg-zinc-600 object-contain p-1" } else { "h-12 w-full object-contain" },
                                    src: "{src}",
                                    alt: "{graphic.label}",
                                }
                                span { class: "w-full truncate text-center", "{graphic.label}" }
                            }
                        }
                    }
                }
            }
            Hint { "Click an item to put it on the sheet, then move and resize it there. Pictures are scaled down and stored in this browser." }
        }
    }
}

/// Format, background and the pictures of the design.
#[component]
pub(super) fn SheetPanel(design: ScheduleDesign) -> Element {
    let ds = use_context::<Designer>();
    let toasts = ds.toasts;
    let upload_task = use_busy();
    let format_index = FORMATS.iter().position(|(_, ratio)| (ratio - design.ratio).abs() < 0.01).unwrap_or(0);
    const KEYS: [&str; 5] = ["0", "1", "2", "3", "4"];
    let uploaded: Vec<(Uuid, String, String)> = design
        .uploaded_images()
        .map(|image| (image.id, image.name.clone(), image_src(&image.data_url)))
        .collect();
    let background_image = design.background_image;
    let full = uploaded.len() >= MAX_IMAGES;
    let upload_class = format!(
        "{} {}",
        button_class(ButtonVariant::Secondary),
        if full { "opacity-50 pointer-events-none" } else { "" }
    );

    rsx! {
        Card { title: "Sheet",
            Field { label: "Format",
                Select {
                    value: format_index.to_string(),
                    options: FORMATS.iter().enumerate().map(|(i, (label, _))| (KEYS[i], *label)).collect(),
                    onchange: move |value: String| {
                        if let Some((_, ratio)) = value.parse::<usize>().ok().and_then(|i| FORMATS.get(i)) {
                            let ratio = *ratio;
                            ds.edit(|d| d.ratio = ratio);
                        }
                    },
                }
            }
            DesignColorField { label: "Background color", value: design.background_color.clone(),
                onchange: move |v: String| ds.edit(|d| d.background_color = v) }

            div { class: "space-y-1.5",
                Eyebrow { text: "Background picture", class: "block" }
                div { class: "flex flex-wrap items-center gap-2",
                    label { r#for: "design-background-input", class: "{upload_class}",
                        if upload_task.is_busy() { "Adding…" } else { "Choose a picture" }
                    }
                    if background_image.is_some() {
                        Button { label: "Remove", variant: ButtonVariant::Secondary, onclick: move |_| ds.edit(|d| d.background_image = None) }
                    }
                }
                FileInput {
                    id: "design-background-input",
                    accept: "image/png,image/jpeg,image/webp,image/gif",
                    onpick: move |_| {
                        upload_task.run(async move {
                            match store::read_image("design-background-input", 1400).await {
                                Ok(Some(picked)) => {
                                    if let Some(id) = ds.add_image(picked) {
                                        ds.edit(|d| d.background_image = Some(id));
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => toasts.error("The picture could not be added", e),
                            }
                        });
                    },
                }
            }

            if !uploaded.is_empty() {
                div { class: "space-y-1.5",
                    Eyebrow { text: format!("Your pictures ({}/{MAX_IMAGES})", uploaded.len()), class: "block" }
                    div { class: "grid grid-cols-3 gap-2",
                        for (id, name, src) in uploaded {
                            div { key: "{id}", class: "rounded-lg border border-amber-100 bg-white p-1 space-y-1",
                                img { class: "h-14 w-full rounded bg-zinc-100 object-contain", src: "{src}", alt: "{name}" }
                                div { class: "flex items-center justify-between gap-1",
                                    TextButton { label: "Background", onclick: move |_| ds.edit(|d| d.background_image = Some(id)) }
                                    TextButton { label: "✕", onclick: move |_| ds.edit(|d| d.remove_image(id)) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
