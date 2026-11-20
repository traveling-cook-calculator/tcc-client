//! The plan board of the Calculation page: look at the plan course by course, change it by hand
//! (drag teams between tables) and let the rules check every change.
//!
//! * Courses – a board per course, drag teams between tables
//! * Next to it: details of the selected team, the rules, proposals.
//!
//! Nothing is stored until "Save" is pressed. The board is embedded in the Calculation page
//! (`PlanBoard`); it is not a page of its own any more.

mod board;
mod describe;
mod inspector;
mod matrix;
mod route_mails;
mod rules_panel;
mod stale;
mod state;

use dioxus::prelude::*;
use uuid::Uuid;

use self::board::Board;
use self::inspector::Inspector;
use self::route_mails::RouteMailsDialog;
use self::rules_panel::{Proposals, RulesPanel};
use self::stale::StalePanel;
use self::state::Editor;
use super::plan_bundle::{team_palette, PlanBundle};
use crate::data::use_storage;
use crate::planning::{evaluate, improve, Draft, Sanitized};
use crate::storage::PlanData;
use crate::ui::*;

/// The plan as an editable board.
///
/// The draft is created once from `plan`; edits never reload the page. To show a *different* stored
/// plan (e.g. after a recalculation) the caller gives the component a new `key`.
///
/// `menu_items` are further entries of the "More" menu of the toolbar (e.g. "Recalculate").
#[component]
pub(crate) fn PlanBoard(
    cook_and_run_id: Uuid,
    bundle: PlanBundle,
    plan: PlanData,
    #[props(default)] menu_items: Option<Element>,
) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let save_task = use_busy();

    // Everything below is created once; edits never reload the page.
    let ctx = use_hook(|| bundle.context());
    let (initial, sanitized) = use_hook(|| Draft::from_plan(&plan, &ctx));
    let ctx_signal = use_signal(|| ctx.clone());
    let draft = use_signal(|| initial.clone());
    let report = use_memo(move || evaluate(&draft.read(), &ctx_signal.read()));
    let ed = Editor {
        draft,
        baseline: use_signal(|| initial.clone()),
        history: use_signal(Vec::new),
        selected: use_signal(|| None),
        hovered: use_signal(|| None),
        drag: use_signal(|| None),
        ctx: ctx_signal,
        names: use_signal(|| bundle.names()),
        palette: use_signal(|| team_palette(&ctx)),
        report,
        toasts,
    };
    use_context_provider(|| ed);

    let mut confirm_save = use_signal(|| false);
    let mut route_mails = use_signal(|| false);
    // After a successful save the outdated marker and what the stored plan lost are history.
    let mut saved_once = use_signal(|| false);

    let report_now = report.read().clone();
    let violations = report_now.violations.len();
    let storable = ed.draft.read().to_plan_data();
    let dirty = ed.dirty();

    let do_save = use_callback(move |_: ()| {
        save_task.run(async move {
            let current = ed.draft.peek().clone();
            let plan = match current.to_plan_data() {
                Ok(plan) => plan,
                Err(e) => return toasts.error("The plan cannot be saved yet", e),
            };
            let result = { storage.write().await.update_plan_of_cook_and_run(cook_and_run_id, &plan).await };
            match result {
                Ok(()) => {
                    ed.mark_saved(current);
                    saved_once.set(true);
                    storage.invalidate();
                    toasts.success("Plan saved", "The schedules of all teams now follow this plan.");
                }
                Err(e) => toasts.error("Saving the plan failed", e),
            }
        });
    });

    let improve_all = move |_| {
        let before = ed.draft.peek().clone();
        let result = improve(&before, &ed.ctx.peek());
        if result.changes == 0 {
            toasts.info("Nothing to improve", "No change makes the plan better.");
        } else {
            let changes = result.changes;
            ed.commit(before, result.draft);
            toasts.success("Plan improved", format!("{changes} place(s) changed. You can undo it."));
        }
    };

    let stale_since = if saved_once() { None } else { plan.stale_since.or(bundle.plan.as_ref().and_then(|p| p.stale_since)) };
    let changed: Vec<Uuid> = match stale_since {
        Some(since) if !saved_once() => {
            bundle.taking_part().filter(|team| team.edited >= since).map(|team| team.id).collect()
        }
        _ => Vec::new(),
    };
    let sanitized_now = if saved_once() { Sanitized::default() } else { sanitized.clone() };

    // What is wrong with the plan, in a few words. Nothing is shown for what is fine.
    let mut issues: Vec<String> = Vec::new();
    if violations > 0 {
        issues.push(format!("{violations} rule violation(s)"));
    }
    if report_now.repeat_count() > 0 {
        issues.push(format!("{} repeated meeting(s)", report_now.repeat_count()));
    }
    if !report_now.balanced() {
        issues.push("uneven tables".to_string());
    }
    let in_order = issues.is_empty();

    rsx! {
        div { class: "space-y-4",
            StalePanel {
                cook_and_run_id,
                stale_since,
                is_cloud: bundle.is_cloud,
                sanitized: sanitized_now,
                changed,
            }

            // One quiet bar: what is the state of the plan on the left, what can be done on the right.
            // Only "Save" is a coloured button, and only while there is something to save.
            Toolbar {
                div { class: "flex flex-wrap items-center gap-x-3 gap-y-1 text-sm min-w-0",
                    span {
                        class: if in_order { "h-2 w-2 shrink-0 rounded-full bg-emerald-500" } else { "h-2 w-2 shrink-0 rounded-full bg-amber-500" },
                        "aria-hidden": "true",
                    }
                    span { class: if in_order { "text-zinc-700" } else { "text-amber-800" },
                        if in_order { "Plan is in order" } else { "{issues.join(\" · \")}" }
                    }
                    if dirty {
                        span { class: "text-xs text-zinc-500", "Unsaved changes" }
                    }
                }
                div { class: "flex flex-wrap items-center gap-2",
                    if dirty {
                        Button { label: "Undo", variant: ButtonVariant::Secondary, disabled: !ed.can_undo(), onclick: move |_| ed.undo() }
                        Button { label: "Discard", variant: ButtonVariant::Secondary, onclick: move |_| ed.discard() }
                        Button {
                            label: "Save",
                            loading: save_task.is_busy(),
                            disabled: storable.is_err(),
                            onclick: move |_| {
                                if violations > 0 {
                                    confirm_save.set(true);
                                } else {
                                    do_save.call(());
                                }
                            },
                        }
                    }
                    ActionMenu { label: "More",
                        MenuItem { label: "Improve automatically", onclick: improve_all }
                        if bundle.is_cloud {
                            MenuItem { label: "Send route mails…", onclick: move |_| route_mails.set(true) }
                        }
                        if let Some(items) = menu_items {
                            MenuDivider {}
                            {items}
                        }
                    }
                }
            }
            if let Err(reason) = &storable {
                if dirty {
                    p { class: "text-xs text-red-600", "Cannot be saved yet: {reason}" }
                }
            }

            div { class: "grid grid-cols-1 xl:grid-cols-[minmax(0,1fr)_24rem] gap-6 items-start",
                div { class: "min-w-0", Board {} }
                div { class: "space-y-4 min-w-0",
                    Inspector {}
                    RulesPanel {}
                    Proposals {}
                }
            }

            if route_mails() {
                RouteMailsDialog {
                    cook_and_run_id,
                    teams: bundle.teams.clone(),
                    outdated: stale_since.is_some(),
                    unsaved: dirty,
                    onclose: move |_| route_mails.set(false),
                }
            }

            if confirm_save() {
                Dialog { title: "Save a plan that breaks rules?", onclose: move |_| confirm_save.set(false),
                    p { class: "text-sm text-zinc-700",
                        "The plan breaks {violations} rule(s) that must be kept (for example a team does not cook or is missing in a course). "
                        "The schedules would show these gaps."
                    }
                    div { class: "flex flex-wrap justify-end gap-2",
                        Button { label: "Keep editing", variant: ButtonVariant::Secondary, onclick: move |_| confirm_save.set(false) }
                        Button {
                            label: "Save anyway",
                            variant: ButtonVariant::Danger,
                            onclick: move |_| {
                                confirm_save.set(false);
                                do_save.call(());
                            },
                        }
                    }
                }
            }
        }
    }
}
