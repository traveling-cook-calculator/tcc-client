//! The rules and what the plan currently does to them, plus proposals for the whole plan.

use dioxus::prelude::*;

use super::board::TeamTag;
use super::describe::describe_violation;
use super::inspector::ProposalRow;
use super::state::Editor;
use crate::planning::{suggest, Rule};
use crate::ui::*;

#[derive(Clone, Copy, PartialEq)]
enum Mark {
    Ok,
    Warn,
    Bad,
}

#[component]
fn RuleMark(mark: Mark) -> Element {
    let (text, class) = match mark {
        Mark::Ok => ("✓", "bg-emerald-100 text-emerald-700"),
        Mark::Warn => ("!", "bg-amber-100 text-amber-700"),
        Mark::Bad => ("✕", "bg-red-100 text-red-600"),
    };
    rsx! {
        span { class: "mt-0.5 inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-xs font-bold {class}",
            "{text}"
        }
    }
}

#[component]
pub(super) fn RulesPanel() -> Element {
    let ed = use_context::<Editor>();
    let report = ed.report.read().clone();
    let hard_ok = report.hard_ok();
    let repeats: Vec<_> = report.repeats().into_iter().cloned().collect();
    let no_repeats = repeats.is_empty();
    let repeat_rows: Vec<(uuid::Uuid, uuid::Uuid, u32, String)> = repeats
        .iter()
        .map(|m| {
            let courses = m.courses.iter().map(|c| ed.course_name(*c)).collect::<Vec<_>>().join(", ");
            (m.a, m.b, m.count, courses)
        })
        .collect();
    let uneven: Vec<String> = report
        .courses
        .iter()
        .filter(|stat| stat.max_persons != stat.min_persons)
        .map(|stat| format!("{}: {}–{}", ed.course_name(stat.course), stat.min_persons, stat.max_persons))
        .collect();

    let uneven_text = uneven.join(" · ");
    let is_even = uneven.is_empty();

    rsx! {
        Card {
            title: "Rules",
            tone: if hard_ok { Tone::Neutral } else { Tone::Danger },
            div { class: "space-y-3",
                Eyebrow { text: "Must be kept", class: "block" }
                for rule in Rule::ALL {
                    {
                        let violations = report.violations_of(rule);
                        rsx! {
                            div { key: "{rule.title()}", class: "flex gap-2.5",
                                RuleMark { mark: if violations.is_empty() { Mark::Ok } else { Mark::Bad } }
                                div { class: "min-w-0 space-y-1",
                                    p { class: "text-sm font-medium text-zinc-800", "{rule.title()}" }
                                    for (index, violation) in violations.iter().enumerate() {
                                        div { key: "{index}", class: "flex flex-wrap items-center gap-1.5 text-xs text-red-700",
                                            span { "{describe_violation(violation, &ed)}" }
                                            if let Some(team) = violation.team() {
                                                TeamTag { team }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                Eyebrow { text: "Should be kept", class: "block pt-1" }
                div { class: "flex gap-2.5",
                    RuleMark { mark: if is_even { Mark::Ok } else { Mark::Warn } }
                    div { class: "min-w-0 space-y-0.5",
                        p { class: "text-sm font-medium text-zinc-800", "Persons are spread evenly over the hosts" }
                        if !is_even {
                            p { class: "text-xs text-amber-700", "Persons per table: {uneven_text}" }
                        }
                    }
                }
                div { class: "flex gap-2.5",
                    RuleMark { mark: if no_repeats { Mark::Ok } else { Mark::Warn } }
                    div { class: "min-w-0 space-y-1",
                        p { class: "text-sm font-medium text-zinc-800", "Teams do not meet more than once" }
                        for (a, b, count, courses) in repeat_rows {
                            div { key: "{a}-{b}", class: "flex flex-wrap items-center gap-1 text-xs text-amber-700",
                                TeamTag { team: a }
                                "×"
                                TeamTag { team: b }
                                span { "{count}× ({courses})" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Proposals for the whole plan. Searching is cheap for small plans (automatic) and on demand for
/// large ones.
#[component]
pub(super) fn Proposals() -> Element {
    let ed = use_context::<Editor>();
    let mut shown = use_signal(|| ed.ctx.peek().teams.len() <= 16);
    let proposals = use_memo(move || {
        if shown() { suggest(&ed.draft.read(), &ed.ctx.read(), None, 6) } else { Vec::new() }
    });
    let perfect = ed.report.read().score == Default::default();

    rsx! {
        Card { title: "Proposals for the whole plan",
            if !shown() {
                Button {
                    label: "Search proposals",
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| shown.set(true),
                }
            } else if proposals.read().is_empty() {
                p { class: "text-sm text-zinc-500",
                    if perfect {
                        "The plan keeps all rules and nobody meets twice."
                    } else {
                        "No single change improves the plan. Try swapping two teams by hand, or use \"Improve automatically\"."
                    }
                }
            }
            for (index, suggestion) in proposals.read().iter().cloned().enumerate() {
                ProposalRow { key: "{index}", suggestion }
            }
        }
    }
}
