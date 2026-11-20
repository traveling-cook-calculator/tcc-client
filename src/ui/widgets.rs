//! Small reusable widgets: status pill, segmented tabs, search field,
//! empty state.

use super::busy::use_busy;
use super::button::{button_class, Button, ButtonVariant, HeaderButton};
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
    Red,
}

impl PillTone {
    fn class(self) -> &'static str {
        match self {
            Self::Amber => "bg-amber-100/80 text-amber-800 border-amber-200/50",
            Self::Neutral => "bg-zinc-100 text-zinc-500 border-zinc-200/50",
            Self::Green => "bg-emerald-100/80 text-emerald-800 border-emerald-200/50",
            Self::Red => "bg-red-100/80 text-red-800 border-red-200/60",
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
                span { class: "{super::form::EYEBROW} px-2 shrink-0 hidden md:inline",
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

// ── InfoTip ──────────────────────────────────────

/// The little "i" that explains something on hover or keyboard focus. The text is also the
/// accessible name, so it works without a mouse.
#[component]
pub fn InfoTip(#[props(into)] text: String) -> Element {
    rsx! {
        span { class: "relative group inline-flex cursor-help", tabindex: "0", role: "note", "aria-label": "{text}",
            super::icons::InfoSVG {}
            span { role: "tooltip",
                class: "absolute bottom-full right-0 mb-1.5 hidden group-hover:block group-focus:block \
                        bg-zinc-800 text-white text-xs rounded-xl px-3 py-2 w-64 z-10 shadow-lg leading-relaxed",
                "{text}"
            }
        }
    }
}

// ── Disclosure ───────────────────────────────────

/// A list of folded rows (a "settings list"): each row is one line until it is opened. Keeps rarely
/// needed things out of the way on small screens. Native `<details>`, so it works with keyboard
/// and screen readers without code.
#[component]
pub fn DisclosureList(children: Element) -> Element {
    rsx! {
        div { class: "bg-white rounded-2xl border border-amber-100 shadow-sm overflow-hidden divide-y divide-amber-100 \
                      max-sm:rounded-none max-sm:border-x-0",
            {children}
        }
    }
}

#[component]
pub fn Disclosure(
    #[props(into)] title: String,
    /// Destructive actions (cancel, delete) get a red title.
    #[props(default)] danger: bool,
    #[props(default)] open: bool,
    children: Element,
) -> Element {
    let title_class = if danger { "text-red-700" } else { "text-zinc-800" };
    rsx! {
        details { class: "group", open,
            summary { class: "flex items-center justify-between gap-3 px-4 min-h-12 py-3 cursor-pointer select-none list-none \
                              [&::-webkit-details-marker]:hidden hover:bg-amber-50/50 focus-visible:outline-2 \
                              focus-visible:outline-brand-hover",
                span { class: "text-sm font-semibold {title_class}", "{title}" }
                svg {
                    class: "w-4 h-4 shrink-0 text-zinc-400 transition-transform group-open:rotate-180",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2.5",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    "aria-hidden": "true",
                    polyline { points: "6 9 12 15 18 9" }
                }
            }
            div { class: "px-4 pb-4 space-y-3", {children} }
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

// ── ActionMenu ───────────────────────────────────

/// Classes of one entry of an `ActionMenu` (also for `Link { class: menu_item_class(false) }`).
pub fn menu_item_class(danger: bool) -> &'static str {
    if danger {
        "flex w-full items-center rounded-lg px-3 py-2 text-left text-sm text-red-700 hover:bg-red-50 cursor-pointer"
    } else {
        "flex w-full items-center rounded-lg px-3 py-2 text-left text-sm text-zinc-700 hover:bg-amber-50 cursor-pointer"
    }
}

/// A button that opens a small menu: for the actions that are rarely needed, so that they do not
/// all compete for attention as buttons of their own. The menu closes after a click on an entry,
/// a click outside of it, or Escape.
#[component]
pub fn ActionMenu(
    #[props(into)] label: String,
    /// The app header's quiet look instead of a secondary button.
    #[props(default)] header: bool,
    /// Own content of the button (an avatar and a name) instead of `label`, which is then only its
    /// accessible name.
    #[props(default)] trigger: Option<Element>,
    children: Element,
) -> Element {
    let mut open = use_signal(|| false);
    let button_content = rsx! {
        if let Some(trigger) = trigger.clone() {
            {trigger}
        } else {
            "{label}"
        }
        span { class: "text-[10px]", "aria-hidden": "true", "▾" }
    };
    rsx! {
        div {
            class: "relative",
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    open.set(false);
                }
            },
            if header {
                HeaderButton {
                    label: label.clone(),
                    expanded: open(),
                    onclick: move |_| {
                        let is_open = open();
                        open.set(!is_open);
                    },
                    {button_content}
                }
            } else {
                button {
                    r#type: "button",
                    class: button_class(ButtonVariant::Secondary),
                    "aria-haspopup": "menu",
                    "aria-expanded": open(),
                    onclick: move |_| {
                        let is_open = open();
                        open.set(!is_open);
                    },
                    {button_content}
                }
            }
            if open() {
                // Invisible layer: a click anywhere else closes the menu.
                div { class: "fixed inset-0 z-30", onclick: move |_| open.set(false) }
                div {
                    role: "menu",
                    class: "absolute right-0 z-40 mt-2 w-64 rounded-xl border border-amber-100 bg-white p-1.5 shadow-lg",
                    onclick: move |_| open.set(false),
                    {children}
                }
            }
        }
    }
}

/// One entry of an `ActionMenu` that runs an action.
#[component]
pub fn MenuItem(
    #[props(into)] label: String,
    #[props(default)] danger: bool,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if disabled {
        "flex w-full items-center rounded-lg px-3 py-2 text-left text-sm text-zinc-300 cursor-not-allowed"
    } else {
        menu_item_class(danger)
    };
    rsx! {
        button {
            r#type: "button",
            role: "menuitem",
            class,
            disabled,
            onclick: move |e| {
                if !disabled {
                    onclick.call(e);
                }
            },
            "{label}"
        }
    }
}

/// Thin line between groups of entries of an `ActionMenu`.
#[component]
pub fn MenuDivider() -> Element {
    rsx! { div { class: "my-1 h-px bg-amber-100", role: "separator" } }
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
    /// Texts of the button and its toasts (the guest pages are German/English).
    #[props(default = "Copy".to_string(), into)] button_label: String,
    #[props(default = "Copied".to_string(), into)] copied_label: String,
    #[props(default = "Copying failed".to_string(), into)] failed_label: String,
) -> Element {
    let toasts = use_toasts();
    let copy_task = use_busy();
    let text = value.clone();

    rsx! {
        Field { label, hint,
            div { class: "flex gap-2",
                TextInput { value, class: "flex-1", readonly: true, oninput: move |_| {} }
                Button {
                    label: button_label,
                    loading: copy_task.is_busy(),
                    onclick: move |_| {
                        let text = text.clone();
                        let (copied_label, failed_label) = (copied_label.clone(), failed_label.clone());
                        copy_task.run(async move {
                            match copy_text(&text).await {
                                Ok(()) => toasts.success(copied_label, ""),
                                Err(e) => toasts.error(failed_label, e.to_string()),
                            }
                        });
                    },
                }
            }
        }
    }
}
