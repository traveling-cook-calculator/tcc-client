//! Small reusable widgets: status pill, segmented tabs, search field,
//! empty state.

use super::busy::use_busy;
use super::button::Button;
use super::form::{Field, TextInput};
use super::toast::use_toasts;
use crate::web::copy_text;
use dioxus::prelude::*;

// ── Pill ─────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum PillTone {
    #[default]
    Amber,
    Neutral,
    Green,
}

impl PillTone {
    fn class(self) -> &'static str {
        match self {
            Self::Amber => "bg-amber-100/80 text-amber-800 border-amber-200/50",
            Self::Neutral => "bg-zinc-100 text-zinc-500 border-zinc-200/50",
            Self::Green => "bg-emerald-100/80 text-emerald-800 border-emerald-200/50",
        }
    }
}

#[component]
pub fn Pill(
    #[props(default)] tone: PillTone,
    #[props(default, into)] title: String,
    children: Element,
) -> Element {
    rsx! {
        span {
            title: "{title}",
            class: "inline-flex items-center justify-center gap-1 px-2.5 py-1 rounded-full \
                    text-xs font-medium shrink-0 border {tone.class()}",
            {children}
        }
    }
}

// ── Segmented tabs ───────────────────────────────

#[component]
pub fn SegmentedTabs(
    options: Vec<&'static str>,
    selected: usize,
    onselect: EventHandler<usize>,
    #[props(default, into)] label: String,
) -> Element {
    rsx! {
        div { class: "flex items-center gap-1.5 bg-amber-50/80 p-1.5 rounded-xl border border-amber-100/80 overflow-x-auto",
            if !label.is_empty() {
                span { class: "text-[11px] font-semibold tracking-wider text-amber-900/60 uppercase px-2 shrink-0 hidden md:inline",
                    "{label}"
                }
            }
            for (index, option) in options.into_iter().enumerate() {
                button {
                    key: "{option}",
                    r#type: "button",
                    class: if index == selected {
                        "px-3 py-1.5 rounded-lg text-xs font-semibold bg-white text-amber-900 shadow-xs border border-amber-200/60 transition-all cursor-pointer"
                    } else {
                        "px-3 py-1.5 rounded-lg text-xs font-medium text-zinc-600 hover:text-zinc-900 hover:bg-amber-100/50 transition-all cursor-pointer"
                    },
                    onclick: move |_| onselect.call(index),
                    "{option}"
                }
            }
        }
    }
}

// ── Search field ─────────────────────────────────

#[component]
pub fn SearchField(
    #[props(into)] value: String,
    oninput: EventHandler<Event<FormData>>,
    #[props(default, into)] placeholder: String,
) -> Element {
    rsx! {
        div { class: "relative w-full sm:max-w-xs md:max-w-sm",
            svg {
                class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-amber-600/60 pointer-events-none",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                view_box: "0 0 24 24",
                path { d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" }
            }
            TextInput {
                value,
                placeholder: placeholder.clone(),
                class: "pl-9",
                oninput: move |e| oninput.call(e),
            }
        }
    }
}

// ── Empty state ──────────────────────────────────

#[component]
pub fn EmptyState(
    #[props(into)] title: String,
    #[props(into)] text: String,
    #[props(default)] icon: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center text-center gap-5 py-16 sm:py-20 px-4 \
                      bg-white/50 rounded-3xl border border-dashed border-amber-200/80 max-w-lg mx-auto w-full",
            if let Some(icon) = icon {
                div { class: "w-16 h-16 rounded-2xl bg-amber-100/80 border border-amber-200 flex items-center justify-center shadow-xs text-amber-600",
                    {icon}
                }
            }
            div { class: "space-y-1.5 max-w-xs",
                h2 { class: "text-lg font-bold text-zinc-900", "{title}" }
                p { class: "text-sm text-zinc-500 leading-relaxed", "{text}" }
            }
            {children}
        }
    }
}

// ── Tabs (underline style) ───────────────────────

/// Tab bar with optional controls on the right. `disabled` lists tab indices.
#[component]
pub fn Tabs(
    tabs: Vec<&'static str>,
    selected: usize,
    onselect: EventHandler<usize>,
    #[props(default)] disabled: Vec<usize>,
    #[props(default)] actions: Option<Element>,
) -> Element {
    rsx! {
        div { class: "flex items-center justify-between gap-3 border-b border-amber-100 mb-5",
            div { class: "flex",
                for (index, tab) in tabs.into_iter().enumerate() {
                    button {
                        key: "{tab}",
                        r#type: "button",
                        role: "tab",
                        "aria-selected": index == selected,
                        disabled: disabled.contains(&index),
                        class: if disabled.contains(&index) {
                            "px-4 py-2 text-sm font-medium text-zinc-300 cursor-not-allowed"
                        } else if index == selected {
                            "px-4 py-2 text-sm font-semibold text-brand-hover border-b-2 border-brand-hover -mb-px cursor-pointer"
                        } else {
                            "px-4 py-2 text-sm font-medium text-zinc-500 hover:text-brand-hover transition-colors cursor-pointer"
                        },
                        onclick: move |_| onselect.call(index),
                        "{tab}"
                    }
                }
            }
            if let Some(actions) = actions {
                div { class: "flex items-center gap-3 pb-1", {actions} }
            }
        }
    }
}

// ── Toolbar ──────────────────────────────────────

/// Search / sort bar above a list (dashboard, teams).
#[component]
pub fn Toolbar(children: Element) -> Element {
    rsx! {
        div { class: "bg-white/80 backdrop-blur-xs p-3 sm:p-4 rounded-2xl border border-amber-100 shadow-xs \
                      flex flex-col sm:flex-row sm:items-center justify-between gap-3 sm:gap-4",
            {children}
        }
    }
}

// ── Skeleton ─────────────────────────────────────

/// Pulsing placeholder while data loads. Size and shape come from `class`
/// (e.g. `"h-4 w-28"`, `"h-9 w-full"`).
#[component]
pub fn Skeleton(#[props(default, into)] class: String) -> Element {
    rsx! { div { class: "animate-pulse rounded-lg bg-zinc-200/70 {class}", "aria-hidden": "true" } }
}

// ── Copyable text ────────────────────────────────

/// Read-only text (a link) with a "Copy" button.
#[component]
pub fn CopyField(
    #[props(into)] label: String,
    #[props(into)] value: String,
    #[props(default)] hint: Option<String>,
) -> Element {
    let toasts = use_toasts();
    let copy_task = use_busy();
    let text = value.clone();

    rsx! {
        Field { label, hint,
            div { class: "flex gap-2",
                TextInput { value, class: "flex-1", readonly: true, oninput: move |_| {} }
                Button {
                    label: "Copy",
                    loading: copy_task.is_busy(),
                    onclick: move |_| {
                        let text = text.clone();
                        copy_task.run(async move {
                            match copy_text(&text).await {
                                Ok(()) => toasts.success("Copied", ""),
                                Err(e) => toasts.error("Copying failed", format!("{e} Select the text and copy it manually.")),
                            }
                        });
                    },
                }
            }
        }
    }
}
