//! Plan editing logic without any UI: the editable plan, the rules and the suggestions.
//!
//! | file         | contents                                                                  |
//! |--------------|---------------------------------------------------------------------------|
//! | `model.rs`   | [`Draft`] (editable plan), [`Op`] (every change a user can make), context |
//! | `rules.rs`   | [`evaluate`]: hard rules, soft rules, score, who meets whom               |
//! | `suggest.rs` | [`suggest`], [`improve`], [`diagnose`]: proposals and automatic repair    |
//!
//! # Terms
//! * **Table** – one hosting: one course, one host team, 1..=5 guest teams (`HostingData`).
//! * **Seat** – the place of a team at a table, as host or as guest.
//!
//! # Rules
//! Hard (a plan that breaks them is wrong):
//! 1. every team cooks exactly once,
//! 2. every team has exactly one seat per course (otherwise it misses a course or is in two
//!    places at the same time),
//! 3. every table has a host and between 1 and 5 guests (the limits of the backend).
//!
//! Soft (the plan gets worse, but stays usable):
//! 1. the persons are spread evenly over the tables of a course,
//! 2. teams meet each other as rarely as possible more than once.

mod model;
mod rules;
mod suggest;

pub use model::{
    Context, CourseInfo, DEFAULT_PERSONS, Draft, Op, Sanitized, Seat, Table, TeamInfo,
};
pub use rules::{Meeting, Report, Rule, Score, Scorer, Violation, evaluate};
pub use suggest::{
    Affected, Diagnosis, Effect, Improvement, Reason, Suggestion, diagnose, improve, suggest,
};
