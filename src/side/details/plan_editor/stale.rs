//! "This plan does not fit the teams any more": shows who is affected and offers the next steps.
//! The cloud marks a plan as outdated (`stale_since`); a local plan never is, so the panel also
//! looks at the plan itself (teams that were canceled or deleted, teams without a place).

use chrono::NaiveDateTime;
use dioxus::prelude::*;
use uuid::Uuid;

use super::board::TeamTag;
use super::describe::describe_reason;
use super::state::Editor;
use crate::data::use_storage;
use crate::planning::{diagnose, improve, Sanitized};
use crate::ui::*;

#[component]
pub(super) fn StalePanel(
    cook_and_run_id: Uuid,
    stale_since: Option<NaiveDateTime>,
    is_cloud: bool,
    sanitized: Sanitized,
    /// Teams that were changed after the plan became outdated (cloud).
    changed: Vec<Uuid>,
) -> Element {
    let ed = use_context::<Editor>();
    let storage = use_storage();
    let toasts = use_toasts();
    let confirm_task = use_busy();

    // Once the user edits the plan, what the stored plan lost is shown by the live check only.
    let empty = Sanitized::default();
    let source = if ed.dirty() { &empty } else { &sanitized };
    let diagnosis = diagnose(&ed.draft.read(), &ed.ctx.read(), source);

    if stale_since.is_none() && diagnosis.is_clean() {
        return rsx! {};
    }
    let outdated = stale_since.is_some() || !source.is_empty();
    let title = if outdated { "This plan is outdated" } else { "Some teams need attention" };
    let ghosts = diagnosis.ghosts.iter().map(|id| ed.name(*id)).collect::<Vec<_>>().join(", ");
    let since_text = stale_since.map(|since| since.format("%d.%m.%Y at %H:%M").to_string());
    let affected_rows: Vec<(Uuid, String)> = diagnosis
        .affected
        .iter()
        .map(|a| (a.team, a.reasons.iter().map(|r| describe_reason(r, &ed)).collect::<Vec<_>>().join(" · ")))
        .collect();

    let has_affected = !diagnosis.affected.is_empty();
    let fill_gaps = move |_| {
        let before = ed.draft.peek().clone();
        let result = improve(&before, &ed.ctx.peek());
        if result.changes == 0 {
            toasts.info("Nothing to place", "No change was found that helps. Place the teams by hand.");
        } else {
            let changes = result.changes;
            ed.commit(before, result.draft);
            toasts.success(
                "Plan updated",
                format!("{changes} place(s) changed. Check the result and save it; you can undo it."),
            );
        }
    };
    let confirm = move |_| {
        confirm_task.run(async move {
            let result = { storage.write().await.confirm_plan_of_cook_and_run(cook_and_run_id).await };
            match result {
                Ok(()) => {
                    storage.invalidate();
                    toasts.success("Plan confirmed", "The plan is marked as up to date.");
                }
                Err(e) => toasts.error("Could not confirm the plan", e),
            }
        });
    };

    rsx! {
        div { class: "rounded-2xl border border-amber-300 bg-amber-50 px-5 py-4 text-sm text-amber-900 space-y-3",
            p { class: "font-bold", "{title}" }
            if let Some(since) = since_text {
                p { "Teams or locations changed on {since}. Check the teams below, adjust who goes where, and save." }
            }
            if !ghosts.is_empty() {
                p { "Still in the stored plan, but not taking part any more: {ghosts}. Their places are free now." }
            }
            if !changed.is_empty() {
                div { class: "flex flex-wrap items-center gap-1.5",
                    span { "Changed since then:" }
                    for team in changed {
                        TeamTag { key: "{team}", team }
                    }
                }
            }
            if !affected_rows.is_empty() {
                div { class: "space-y-1.5",
                    p { class: "font-semibold", "Affected teams ({affected_rows.len()})" }
                    for (team, reasons) in affected_rows {
                        div { key: "{team}", class: "flex flex-wrap items-center gap-2 text-xs",
                            TeamTag { team }
                            span { class: "text-amber-800", "{reasons}" }
                        }
                    }
                }
            }
            div { class: "flex flex-wrap gap-2 pt-1",
                if has_affected {
                    Button { label: "Fill the gaps automatically", onclick: fill_gaps }
                }
                if is_cloud && stale_since.is_some() {
                    Button {
                        label: "The plan is still fine",
                        variant: ButtonVariant::Secondary,
                        loading: confirm_task.is_busy(),
                        onclick: confirm,
                    }
                }
                p { class: "text-xs text-amber-800 self-center", "Or use \"Recalculate\" above to start over." }
            }
        }
    }
}
