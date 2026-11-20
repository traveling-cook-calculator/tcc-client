//! The state of the plan editor. One `Editor` is created by the page and handed to every
//! component through the context, so that all views edit the same draft with the same undo.

use std::collections::HashMap;

use dioxus::prelude::*;
use uuid::Uuid;

use super::super::plan_bundle::TeamColors;
use crate::planning::{Context, Draft, Op, Report};
use crate::ui::toast::Toasts;

const HISTORY_LIMIT: usize = 50;

#[derive(Clone, Copy)]
pub(super) struct Editor {
    /// The plan as it is edited right now.
    pub draft: Signal<Draft>,
    /// The plan as it is stored (to see whether there is something to save).
    pub baseline: Signal<Draft>,
    pub history: Signal<Vec<Draft>>,
    /// Clicked team (details on the right).
    pub selected: Signal<Option<Uuid>>,
    /// Team under the mouse: its other seats are highlighted in all courses.
    pub hovered: Signal<Option<Uuid>>,
    /// Team being dragged and the course it comes from.
    pub drag: Signal<Option<(Uuid, Uuid)>>,
    pub ctx: Signal<Context>,
    pub names: Signal<HashMap<Uuid, String>>,
    pub palette: Signal<HashMap<Uuid, TeamColors>>,
    pub report: Memo<Report>,
    pub toasts: Toasts,
}

impl Editor {
    /// Applies a change if it is possible. Every change can be undone.
    pub fn apply(&self, op: Op) {
        let current = self.draft.peek().clone();
        match current.applied(&op) {
            Ok(next) => self.commit(current, next),
            Err(message) => self.toasts.error("This change is not possible", message),
        }
    }

    /// Replaces the whole plan (automatic repair) as one undo step.
    pub fn commit(&self, before: Draft, next: Draft) {
        let mut history = self.history;
        let mut draft = self.draft;
        {
            let mut steps = history.write();
            steps.push(before);
            if steps.len() > HISTORY_LIMIT {
                steps.remove(0);
            }
        }
        draft.set(next);
    }

    pub fn undo(&self) {
        let mut history = self.history;
        let previous = history.write().pop();
        if let Some(previous) = previous {
            let mut draft = self.draft;
            draft.set(previous);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.history.read().is_empty()
    }

    /// The stored plan is now `plan`.
    pub fn mark_saved(&self, plan: Draft) {
        let (mut baseline, mut history, mut draft) = (self.baseline, self.history, self.draft);
        baseline.set(plan.clone());
        draft.set(plan);
        history.set(Vec::new());
    }

    pub fn discard(&self) {
        let baseline = self.baseline.peek().clone();
        let (mut history, mut draft) = (self.history, self.draft);
        draft.set(baseline);
        history.set(Vec::new());
    }

    pub fn dirty(&self) -> bool {
        !self.draft.read().same_plan(&self.baseline.read())
    }

    pub fn select(&self, team: Option<Uuid>) {
        let mut selected = self.selected;
        let next = if *selected.peek() == team { None } else { team };
        selected.set(next);
    }

    pub fn name(&self, id: Uuid) -> String {
        self.names.read().get(&id).cloned().unwrap_or_else(|| "(deleted team)".to_string())
    }

    pub fn course_name(&self, id: Uuid) -> String {
        self.ctx.read().course(id).map_or_else(|| "(removed course)".to_string(), |c| c.name.clone())
    }

    pub fn colors(&self, id: Uuid) -> TeamColors {
        self.palette.read().get(&id).cloned().unwrap_or_else(|| TeamColors {
            background: "#F4F4F5".into(),
            border: "#A1A1AA".into(),
            ink: "#52525B".into(),
            solid: "#A1A1AA".into(),
        })
    }

    /// The team whose seats are highlighted: hovered first, otherwise the selected one.
    pub fn focus(&self) -> Option<Uuid> {
        (*self.hovered.read()).or(*self.selected.read())
    }

    /// Name of the host of a table, or a hint if the table has none.
    pub fn table_label(&self, table: usize) -> String {
        match self.draft.read().tables.get(table).and_then(|t| t.host) {
            Some(host) => self.name(host),
            None => "a table without host".to_string(),
        }
    }
}
