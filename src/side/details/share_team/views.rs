//! Static states of the registration page.

use chrono::NaiveDateTime;
use dioxus::prelude::*;

use super::lang::Lang;
use crate::ui::*;

#[component]
pub fn LockSVG() -> Element {
    rsx! {
        svg {
            class: "w-7 h-7",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            rect { x: "3", y: "11", width: "18", height: "11", rx: "2", ry: "2" }
            path { d: "M7 11V7a5 5 0 0 1 10 0v4" }
        }
    }
}

#[component]
pub fn ClockSVG(#[props(default, into)] class: String) -> Element {
    let size = if class.is_empty() { "w-7 h-7" } else { class.as_str() };
    rsx! {
        svg {
            class: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            circle { cx: "12", cy: "12", r: "10" }
            polyline { points: "12 6 12 12 16 14" }
        }
    }
}

/// The share link is not active (no configuration saved).
#[component]
pub fn InactiveView(lang: Lang) -> Element {
    let t = lang.txt();
    rsx! {
        EmptyState {
            title: t.inactive_title,
            text: t.inactive_body,
            icon: rsx! { LockSVG {} },
        }
    }
}

#[component]
pub fn ClosedView(lang: Lang, deadline: NaiveDateTime) -> Element {
    let t = lang.txt();
    rsx! {
        EmptyState {
            title: t.deadline_passed_title,
            text: t.deadline_passed_body,
            icon: rsx! { ClockSVG {} },
            Pill { tone: PillTone::Amber,
                ClockSVG { class: "w-3.5 h-3.5" }
                "{lang.format_deadline(deadline)}"
            }
        }
    }
}

#[component]
pub fn SuccessView(lang: Lang, verify_email: bool, in_review: bool) -> Element {
    let t = lang.txt();
    rsx! {
        Card { title: t.success_title, tone: Tone::Success, class: "max-sm:rounded-none max-sm:border-x-0",
            div { class: "flex flex-col items-center gap-4 text-center py-6",
                div { class: "w-16 h-16 rounded-full bg-green-100 flex items-center justify-center",
                    svg {
                        class: "w-8 h-8 text-green-600",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2.5",
                        polyline { points: "20 6 9 17 4 12" }
                    }
                }
                p { class: "text-sm text-zinc-500 max-w-xs leading-relaxed", "{t.success_body}" }
                if verify_email {
                    p { class: "text-sm text-zinc-600 max-w-xs leading-relaxed", "{t.success_verify}" }
                }
                if in_review {
                    p { class: "text-sm text-zinc-600 max-w-xs leading-relaxed", "{t.success_review}" }
                }
            }
        }
    }
}
