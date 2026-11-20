//! Perspective "Encounters": who sits at one table with whom, and how often. A green field is a
//! meeting, a red field a repeated meeting (the soft rule that teams should not meet twice).

use std::collections::HashMap;

use dioxus::prelude::*;
use uuid::Uuid;

use super::state::Editor;

#[component]
pub(super) fn Encounters() -> Element {
    let ed = use_context::<Editor>();
    let ctx = ed.ctx.read().clone();
    let (lookup, repeated, never) = {
        let report = ed.report.read();
        let mut lookup: HashMap<(Uuid, Uuid), (u32, Vec<Uuid>)> = HashMap::new();
        for meeting in &report.meetings {
            let value = (meeting.count, meeting.courses.clone());
            lookup.insert((meeting.a, meeting.b), value.clone());
            lookup.insert((meeting.b, meeting.a), value);
        }
        let teams = ctx.teams.len();
        let pairs = teams * teams.saturating_sub(1) / 2;
        (lookup, report.repeat_count(), pairs.saturating_sub(report.meetings.len()))
    };
    let selected = *ed.selected.read();

    rsx! {
        div { class: "space-y-3",
            p { class: "text-xs text-zinc-500",
                "Every field shows how often two teams sit at the same table. "
                span { class: "inline-block w-3 h-3 rounded-sm bg-emerald-200 align-middle mx-1" }
                "met once, "
                span { class: "inline-block w-3 h-3 rounded-sm bg-red-300 align-middle mx-1" }
                "met more than once, empty: never meet. "
                "{repeated} repeated meeting(s), {never} pair(s) never meet."
            }
            div { class: "overflow-x-auto rounded-2xl border border-amber-200 bg-white shadow-sm",
                table { class: "border-collapse text-xs",
                    thead {
                        tr {
                            th { class: "sticky left-0 bg-amber-50 px-3 py-2 text-left" }
                            for (index, team) in ctx.teams.iter().enumerate() {
                                th {
                                    key: "{team.id}",
                                    class: "bg-amber-50 px-1 py-2 text-[10px] font-bold text-amber-800/70 w-8 min-w-8",
                                    title: "{team.name}",
                                    "{index + 1}"
                                }
                            }
                        }
                    }
                    tbody {
                        for (row, a) in ctx.teams.iter().enumerate() {
                            tr {
                                key: "{a.id}",
                                class: if selected == Some(a.id) { "bg-amber-100/60" } else { "" },
                                th {
                                    class: "sticky left-0 bg-white px-3 py-1.5 text-left font-semibold text-zinc-700 whitespace-nowrap cursor-pointer hover:text-brand",
                                    onclick: {
                                        let id = a.id;
                                        move |_| ed.select(Some(id))
                                    },
                                    "{row + 1}. {a.name}"
                                }
                                for (column, b) in ctx.teams.iter().enumerate() {
                                    {
                                        let (count, courses) = lookup.get(&(a.id, b.id)).cloned().unwrap_or_default();
                                        let (tone, text) = if row == column {
                                            ("bg-zinc-200", String::new())
                                        } else if count >= 2 {
                                            ("bg-red-300 text-red-900 font-bold", count.to_string())
                                        } else if count == 1 {
                                            ("bg-emerald-200", String::new())
                                        } else {
                                            ("", String::new())
                                        };
                                        let title = if count == 0 {
                                            format!("{} and {} never meet", a.name, b.name)
                                        } else {
                                            let names: Vec<String> = courses.iter().map(|c| ed.course_name(*c)).collect();
                                            format!("{} and {}: {}× ({})", a.name, b.name, count, names.join(", "))
                                        };
                                        rsx! {
                                            td {
                                                key: "{b.id}",
                                                class: "h-7 w-8 min-w-8 border border-white text-center {tone}",
                                                title: "{title}",
                                                "{text}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
