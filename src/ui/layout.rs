//! Page-level layout: `PageShell`, `PageHeader`, `Card`, `AddCardButton`, `Dialog`.

use super::button::CloseButton;
use super::icons::CheckSVG;
use dioxus::prelude::*;

/// Semantic color of a card. `Success` is used for the "just saved" flash.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Tone {
    #[default]
    Neutral,
    Success,
    Danger,
}

struct ToneClasses {
    border: &'static str,
    header: &'static str,
    accent: &'static str,
    title: &'static str,
}

impl Tone {
    fn classes(self, eyebrow: bool) -> ToneClasses {
        match self {
            Self::Neutral => ToneClasses {
                border: "border-amber-100",
                header: "bg-amber-50/70 border-amber-100",
                accent: "bg-amber-400/70",
                title: if eyebrow { "text-amber-700/70" } else { "text-zinc-800" },
            },
            Self::Success => ToneClasses {
                border: "border-green-200",
                header: "bg-green-50/70 border-green-200",
                accent: "bg-green-500/70",
                title: "text-green-800",
            },
            Self::Danger => ToneClasses {
                border: "border-red-200",
                header: "bg-red-50/70 border-red-200",
                accent: "bg-red-400/70",
                title: "text-red-800",
            },
        }
    }
}

/// The one card used by every page.
///
/// * `title: Some(..)` renders the header strip; `None` renders body only.
/// * `eyebrow` switches the title to the small uppercase list-item style.
/// * `flash` shows the green "saved" state (border, header, glow, check icon).
/// * `actions` is placed on the right of the header, `icon` sits in front of the title.
#[component]
pub fn Card(
    #[props(default, into)] title: Option<String>,
    #[props(default)] tone: Tone,
    #[props(default)] eyebrow: bool,
    #[props(default)] flash: bool,
    #[props(default)] actions: Option<Element>,
    #[props(default)] icon: Option<Element>,
    #[props(default = true)] padded: bool,
    #[props(default, into)] class: String,
    children: Element,
) -> Element {
    let tone = if flash { Tone::Success } else { tone };
    let c = tone.classes(eyebrow);
    let glow = if flash { "save-glow-card" } else { "" };
    let title_class = if eyebrow {
        "text-[11px] font-semibold tracking-[0.12em] uppercase"
    } else {
        "text-sm font-semibold"
    };
    let accent_h = if eyebrow { "h-4" } else { "h-5" };

    rsx! {
        section {
            class: "bg-white rounded-2xl border shadow-sm overflow-hidden \
                    transition-colors duration-300 {c.border} {glow} {class}",
            if let Some(title) = title {
                header { class: "px-5 py-3.5 border-b flex items-center justify-between gap-3 {c.header}",
                    div { class: "flex items-center gap-2.5 min-w-0",
                        div { class: "w-1.5 {accent_h} rounded-full shrink-0 {c.accent}" }
                        if let Some(icon) = icon {
                            {icon}
                        }
                        h3 { class: "truncate {title_class} {c.title}", "{title}" }
                        if flash {
                            CheckSVG {}
                        }
                    }
                    if let Some(actions) = actions {
                        div { class: "flex items-center gap-2 shrink-0", {actions} }
                    }
                }
            }
            if padded {
                div { class: "px-5 py-4 space-y-4", {children} }
            } else {
                {children}
            }
        }
    }
}

/// Outer wrapper of a routed page (replaces `section { class: "px-8 py-6 space-y-8" }`).
#[component]
pub fn PageShell(#[props(default, into)] class: String, children: Element) -> Element {
    rsx! {
        section { class: "flex-1 min-w-0 w-full px-4 sm:px-8 py-6 space-y-8 {class}", {children} }
    }
}

/// Centers content in the free space of the layout (home, 404, …). Never `h-screen`:
/// the layout already has a header and footer.
#[component]
pub fn Centered(#[props(default, into)] class: String, children: Element) -> Element {
    rsx! {
        div { class: "flex flex-1 w-full flex-col items-center justify-center px-6 py-12 {class}", {children} }
    }
}

/// Page title with optional subtitle and right-aligned actions.
#[component]
pub fn PageHeader(
    #[props(into)] title: String,
    #[props(default)] subtitle: Option<String>,
    #[props(default)] actions: Option<Element>,
) -> Element {
    rsx! {
        div { class: "flex items-start justify-between gap-4",
            div { class: "min-w-0",
                h1 { class: "text-3xl font-bold text-zinc-900 leading-tight", "{title}" }
                if let Some(subtitle) = subtitle {
                    p { class: "mt-1 text-sm text-zinc-500", "{subtitle}" }
                }
            }
            if let Some(actions) = actions {
                div { class: "flex items-center gap-2 shrink-0", {actions} }
            }
        }
    }
}

/// Dashed "add something" tile at the end of a list of cards.
#[component]
pub fn AddCardButton(#[props(into)] label: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "w-full flex items-center justify-center gap-2 py-4 rounded-2xl \
                    border-2 border-dashed border-amber-200 text-sm font-medium text-amber-700/80 \
                    hover:border-brand hover:text-brand hover:bg-amber-50/50 \
                    transition-colors duration-150 cursor-pointer",
            onclick: move |e| onclick.call(e),
            span { class: "text-lg leading-none", "+" }
            "{label}"
        }
    }
}

/// Modal dialog. The parent decides whether to render it at all
/// (`if show() { Dialog { … } }`), so there is no hidden open/close state.
#[component]
pub fn Dialog(
    #[props(into)] title: String,
    onclose: EventHandler<()>,
    /// Wide variant for two-column forms.
    #[props(default)] wide: bool,
    children: Element,
) -> Element {
    let width = if wide { "max-w-4xl" } else { "max-w-md" };
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center p-4 sm:p-6 \
                    bg-black/40 backdrop-blur-xs overflow-y-auto animate-fade-in",
            role: "dialog",
            "aria-modal": "true",
            div { class: "relative bg-white rounded-3xl border border-amber-100 shadow-2xl \
                          w-full {width} overflow-hidden my-auto",
                div { class: "px-6 py-4 bg-amber-50/80 border-b border-amber-100/80 flex items-center",
                    div { class: "flex items-center gap-2.5",
                        div { class: "w-1.5 h-5 rounded-full bg-amber-400" }
                        h3 { class: "text-base font-bold text-zinc-900", "{title}" }
                    }
                    CloseButton { onclick: move |_| onclose.call(()) }
                }
                div { class: "p-6 space-y-5", {children} }
            }
        }
    }
}
