//! Human readable texts for changes, rule violations and effects.

use dioxus::prelude::*;

use super::state::Editor;
use crate::planning::{Effect, Op, Reason, Violation};

pub(super) fn describe_op(op: &Op, ed: &Editor) -> String {
    match op {
        Op::Move { course, team, to_table } => format!(
            "Move {} to the table of {} ({})",
            ed.name(*team),
            ed.table_label(*to_table),
            ed.course_name(*course)
        ),
        Op::Swap { course, a, b } => {
            format!("Swap {} and {} ({})", ed.name(*a), ed.name(*b), ed.course_name(*course))
        }
        Op::SwapRoles { a, b } => {
            format!("{} and {} swap their cooking roles", ed.name(*a), ed.name(*b))
        }
        Op::SeatHost { table, team } => {
            let course = ed.draft.read().tables.get(*table).map(|t| t.course);
            format!(
                "{} takes over the table without host ({})",
                ed.name(*team),
                course.map(|c| ed.course_name(c)).unwrap_or_default()
            )
        }
        Op::OpenTable { course, host, take } => {
            let guests: Vec<String> = take.iter().map(|id| ed.name(*id)).collect();
            format!(
                "{} cooks {} with {} as guests",
                ed.name(*host),
                ed.course_name(*course),
                guests.join(" and ")
            )
        }
        Op::Unseat { course, team } => {
            format!("Remove {} from {}", ed.name(*team), ed.course_name(*course))
        }
    }
}

pub(super) fn describe_violation(violation: &Violation, ed: &Editor) -> String {
    match violation {
        Violation::NeverCooks { team } => format!("{} does not cook.", ed.name(*team)),
        Violation::CooksMultiple { team, courses } => {
            let names: Vec<String> = courses.iter().map(|c| ed.course_name(*c)).collect();
            format!("{} cooks {} times ({}).", ed.name(*team), courses.len(), names.join(", "))
        }
        Violation::MissingInCourse { team, course } => {
            format!("{} has no place for {}.", ed.name(*team), ed.course_name(*course))
        }
        Violation::DoubleBooked { team, course } => {
            format!("{} is at two places for {}.", ed.name(*team), ed.course_name(*course))
        }
        Violation::HostMissing { table, course } => {
            let former = ed.draft.read().tables.get(*table).and_then(|t| t.former_host);
            match former {
                Some(former) => format!(
                    "A table for {} has no host (it was {}).",
                    ed.course_name(*course),
                    ed.name(former)
                ),
                None => format!("A table for {} has no host.", ed.course_name(*course)),
            }
        }
        Violation::TooFewGuests { table, course, count } => format!(
            "The table of {} ({}) has {} guests, at least 1 is needed.",
            ed.table_label(*table),
            ed.course_name(*course),
            count
        ),
        Violation::TooManyGuests { table, course, count } => format!(
            "The table of {} ({}) has {} guests, at most 5 fit.",
            ed.table_label(*table),
            ed.course_name(*course),
            count
        ),
    }
}

pub(super) fn describe_reason(reason: &Reason, ed: &Editor) -> String {
    match reason {
        Reason::NoSeat { course } => format!("no place for {}", ed.course_name(*course)),
        Reason::HostLeft { course } => format!("host of {} left", ed.course_name(*course)),
        Reason::TableChanged { course } => format!("table of {} changed", ed.course_name(*course)),
        Reason::NoCookingRole => "does not cook yet".to_string(),
    }
}

pub(super) fn describe_effect(effect: &Effect) -> String {
    let mut parts: Vec<String> = Vec::new();
    if effect.hard.1 < effect.hard.0 {
        let fixed = effect.hard.0 - effect.hard.1;
        parts.push(format!("fixes {fixed} rule violation{}", if fixed == 1 { "" } else { "s" }));
    }
    if effect.repeats.1 < effect.repeats.0 {
        let fewer = effect.repeats.0 - effect.repeats.1;
        parts.push(format!("{fewer} repeated meeting{} less", if fewer == 1 { "" } else { "s" }));
    } else if effect.repeats.1 > effect.repeats.0 {
        let more = effect.repeats.1 - effect.repeats.0;
        parts.push(format!("{more} more repeated meeting{}", if more == 1 { "" } else { "s" }));
    }
    if effect.imbalance.1 + 0.01 < effect.imbalance.0 {
        parts.push("more even tables".to_string());
    } else if effect.imbalance.1 > effect.imbalance.0 + 0.01 {
        parts.push("less even tables".to_string());
    }
    if parts.is_empty() { "slightly better".to_string() } else { parts.join(" · ") }
}
