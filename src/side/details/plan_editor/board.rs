//! Perspective "Courses": one row per course, one card per table (= one host with its guests).
//! Teams are chips that can be dragged to another table (move) or onto another team (swap).
//! The same team has the same color in every row; the team under the mouse is highlighted in all
//! courses, and everyone it already meets twice gets a red badge.

use dioxus::prelude::*;
use uuid::Uuid;

use super::state::Editor;
use crate::planning::{Op, Violation};
use crate::ui::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ChipRole {
    Host,
    Guest,
    Free,
}

#[component]
pub(super) fn Board() -> Element {
    let ed = use_context::<Editor>();
    let courses: Vec<Uuid> = ed.ctx.read().courses.iter().map(|course| course.id).collect();

    rsx! {
        div { class: "space-y-4",
            p { class: "text-xs text-zinc-500",
                "Drag a team onto a table to move it there, or onto another team to swap places. "
                "Hover a team to follow it through all courses; a red badge means that you already meet this team at another table. "
                "On a phone, tap a team and use the details."
            }
            for course in courses {
                CourseRow { key: "{course}", course }
            }
        }
    }
}

#[component]
fn CourseRow(course: Uuid) -> Element {
    let ed = use_context::<Editor>();
    let (title, time) = {
        let ctx = ed.ctx.read();
        match ctx.course(course) {
            Some(info) => (info.name.clone(), info.time.format("%H:%M").to_string()),
            None => return rsx! {},
        }
    };
    let tables: Vec<usize> = ed
        .draft
        .read()
        .tables_of(course)
        .filter(|(_, table)| table.host.is_some() || !table.guests.is_empty())
        .map(|(index, _)| index)
        .collect();
    let (min, max, balanced) = {
        let report = ed.report.read();
        match report.courses.iter().find(|stat| stat.course == course) {
            Some(stat) => (stat.min_persons, stat.max_persons, stat.max_persons == stat.min_persons),
            None => (0, 0, true),
        }
    };

    rsx! {
        Card {
            title: "{time}  ·  {title}",
            actions: rsx! {
                Pill {
                    tone: if balanced { PillTone::Green } else { PillTone::Amber },
                    title: "Persons per table in this course",
                    if balanced { "{min} persons per table" } else { "{min}–{max} persons per table" }
                }
            },
            div { class: "flex gap-3 overflow-x-auto pb-2",
                for index in tables {
                    TableCard { key: "{index}", index }
                }
            }
            Tray { course }
        }
    }
}

#[component]
fn TableCard(index: usize) -> Element {
    let ed = use_context::<Editor>();
    let Some(table) = ed.draft.read().tables.get(index).cloned() else {
        return rsx! {};
    };
    let course = table.course;
    let mut drag = ed.drag;
    let dragging_here = matches!(*drag.read(), Some((_, from)) if from == course);

    let (persons, mean, problems) = {
        let report = ed.report.read();
        let stat = report.courses.iter().find(|stat| stat.course == course);
        let persons = stat
            .and_then(|stat| stat.tables.iter().find(|t| t.table == index))
            .map_or(0, |t| t.persons);
        let problems: Vec<&'static str> = report
            .violations
            .iter()
            .filter(|violation| violation.table() == Some(index))
            .map(|violation| match violation {
                Violation::HostMissing { .. } => "No host",
                Violation::TooFewGuests { .. } => "Needs at least 1 guest",
                _ => "Too many guests",
            })
            .collect();
        (persons, stat.map_or(0.0, |stat| stat.mean_persons), problems)
    };
    let off = (persons as f64 - mean).abs();
    let persons_tone = if off < 0.5 {
        "text-emerald-700"
    } else if off < 2.5 {
        "text-amber-700"
    } else {
        "text-red-600"
    };
    let border = if dragging_here {
        "border-amber-400 ring-2 ring-amber-200"
    } else if problems.is_empty() {
        "border-amber-100"
    } else {
        "border-red-300"
    };
    let former = table.former_host.map(|id| ed.name(id));
    let problem_text = problems.join(", ");

    rsx! {
        div {
            class: "w-56 shrink-0 rounded-2xl border bg-white p-2.5 space-y-2 transition-colors {border}",
            ondragover: move |e| {
                if dragging_here {
                    e.prevent_default();
                }
            },
            ondrop: move |e| {
                e.prevent_default();
                let dragged = *drag.peek();
                drag.set(None);
                if let Some((team, from)) = dragged
                    && from == course
                {
                    ed.apply(Op::Move { course, team, to_table: index });
                }
            },
            match table.host {
                Some(host) => rsx! {
                    TeamChip { team: host, course, role: ChipRole::Host }
                },
                None => rsx! {
                    div {
                        class: "rounded-xl border-2 border-dashed border-red-300 bg-red-50 px-2.5 py-2 text-xs text-red-700",
                        ondragover: move |e| {
                            if dragging_here {
                                e.prevent_default();
                                e.stop_propagation();
                            }
                        },
                        ondrop: move |e| {
                            e.prevent_default();
                            e.stop_propagation();
                            let dragged = *drag.peek();
                            drag.set(None);
                            if let Some((team, from)) = dragged
                                && from == course
                            {
                                ed.apply(Op::SeatHost { table: index, team });
                            }
                        },
                        "Host needed"
                        if let Some(former) = former {
                            span { class: "opacity-70", " (was {former})" }
                        }
                    }
                },
            }
            div { class: "space-y-1.5",
                for guest in table.guests.iter().copied() {
                    TeamChip { key: "{guest}", team: guest, course, role: ChipRole::Guest }
                }
                if table.guests.is_empty() {
                    div { class: "rounded-xl border border-dashed border-zinc-300 px-2.5 py-2 text-xs text-zinc-400",
                        "No guests yet"
                    }
                }
            }
            div { class: "flex items-center justify-between text-[11px]",
                span { class: "font-semibold {persons_tone}", title: "Average of this course: {mean:.1}",
                    "{persons} persons"
                }
                if !problem_text.is_empty() {
                    span { class: "text-red-600", "{problem_text}" }
                }
            }
        }
    }
}

/// Teams without a place in this course. Dropping a team here takes it out of its seat.
#[component]
fn Tray(course: Uuid) -> Element {
    let ed = use_context::<Editor>();
    let mut drag = ed.drag;
    let dragging_here = matches!(*drag.read(), Some((_, from)) if from == course);
    let free = ed.draft.read().unseated(course, &ed.ctx.read());
    if free.is_empty() && !dragging_here {
        return rsx! {};
    }
    let border = if dragging_here { "border-amber-400 bg-amber-50" } else { "border-zinc-300" };

    rsx! {
        div {
            class: "mt-1 rounded-xl border border-dashed px-3 py-2 {border}",
            ondragover: move |e| {
                if dragging_here {
                    e.prevent_default();
                }
            },
            ondrop: move |e| {
                e.prevent_default();
                let dragged = *drag.peek();
                drag.set(None);
                if let Some((team, from)) = dragged
                    && from == course
                {
                    ed.apply(Op::Unseat { course, team });
                }
            },
            p { class: "{EYEBROW} mb-1.5",
                if free.is_empty() { "Drop here to take a team out of this course" } else { "No place in this course yet" }
            }
            div { class: "flex flex-wrap gap-1.5",
                for team in free {
                    div { key: "{team}", class: "w-52",
                        TeamChip { team, course, role: ChipRole::Free }
                    }
                }
            }
        }
    }
}

/// A team as a colored chip. Draggable; click selects, hover highlights.
#[component]
pub(super) fn TeamChip(team: Uuid, course: Uuid, role: ChipRole) -> Element {
    let ed = use_context::<Editor>();
    let colors = ed.colors(team);
    let persons = ed.ctx.read().persons(team);
    let focus = ed.focus();
    let selected = *ed.selected.read() == Some(team);
    let is_focus = focus == Some(team);
    let meets = match focus {
        Some(other) if other != team => ed.report.read().meeting_count(other, team),
        _ => 0,
    };
    let mut drag = ed.drag;
    let mut hovered = ed.hovered;
    let dragging_here = matches!(*drag.read(), Some((other, from)) if from == course && other != team);

    let ring = if selected {
        "ring-2 ring-brand ring-offset-1"
    } else if is_focus {
        "ring-2 ring-zinc-700 ring-offset-1"
    } else {
        ""
    };
    let style = format!(
        "background:{}; border-color:{}; color:{};",
        colors.background, colors.border, colors.ink
    );
    let name = ed.name(team);

    rsx! {
        div {
            class: "flex items-center gap-1.5 rounded-xl border px-2.5 py-1.5 text-xs font-semibold \
                    cursor-grab active:cursor-grabbing select-none transition-shadow hover:shadow-sm {ring}",
            style: "{style}",
            draggable: true,
            title: "{name}",
            ondragstart: move |e| {
                let _ = e.data_transfer().set_data("text/plain", &team.to_string());
                e.data_transfer().set_effect_allowed("move");
                drag.set(Some((team, course)));
            },
            ondragend: move |_| drag.set(None),
            ondragover: move |e| {
                if dragging_here {
                    e.prevent_default();
                    e.stop_propagation();
                }
            },
            ondrop: move |e| {
                e.prevent_default();
                e.stop_propagation();
                let dragged = *drag.peek();
                drag.set(None);
                if let Some((other, from)) = dragged
                    && from == course
                    && other != team
                {
                    ed.apply(Op::Swap { course, a: other, b: team });
                }
            },
            onclick: move |e| {
                e.stop_propagation();
                ed.select(Some(team));
            },
            onmouseenter: move |_| hovered.set(Some(team)),
            onmouseleave: move |_| hovered.set(None),
            if role == ChipRole::Host {
                span { class: "text-[9px] font-semibold opacity-70", "Host" }
            }
            span { class: "truncate", "{name}" }
            span { class: "ml-auto shrink-0 text-[10px] font-medium opacity-70", "👤{persons}" }
            if meets >= 2 {
                span {
                    class: "shrink-0 rounded-full bg-red-500 px-1.5 text-[10px] font-bold text-white",
                    title: "You meet this team {meets} times",
                    "{meets}×"
                }
            }
        }
    }
}

/// A team as a small, not draggable label (lists, details).
#[component]
pub(super) fn TeamTag(team: Uuid, #[props(default)] note: Option<String>) -> Element {
    let ed = use_context::<Editor>();
    let colors = ed.colors(team);
    let name = ed.name(team);
    let style = format!(
        "background:{}; border-color:{}; color:{};",
        colors.background, colors.border, colors.ink
    );
    rsx! {
        TagButton {
            style,
            onclick: move |_| ed.select(Some(team)),
            "{name}"
            if let Some(note) = note {
                span { class: "font-normal opacity-70", "{note}" }
            }
        }
    }
}
