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
    /// Called when Enter is pressed inside the field (e.g. to start a search).
    #[props(default)] onenter: Option<EventHandler<()>>,
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
            onkeydown: move |e| {
                // `is_composing`: Enter that only confirms an IME candidate must not submit.
                if let Some(onenter) = onenter
                    && e.key() == Key::Enter
                    && !e.is_composing()
                {
                    e.prevent_default();
                    onenter.call(());
                }
            },
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
    /// DOM id, for code that has to focus the field (an editor inserting text at the cursor).
    #[props(default, into)] id: String,
) -> Element {
    rsx! {
        textarea {
            id: if id.is_empty() { None } else { Some(id) },
            class: input_class(error, &class),
            placeholder: "{placeholder}",
            rows: "{rows}",
            disabled,
            value: "{value}",
            oninput: move |e| oninput.call(e),
        }
    }
}

/// The small uppercase label style: field labels, section captions, the names in a list of facts.
pub const EYEBROW: &str = "text-[11px] font-semibold tracking-[0.12em] uppercase text-amber-700/70";

/// A small uppercase caption (a section of a form, a fact's name). Field labels use the same style.
#[component]
pub fn Eyebrow(#[props(into)] text: String, #[props(default, into)] class: String) -> Element {
    rsx! { span { class: "{EYEBROW} {class}", "{text}" } }
}

/// A short explaining text below a control or a block.
#[component]
pub fn Hint(#[props(default, into)] class: String, children: Element) -> Element {
    rsx! { p { class: "text-xs text-zinc-500 leading-relaxed {class}", {children} } }
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
            label { class: "block {EYEBROW}", "{label}" }
            {children}
            FieldError { error }
            if !has_error {
                if let Some(hint) = hint {
                    Hint { class: "ml-1", "{hint}" }
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
/// A checkbox with its label. `boxed` puts it into the amber frame used for consents.
#[component]
pub fn Checkbox(
    #[props(into)] label: String,
    checked: bool,
    onchange: EventHandler<bool>,
    #[props(default)] disabled: bool,
    #[props(default)] boxed: bool,
) -> Element {
    if boxed {
        return rsx! {
            label { class: "flex items-start gap-2.5 cursor-pointer rounded-xl border border-amber-200 bg-amber-50/60 px-3 py-2.5",
                input {
                    r#type: "checkbox",
                    class: "accent-brand w-4 h-4 mt-0.5 shrink-0 cursor-pointer",
                    checked,
                    disabled,
                    onchange: move |e| onchange.call(e.checked()),
                }
                span { class: "text-xs text-zinc-700 leading-relaxed", "{label}" }
            }
        };
    }
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

/// A checkbox without visible text, for the cells of a table. `label` is what a screen reader says
/// ("Phone: required").
#[component]
pub fn CheckboxCell(
    #[props(into)] label: String,
    checked: bool,
    onchange: EventHandler<bool>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        input {
            r#type: "checkbox",
            class: "accent-brand w-4 h-4 cursor-pointer disabled:cursor-not-allowed disabled:opacity-50",
            "aria-label": "{label}",
            checked,
            disabled,
            onchange: move |e| onchange.call(e.checked()),
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
    #[props(default, into)] class: String,
) -> Element {
    let options = options.into_iter().map(|(value, label)| (value.to_string(), label.to_string())).collect();
    rsx! { ChoiceSelect { value, options, onchange, disabled, class } }
}

/// A `<select>` over choices that are only known at run time (pictures, teams, fonts).
/// `options` are `(value, label)` pairs; the entry whose value equals `value` is selected.
#[component]
pub fn ChoiceSelect(
    #[props(into)] value: String,
    options: Vec<(String, String)>,
    onchange: EventHandler<String>,
    #[props(default)] disabled: bool,
    #[props(default, into)] class: String,
) -> Element {
    rsx! {
        select {
            class: input_class(false, &format!("cursor-pointer {class}")),
            disabled,
            onchange: move |e| onchange.call(e.value()),
            for (option_value, label) in options {
                option {
                    key: "{option_value}",
                    value: "{option_value}",
                    selected: option_value == value,
                    "{label}"
                }
            }
        }
    }
}

/// The hidden file field that a `label { r#for: id }` styled as a button opens. `onpick` fires when
/// a file was chosen; the owner reads it by `id` (see `store::read_text` / `read_image`).
#[component]
pub fn FileInput(
    #[props(into)] id: String,
    #[props(into)] accept: String,
    onpick: EventHandler<Event<FormData>>,
) -> Element {
    rsx! {
        input {
            id: "{id}",
            r#type: "file",
            accept: "{accept}",
            class: "hidden",
            onchange: move |e| onpick.call(e),
        }
    }
}

/// One entry of a `PickSelect`.
#[derive(Clone, PartialEq, Debug)]
pub struct PickOption {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

impl PickOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self { value: value.into(), label: label.into(), disabled: false }
    }

    pub fn disabled_if(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// A select that does something when an entry is picked ("Move to ...", "Swap cooking with ..."):
/// it always shows its `placeholder` and hands the picked value to `onpick`. Smaller than `Select`,
/// for the dense side panels of the plan editor.
#[component]
pub fn PickSelect(
    #[props(into)] placeholder: String,
    options: Vec<PickOption>,
    onpick: EventHandler<String>,
    #[props(default, into)] class: String,
) -> Element {
    rsx! {
        select {
            class: "w-full rounded-lg border border-amber-200 bg-white px-2 py-1 text-xs text-zinc-600 cursor-pointer \
                    focus:outline-none focus:ring-2 focus:ring-amber-300 {class}",
            onchange: move |e| onpick.call(e.value()),
            option { value: "", selected: true, "{placeholder}" }
            for option_entry in options {
                option {
                    key: "{option_entry.value}",
                    value: "{option_entry.value}",
                    disabled: option_entry.disabled,
                    "{option_entry.label}"
                }
            }
        }
    }
}

/// An on/off switch (a setting that takes effect at once, like "analytics cookies"). Use
/// `Checkbox` for options that are part of a form that is sent later.
#[component]
pub fn Switch(
    #[props(into)] label: String,
    on: bool,
    onchange: EventHandler<bool>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            role: "switch",
            "aria-checked": "{on}",
            "aria-label": "{label}",
            title: "{label}",
            disabled,
            class: if on {
                "flex w-10 h-5 items-center rounded-full bg-amber-500 px-0.5 transition-colors duration-200 cursor-pointer disabled:cursor-not-allowed disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-brand-hover"
            } else {
                "flex w-10 h-5 items-center rounded-full bg-zinc-300 px-0.5 transition-colors duration-200 cursor-pointer disabled:cursor-not-allowed disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-brand-hover"
            },
            onclick: move |_| onchange.call(!on),
            span { class: if on { "w-4 h-4 rounded-full bg-white shadow-sm transition-all duration-200 ml-auto" } else { "w-4 h-4 rounded-full bg-white shadow-sm transition-all duration-200" } }
        }
    }
}
