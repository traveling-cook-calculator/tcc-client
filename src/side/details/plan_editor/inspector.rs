//! Details of the selected team: its route, who it meets, and proposals that involve it.

use dioxus::prelude::*;

use super::board::TeamTag;
use super::describe::{describe_effect, describe_op};
use super::matrix::MatrixCell;
use super::state::Editor;
use crate::planning::suggest;
use crate::ui::*;

#[component]
pub(super) fn Inspector() -> Element {
    let ed = use_context::<Editor>();
    let proposals = use_memo(move || {
        match *ed.selected.read() {
            Some(team) => suggest(&ed.draft.read(), &ed.ctx.read(), Some(team), 5),
            None => Vec::new(),
        }
    });

    let Some(team) = *ed.selected.read() else {
        return rsx! {
            Card { title: "Team details",
                p { class: "text-sm text-zinc-500",
                    "Click a team to see its route, the teams it meets and proposals for changes."
                }
            }
        };
    };
    let ctx = ed.ctx.read().clone();
    let Some(info) = ctx.team(team).cloned() else {
        return rsx! {};
    };
    let cooks = ed.draft.read().cooking_course(team).map(|course| ed.course_name(course));
    let course_labels: Vec<(uuid::Uuid, String)> = ctx
        .courses
        .iter()
        .map(|course| (course.id, format!("{} · {}", course.time.format("%H:%M"), course.name)))
        .collect();
    let (met, never) = {
        let report = ed.report.read();
        let mut met = report.met_by(team);
        met.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| ed.name(a.0).cmp(&ed.name(b.0))));
        let never: Vec<_> = ctx
            .teams
            .iter()
            .map(|other| other.id)
            .filter(|other| *other != team && !met.iter().any(|(id, _)| id == other))
            .collect();
        (met, never)
    };

    rsx! {
        Card {
            title: "{info.name}",
            actions: rsx! {
                TextButton { label: "Close", onclick: move |_| ed.select(Some(team)) }
            },
            div { class: "flex flex-wrap items-center gap-2 text-xs",
                Pill { tone: PillTone::Neutral, "👤 {info.persons} persons" }
                match cooks {
                    Some(course) => rsx! { Pill { "Cooks {course}" } },
                    None => rsx! { Pill { tone: PillTone::Neutral, "Does not cook" } },
                }
            }

            div { class: "space-y-1",
                h4 { class: EYEBROW, "Route" }
                div { class: "rounded-xl border border-amber-100 divide-y divide-amber-100",
                    for (course, label) in course_labels {
                        div { key: "{course}", class: "px-3 py-2",
                            p { class: "text-[11px] text-zinc-500", "{label}" }
                            MatrixCell { team, course, compact: true }
                        }
                    }
                }
            }

            div { class: "space-y-1.5",
                h4 { class: EYEBROW,
                    "Meets {met.len()} of {ctx.teams.len().saturating_sub(1)} teams"
                }
                div { class: "flex flex-wrap gap-1.5",
                    for (other, count) in met {
                        TeamTag {
                            key: "{other}",
                            team: other,
                            note: if count >= 2 { Some(format!(" {count}×")) } else { None },
                        }
                    }
                }
                if !never.is_empty() {
                    Hint { "Does not meet:" }
                    div { class: "flex flex-wrap gap-1.5",
                        for other in never {
                            TeamTag { key: "{other}", team: other }
                        }
                    }
                }
            }

            div { class: "space-y-1.5",
                h4 { class: EYEBROW, "Proposals" }
                if proposals.read().is_empty() {
                    Hint { "No single change involving this team improves the plan." }
                }
                for (index, suggestion) in proposals.read().iter().cloned().enumerate() {
                    ProposalRow { key: "{index}", suggestion }
                }
            }
        }
    }
}

#[component]
pub(super) fn ProposalRow(suggestion: crate::planning::Suggestion) -> Element {
    let ed = use_context::<Editor>();
    let text = describe_op(&suggestion.op, &ed);
    let effect = describe_effect(&suggestion.effect);
    let op = suggestion.op.clone();
    rsx! {
        Panel { flush: true, class: "flex items-center gap-2 px-3 py-2",
            div { class: "min-w-0 flex-1",
                p { class: "text-xs font-semibold text-zinc-800", "{text}" }
                p { class: "text-[11px] text-emerald-700", "{effect}" }
            }
            Button {
                label: "Apply",
                variant: ButtonVariant::Secondary,
                onclick: move |_| ed.apply(op.clone()),
            }
        }
    }
}
