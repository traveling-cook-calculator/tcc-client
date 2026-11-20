//! Static states of the registration page.

use chrono::NaiveDateTime;
use dioxus::prelude::*;

use super::lang::{with_days, Lang};
use crate::config::Config;
use crate::Route;
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

/// Registration is over because `max_teams` is reached.
#[component]
pub fn FullView(lang: Lang) -> Element {
    let t = lang.txt();
    rsx! {
        EmptyState {
            title: t.full_title,
            text: t.full_body,
            icon: rsx! { LockSVG {} },
        }
    }
}

/// After the registration: what happens next, and the personal link of the team. The link is
/// shown in every case; without an email address it is the only way back to the team.
#[component]
pub fn SuccessView(lang: Lang, verify_email: bool, in_review: bool, has_mail: bool, team_link: String) -> Element {
    let t = lang.txt();
    rsx! {
        Card { title: t.success_title, tone: Tone::Success, class: "max-sm:rounded-none max-sm:border-x-0",
            div { class: "flex flex-col items-center gap-3 text-center py-4",
                div { class: "w-14 h-14 rounded-full bg-green-100 flex items-center justify-center",
                    svg {
                        class: "w-7 h-7 text-green-600",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2.5",
                        "aria-hidden": "true",
                        polyline { points: "20 6 9 17 4 12" }
                    }
                }
                p { class: "text-sm text-zinc-600 max-w-sm leading-relaxed", "{t.success_body}" }
                if verify_email {
                    p { class: "text-sm text-zinc-700 max-w-sm leading-relaxed font-medium", "{t.success_verify}" }
                }
                if in_review {
                    p { class: "text-sm text-zinc-600 max-w-sm leading-relaxed", "{t.success_review}" }
                }
            }

            Notice { tone: NoticeTone::Attention, roomy: true, class: "space-y-3", title: t.success_link_title,
                if has_mail {
                    p { "{t.success_link_mail}" }
                } else {
                    p { class: "font-semibold text-red-700", "{t.success_link_nomail}" }
                }
                CopyField {
                    label: t.lbl_your_link,
                    value: team_link.clone(),
                    button_label: t.btn_copy,
                    copied_label: t.copied,
                    failed_label: t.copy_failed_title,
                }
                p { class: "text-xs text-zinc-600 leading-relaxed", "{t.link_secret_note}" }
                a { href: "{team_link}", class: button_class(ButtonVariant::Primary), "{t.btn_open_team}" }
            }
        }
    }
}

/// Information duty at the point of collection (Art. 13 GDPR): purpose, recipients, deletion,
/// rights. The deletion period comes from the configuration (the API alone decides about it; a
/// missing value just leaves the sentence out).
#[component]
pub fn PrivacyNotice(lang: Lang) -> Element {
    let t = lang.txt();
    let config = use_context::<Signal<Option<Config>>>();
    let days = config.read().as_ref().and_then(|config| config.legal.retention_participant_data_days);

    rsx! {
        Notice { tone: NoticeTone::Attention, small: true, roomy: true, class: "space-y-1.5", title: t.privacy_title,
            p { class: "text-xs text-zinc-700 leading-relaxed", "{t.privacy_intro}" }
            if let Some(days) = days {
                p { class: "text-xs text-zinc-700 leading-relaxed", "{with_days(t.privacy_retention_participants, days)}" }
            }
            p { class: "text-xs text-zinc-700 leading-relaxed", "{t.privacy_rights}" }
            p { class: "text-xs text-zinc-700 leading-relaxed", "{t.privacy_minors}" }
            Link { to: Route::Privacy {}, class: "text-xs font-medium text-amber-700 underline", "{t.privacy_link}" }
        }
    }
}
