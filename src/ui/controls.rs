//! Controls of the design tools: a slider, a color picker. They use the same colors, rounding and
//! focus ring as the form kit; only the size is smaller, because they sit in narrow side panels.

use dioxus::prelude::*;

use super::form::TextInput;

/// A range slider with its label and current value.
#[component]
pub fn Slider(
    #[props(into)] label: String,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    onchange: EventHandler<f32>,
    #[props(default, into)] unit: String,
) -> Element {
    rsx! {
        label { class: "block",
            div { class: "flex justify-between text-xs text-zinc-500",
                span { "{label}" }
                span { class: "font-mono", "{value:.1}{unit}" }
            }
            input {
                r#type: "range",
                class: "w-full accent-brand cursor-pointer",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                oninput: move |e| {
                    if let Ok(parsed) = e.value().parse::<f32>() {
                        onchange.call(parsed);
                    }
                },
            }
        }
    }
}

/// `<input type=color>` only understands `#rrggbb`; anything else becomes black.
pub fn hex6(color: &str) -> String {
    let hex = color.trim().strip_prefix('#').unwrap_or("");
    match hex.len() {
        6 if hex.chars().all(|c| c.is_ascii_hexdigit()) => format!("#{hex}"),
        3 if hex.chars().all(|c| c.is_ascii_hexdigit()) => {
            let doubled: String = hex.chars().flat_map(|c| [c, c]).collect();
            format!("#{doubled}")
        }
        _ => "#000000".to_string(),
    }
}

/// Just the color swatch (the browser's picker). `title` is its accessible name.
#[component]
pub fn ColorInput(#[props(into)] title: String, #[props(into)] value: String, onchange: EventHandler<String>) -> Element {
    rsx! {
        input {
            r#type: "color",
            title: "{title}",
            "aria-label": "{title}",
            class: "h-9 w-12 shrink-0 cursor-pointer rounded-lg border border-amber-200 bg-white p-0.5",
            value: "{hex6(&value)}",
            oninput: move |e| onchange.call(e.value()),
        }
    }
}

/// A color: swatch plus the text (`#rrggbb`) next to it. `valid` marks the text field as faulty.
#[component]
pub fn ColorField(
    #[props(into)] label: String,
    #[props(into)] value: String,
    #[props(default = true)] valid: bool,
    onchange: EventHandler<String>,
    /// Called with what is typed into the text field; the owner decides whether it is a color.
    ontext: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "space-y-1",
            span { class: "text-xs text-zinc-500", "{label}" }
            div { class: "flex items-center gap-2",
                ColorInput { title: label.clone(), value: value.clone(), onchange }
                TextInput {
                    value: value.clone(),
                    error: !valid,
                    oninput: move |e: Event<FormData>| ontext.call(e.value()),
                }
            }
        }
    }
}
