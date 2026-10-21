//! Form building blocks. One `TextInput` / `TextArea` replaces the six
//! near-identical `Input*` components; `Field` adds label + error text.

use super::icons::ErrorSVG;
use dioxus::prelude::*;

const INPUT_BASE: &str = "w-full px-3 py-2 border rounded-xl text-sm \
    focus:outline-none focus:ring-2 transition-colors duration-150";
const INPUT_NORMAL: &str = "border-amber-200 bg-amber-50/40 text-zinc-800 placeholder-zinc-400 \
    focus:ring-amber-400/40 focus:border-amber-400";
const INPUT_ERROR: &str = "border-red-300 bg-red-50/40 text-red-700 placeholder-red-300 \
    focus:ring-red-400/40 focus:border-red-400";

fn input_class(error: bool, extra: &str) -> String {
    let tone = if error { INPUT_ERROR } else { INPUT_NORMAL };
    format!("{INPUT_BASE} {tone} {extra}")
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum InputKind {
    #[default]
    Text,
    Number,
    Tel,
    Time,
    Date,
    Email,
}

impl InputKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Tel => "tel",
            Self::Time => "time",
            Self::Date => "date",
            Self::Email => "email",
        }
    }
}

#[component]
pub fn TextInput(
    #[props(into)] value: String,
    oninput: EventHandler<Event<FormData>>,
    #[props(default)] kind: InputKind,
    #[props(default, into)] placeholder: String,
    #[props(default)] error: bool,
    #[props(default)] disabled: bool,
    #[props(default)] readonly: bool,
    #[props(default, into)] class: String,
) -> Element {
    rsx! {
        input {
            class: input_class(error, &class),
            r#type: kind.as_str(),
            placeholder: "{placeholder}",
            disabled,
            readonly,
            value: "{value}",
            oninput: move |e| oninput.call(e),
        }
    }
}

#[component]
pub fn TextArea(
    #[props(into)] value: String,
    oninput: EventHandler<Event<FormData>>,
    #[props(default = 3)] rows: u32,
    #[props(default, into)] placeholder: String,
    #[props(default)] error: bool,
    #[props(default)] disabled: bool,
    #[props(default, into)] class: String,
) -> Element {
    rsx! {
        textarea {
            class: input_class(error, &class),
            placeholder: "{placeholder}",
            rows: "{rows}",
            disabled,
            value: "{value}",
            oninput: move |e| oninput.call(e),
        }
    }
}

/// Inline validation message (renders nothing when `error` is empty).
#[component]
pub fn FieldError(#[props(into)] error: String, #[props(default, into)] class: String) -> Element {
    rsx! {
        if !error.is_empty() {
            div { class: "flex items-center gap-1.5 text-red-500 text-xs {class}",
                ErrorSVG {}
                span { "{error}" }
            }
        }
    }
}

/// Label + control + error/hint, with consistent spacing.
///
/// ```ignore
/// Field { label: "Project name", error: name_error(),
///     TextInput { value: name(), error: !name_error().is_empty(), oninput: … }
/// }
/// ```
#[component]
pub fn Field(
    #[props(into)] label: String,
    #[props(default, into)] error: String,
    #[props(default)] hint: Option<String>,
    #[props(default, into)] class: String,
    children: Element,
) -> Element {
    let has_error = !error.is_empty();
    rsx! {
        div { class: "flex flex-col gap-1.5 {class}",
            label { class: "block text-[11px] font-semibold tracking-[0.12em] uppercase text-amber-700/70", "{label}" }
            {children}
            FieldError { error }
            if !has_error {
                if let Some(hint) = hint {
                    p { class: "text-xs text-zinc-400 ml-1", "{hint}" }
                }
            }
        }
    }
}

/// Tiny status indicator for auto-save rows: spinner while saving, icon on error.
#[component]
pub fn SavingIcon(saving: bool, #[props(default, into)] error: String) -> Element {
    rsx! {
        if !error.is_empty() {
            div { title: "{error}", ErrorSVG {} }
        } else if saving {
            div {
                class: "w-5 h-5 border-2 border-amber-200 border-t-brand rounded-full animate-spin",
                title: "Saving…",
            }
        }
    }
}

/// Labelled checkbox / toggle.
#[component]
pub fn Checkbox(
    #[props(into)] label: String,
    checked: bool,
    onchange: EventHandler<bool>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        label { class: "flex items-center gap-2.5 cursor-pointer group w-fit",
            input {
                r#type: "checkbox",
                class: "accent-brand w-4 h-4 rounded cursor-pointer",
                checked,
                disabled,
                onchange: move |e| onchange.call(e.checked()),
            }
            span { class: "text-[13px] text-zinc-600 group-hover:text-zinc-800 transition-colors",
                "{label}"
            }
        }
    }
}

/// Native `<select>`. `options` are `(value, label)` pairs.
#[component]
pub fn Select(
    #[props(into)] value: String,
    options: Vec<(&'static str, &'static str)>,
    onchange: EventHandler<String>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        select {
            class: input_class(false, "cursor-pointer"),
            disabled,
            onchange: move |e| onchange.call(e.value()),
            for (option_value, label) in options {
                option {
                    key: "{option_value}",
                    value: option_value,
                    selected: option_value == value,
                    "{label}"
                }
            }
        }
    }
}
