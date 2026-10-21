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
