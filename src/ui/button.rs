//! Buttons. All props are plain comparable values, so the derived
//! `PartialEq` is correct (no "ignore the closure" shortcuts).

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ButtonVariant {
    /// Main call to action (brand color).
    #[default]
    Primary,
    /// Neutral secondary action.
    Secondary,
    /// Destructive, filled.
    Danger,
    /// Destructive, outlined (lower emphasis, e.g. "Delete").
    DangerOutline,
}

const BTN_BASE: &str = "inline-flex items-center justify-center gap-2 h-9 px-4 text-sm \
    font-medium rounded-xl transition-colors duration-150 cursor-pointer \
    focus:outline-none focus-visible:ring-2 focus-visible:ring-amber-400/60";

const BTN_DISABLED: &str = "bg-amber-100 text-amber-400 cursor-not-allowed";

impl ButtonVariant {
    fn class(self) -> &'static str {
        match self {
            Self::Primary => "bg-brand text-white hover:bg-brand-hover",
            Self::Secondary => "bg-zinc-100 text-zinc-700 hover:bg-zinc-200",
            Self::Danger => "bg-red-500 text-white hover:bg-red-600",
            Self::DangerOutline => "border border-red-400 text-red-600 hover:bg-red-50",
        }
    }
}

/// Classes of a button, for `Link { class: button_class(..) }` (navigation that looks like a button).
pub fn button_class(variant: ButtonVariant) -> String {
    format!("{BTN_BASE} {}", variant.class())
}

/// Small spinner that inherits the text color of its parent.
/// `class` replaces the default size (`w-4 h-4`), e.g. `"w-3.5 h-3.5 text-amber-400"`.
#[component]
pub fn Spinner(#[props(default, into)] class: String) -> Element {
    let size = if class.is_empty() { "w-4 h-4" } else { class.as_str() };
    rsx! {
        span {
            role: "status",
            class: "inline-block rounded-full border-2 border-current \
                    border-t-transparent animate-spin {size}",
        }
    }
}

#[component]
pub fn Button(
    #[props(into)] label: String,
    #[props(default)] variant: ButtonVariant,
    #[props(default)] loading: bool,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let blocked = disabled || loading;
    let tone = if disabled && !loading { BTN_DISABLED } else { variant.class() };
    let cursor = if loading { "cursor-wait opacity-80" } else { "" };

    rsx! {
        button {
            r#type: "button",
            class: "{BTN_BASE} {tone} {cursor}",
            disabled: blocked,
            "aria-busy": loading,
            onclick: move |e| {
                if !blocked {
                    onclick.call(e);
                }
            },
            if loading {
                Spinner {}
            }
            "{label}"
        }
    }
}

/// Icon-only trash button for list rows.
#[component]
pub fn DeleteButton(
    #[props(default)] loading: bool,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let blocked = disabled || loading;
    let tone = if disabled && !loading { BTN_DISABLED } else { ButtonVariant::DangerOutline.class() };
    let cursor = if loading { "cursor-wait opacity-80" } else { "" };

    rsx! {
        button {
            r#type: "button",
            class: "{BTN_BASE} {tone} {cursor}",
            title: "Delete",
            "aria-label": "Delete",
            "aria-busy": loading,
            disabled: blocked,
            onclick: move |e| {
                if !blocked {
                    onclick.call(e);
                }
            },
            if loading {
                Spinner {}
            } else {
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke_width: "2",
                    stroke: "currentColor",
                    class: "w-5 h-5",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6M9 7V4a1 1 0 011-1h4a1 1 0 011 1v3m4 0H5",
                    }
                }
            }
        }
    }
}

/// "×" in the top-right corner of a card or dialog.
#[component]
pub fn CloseButton(onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            "aria-label": "Close",
            class: "absolute top-3 right-3 p-1 rounded-lg text-zinc-400 \
                    hover:text-zinc-600 hover:bg-zinc-100 \
                    transition-colors duration-150 cursor-pointer",
            onclick: move |e| onclick.call(e),
            svg {
                class: "w-5 h-5",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                xmlns: "http://www.w3.org/2000/svg",
                view_box: "0 0 24 24",
                path { d: "M6 18L18 6M6 6l12 12" }
            }
        }
    }
}

/// A round button that floats in the corner of the screen (the one main action of a page, e.g.
/// the image download of a plan). `label` is what a screen reader says; the icon is the content.
#[component]
pub fn FloatingButton(
    #[props(into)] label: String,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            "aria-label": "{label}",
            title: "{label}",
            class: "fixed bottom-6 right-6 z-40 flex items-center justify-center w-14 h-14 rounded-full \
                    bg-brand-hover text-white shadow-lg hover:shadow-xl hover:scale-110 \
                    transition-all duration-150 cursor-pointer \
                    focus:outline-none focus-visible:ring-2 focus-visible:ring-amber-400/60",
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
}

/// A button that is either on or off (a style switch, a choice among a few): `aria-pressed` tells
/// assistive technology which. The content is the `label` or the children (icon, badge).
/// `large` is the size for choices that stand alone (the designs of a project); the default is
/// for the dense panels of an editor.
#[component]
pub fn ToggleButton(
    #[props(default, into)] label: String,
    on: bool,
    onclick: EventHandler<()>,
    #[props(default, into)] title: String,
    #[props(default)] large: bool,
    children: Element,
) -> Element {
    let size = if large { "px-3 py-1.5 text-sm" } else { "px-2.5 py-1 text-xs" };
    rsx! {
        button {
            r#type: "button",
            title: "{title}",
            "aria-pressed": "{on}",
            class: if on {
                "rounded-lg border border-amber-300 bg-amber-100 {size} font-semibold text-amber-900 cursor-pointer"
            } else {
                "rounded-lg border border-amber-100 bg-white {size} text-zinc-600 hover:bg-amber-50 cursor-pointer"
            },
            onclick: move |_| onclick.call(()),
            "{label}"
            {children}
        }
    }
}

/// A square icon button of a toolbar (bold, alignment ...); on or off like `ToggleButton`.
#[component]
pub fn ToolButton(#[props(into)] title: String, on: bool, onclick: EventHandler<()>, children: Element) -> Element {
    rsx! {
        button {
            r#type: "button",
            title: "{title}",
            "aria-label": "{title}",
            "aria-pressed": "{on}",
            class: if on {
                "flex h-8 min-w-8 items-center justify-center rounded-lg border border-amber-300 bg-amber-100 px-1.5 text-sm font-semibold text-amber-900 cursor-pointer"
            } else {
                "flex h-8 min-w-8 items-center justify-center rounded-lg border border-transparent px-1.5 text-sm text-zinc-600 hover:bg-amber-50 cursor-pointer"
            },
            onclick: move |_| onclick.call(()),
            {children}
        }
    }
}

/// A button that looks like an underlined text ("Select all"): for small secondary actions
/// within a line of text.
#[component]
pub fn TextButton(#[props(into)] label: String, onclick: EventHandler<MouseEvent>, #[props(default)] disabled: bool) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "text-xs text-zinc-600 underline hover:text-zinc-900 cursor-pointer disabled:cursor-not-allowed disabled:opacity-50",
            disabled,
            onclick: move |e| onclick.call(e),
            "{label}"
        }
    }
}

/// A small outlined button inside a list of choices (a text block to insert, a team to pick).
#[component]
pub fn ChipButton(#[props(into)] label: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "rounded-lg border border-amber-100 px-2.5 py-1.5 text-left text-xs text-zinc-700 \
                    hover:border-amber-300 hover:bg-amber-50 cursor-pointer \
                    focus:outline-none focus-visible:ring-2 focus-visible:ring-amber-400/60",
            onclick: move |e| onclick.call(e),
            "{label}"
        }
    }
}

/// A tile with a picture and a caption in a strip of choices (a graphic to put on a sheet).
#[component]
pub fn TileButton(#[props(into)] title: String, onclick: EventHandler<MouseEvent>, children: Element) -> Element {
    rsx! {
        button {
            r#type: "button",
            title: "{title}",
            class: "flex w-20 shrink-0 flex-col items-center gap-1 rounded-lg border border-amber-100 p-1.5 text-[10px] \
                    text-zinc-500 hover:border-amber-300 hover:bg-amber-50 cursor-pointer",
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
}

/// The button of the app's header (login, logout, account menu): quiet, amber, rounded.
#[component]
pub fn HeaderButton(
    onclick: EventHandler<MouseEvent>,
    #[props(default)] disabled: bool,
    #[props(default, into)] label: String,
    #[props(default, into)] expanded: Option<bool>,
    children: Element,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex items-center gap-2 px-3 py-1.5 rounded-xl text-sm font-medium text-zinc-600 bg-amber-50 \
                    border border-amber-200 hover:bg-amber-100 hover:border-amber-300 transition-colors duration-150 \
                    cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed",
            "aria-label": if label.is_empty() { None } else { Some(label.clone()) },
            "aria-haspopup": if expanded.is_some() { Some("menu") } else { None },
            "aria-expanded": expanded.map(|open| open.to_string()),
            disabled,
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
}

/// A whole card that is one button (a list entry that opens a dialog). `class` adds the border and
/// the colors of its state.
#[component]
pub fn CardButton(onclick: EventHandler<MouseEvent>, #[props(default, into)] class: String, children: Element) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "w-full text-left relative bg-white rounded-2xl border shadow-sm hover:shadow-md transition-all duration-150 \
                    cursor-pointer overflow-hidden focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400 {class}",
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
}

/// A small tag that is colored by data (the color of a team). `style` carries the colors.
#[component]
pub fn TagButton(#[props(into)] style: String, onclick: EventHandler<MouseEvent>, children: Element) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "inline-flex items-center gap-1 rounded-lg border px-2 py-0.5 text-xs font-semibold cursor-pointer hover:shadow-sm",
            style: "{style}",
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
}
