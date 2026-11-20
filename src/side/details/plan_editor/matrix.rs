//! The "where is this team?" cell of the inspector: at whose place a team is for one course, with a
//! select to move it to another table or to swap who cooks.

use dioxus::prelude::*;
use uuid::Uuid;

use super::state::Editor;
use crate::planning::{Op, Seat};
use crate::ui::{PickOption, PickSelect};

#[component]
pub(super) fn MatrixCell(team: Uuid, course: Uuid, #[props(default)] compact: bool) -> Element {
    let ed = use_context::<Editor>();
    let draft = ed.draft.read().clone();
    let seat = draft.seat(course, team);
    let current_table = seat.map(Seat::table);

    // Every table of the course, as (index, label, full).
    let tables: Vec<(usize, String, bool)> = draft
        .tables_of(course)
        .filter(|(_, table)| table.host.is_some() || !table.guests.is_empty())
        .map(|(index, table)| {
            let label = match table.host {
                Some(host) => format!("{} ({} guests)", ed.name(host), table.guests.len()),
                None => format!("No host ({} guests)", table.guests.len()),
            };
            (index, label, table.guests.len() >= crate::storage::MAX_GUESTS_PER_HOSTING)
        })
        .collect();
    let others: Vec<Uuid> = ed.ctx.read().teams.iter().map(|t| t.id).filter(|id| *id != team).collect();
    let select_key = format!("{team}-{course}-{current_table:?}");

    rsx! {
        div { key: "{select_key}", class: if compact { "text-sm" } else { "px-4 py-3 border-l border-amber-100 text-sm" },
            match seat {
                Some(Seat::Host(_)) => rsx! {
                    span { class: "inline-flex items-center gap-1 rounded-lg border border-brand-hover px-2 py-0.5 text-xs font-semibold text-brand-hover",
                        "Cooks here"
                    }
                    PickSelect {
                        class: "mt-1.5",
                        placeholder: "Swap cooking with…",
                        options: others.iter().map(|other| PickOption::new(other.to_string(), ed.name(*other))).collect::<Vec<_>>(),
                        onpick: move |value: String| {
                            if let Ok(other) = value.parse::<Uuid>() {
                                ed.apply(Op::SwapRoles { a: team, b: other });
                            }
                        },
                    }
                },
                Some(Seat::Guest(table)) => rsx! {
                    span { class: "text-xs font-semibold text-zinc-800", "{ed.table_label(table)}" }
                    TableSelect { team, course, tables: tables.clone(), current: Some(table) }
                },
                None => rsx! {
                    span { class: "text-xs font-semibold text-red-600", "No place" }
                    TableSelect { team, course, tables: tables.clone(), current: None }
                },
            }
        }
    }
}

#[component]
fn TableSelect(team: Uuid, course: Uuid, tables: Vec<(usize, String, bool)>, current: Option<usize>) -> Element {
    let ed = use_context::<Editor>();
    rsx! {
        PickSelect {
            class: "mt-1.5",
            placeholder: "Move to…",
            options: tables
                .into_iter()
                .map(|(index, label, full)| PickOption::new(index.to_string(), label).disabled_if(full || Some(index) == current))
                .collect::<Vec<_>>(),
            onpick: move |value: String| {
                if let Ok(table) = value.parse::<usize>() {
                    ed.apply(Op::Move { course, team, to_table: table });
                }
            },
        }
    }
}
