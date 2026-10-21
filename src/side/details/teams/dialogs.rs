//! "Add team" and "Edit team" dialogs.

use dioxus::prelude::*;
use uuid::Uuid;

use super::draft::TeamDraft;
use super::form::TeamFields;
use super::notes::TeamNotes;
use crate::data::use_storage;
use crate::side::details::address::AddressParam;
use crate::side::participant::team_url;
use crate::storage::{AddressData, TeamData, TeamStatus};
use crate::time::format_local;
use crate::ui::*;

// ─────────────────────────────────────────────
//  Add
// ─────────────────────────────────────────────

#[component]
pub fn AddTeamDialog(cook_and_run_id: Uuid, onclose: EventHandler<()>) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let create_task = use_busy();

    let draft = use_signal(TeamDraft::default);
    let address = AddressParam::new(&AddressData::default());
    let mut attempted = use_signal(|| false);

    let on_create = move |_| {
        attempted.set(true);
        let current = draft.peek().clone();
        if !current.errors().is_empty() {
            return;
        }
        // Also lets the address form mark what is wrong.
        let Ok(address_data) = address.get_address_data() else { return };
        let team = match current.to_create(address_data, false) {
            Ok(team) => team,
            Err(e) => return toasts.error("Please check your input", e),
        };

        create_task.run(async move {
            let name = team.name.clone();
            let result = {
                storage.write().await.create_team_of_cook_and_run(cook_and_run_id, Uuid::new_v4(), &team).await
            };
            match result {
                Ok(()) => {
                    storage.invalidate();
                    toasts.success("Team added", name);
                    onclose.call(());
                }
                Err(e) => toasts.error("Creating the team failed", e),
            }
        });
    };

    rsx! {
        Dialog { title: "Add Team", wide: true, onclose,
            TeamFields { draft, address, show_all_errors: attempted() }
            div { class: "flex justify-center",
                Button { label: "Create Team", loading: create_task.is_busy(), onclick: on_create }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Edit
//
//  The full team (details + notes, two requests in the cloud) is loaded when the dialog opens.
//  The list behind it only knows the short form.
// ─────────────────────────────────────────────

#[component]
pub fn EditTeamDialog(cook_and_run_id: Uuid, team_id: Uuid, onclose: EventHandler<()>) -> Element {
    let storage = use_storage();
    // Reads `revision()`: after a note was posted the dialog reloads the team (the old value
    // stays on screen until the new one has arrived).
    let mut team = use_resource(move || async move {
        let _ = storage.revision();
        storage.read().await.select_cook_and_run_team(cook_and_run_id, team_id).await
    });

    rsx! {
        Dialog { title: "Edit Team", wide: true, onclose,
            match &*team.read_unchecked() {
                None => rsx! { TeamSkeleton {} },
                Some(Err(details)) => rsx! {
                    ErrorView {
                        error_text: "The team could not be loaded.",
                        error_details: details.clone(),
                        onretry: move |_| team.restart(),
                    }
                },
                Some(Ok(data)) => rsx! {
                    EditTeamForm { key: "{team_id}", cook_and_run_id, team: data.clone(), onclose }
                },
            }
        }
    }
}

/// Same layout as the loaded form, so nothing jumps when the data arrives.
#[component]
fn TeamSkeleton() -> Element {
    rsx! {
        div { class: "space-y-5", "aria-busy": "true",
            div { class: "flex gap-4 border-b border-amber-100 pb-3",
                Skeleton { class: "h-5 w-24" }
                Skeleton { class: "h-5 w-16" }
            }
            div { class: "flex flex-col md:flex-row gap-6",
                div { class: "flex-1 space-y-4",
                    for row in 0..5 {
                        div { key: "{row}", class: "space-y-1.5",
                            Skeleton { class: "h-3 w-24" }
                            Skeleton { class: "h-10 w-full" }
                        }
                    }
                }
                div { class: "flex-1 space-y-3",
                    Skeleton { class: "h-3 w-16" }
                    Skeleton { class: "h-8 w-full" }
                    Skeleton { class: "h-10 w-full" }
                    Skeleton { class: "h-9 w-28" }
                }
            }
            span { class: "sr-only", "Loading team…" }
        }
    }
}

#[component]
fn EditTeamForm(cook_and_run_id: Uuid, team: TeamData, onclose: EventHandler<()>) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let update_task = use_busy();
    let delete_task = use_busy();

    // A canceled team can no longer be edited (the storage layer refuses it).
    let canceled = team.status == TeamStatus::Canceled;

    let draft = use_signal(|| TeamDraft::from_team(&team));
    let address = AddressParam::new(&team.address);
    let mut needs_check = use_signal(|| team.needs_check());
    let mut tab = use_signal(|| 0usize);
    let mut confirm_delete = use_signal(|| false);

    // What was loaded; "unsaved changes" = anything that differs from it.
    let initial_draft = use_hook(|| TeamDraft::from_team(&team));
    let initial_address = use_hook(|| address.snapshot());

    let current = draft.read().clone();
    let dirty = current != initial_draft
        || needs_check() != team.needs_check()
        || address.snapshot() != initial_address;
    let can_update = !canceled && dirty && current.errors().is_empty();

    let team_id = team.id;
    let on_update = move |_| {
        let current = draft.peek().clone();
        let Ok(address_data) = address.get_address_data() else { return };
        let update = match current.to_update(address_data, *needs_check.peek()) {
            Ok(update) => update,
            Err(e) => return toasts.error("Please check your input", e),
        };
        update_task.run(async move {
            let result = {
                storage.write().await.update_team_of_cook_and_run(cook_and_run_id, team_id, &update).await
            };
            match result {
                Ok(()) => {
                    storage.invalidate();
                    toasts.success("Team updated", update.name);
                    onclose.call(());
                }
                Err(e) => toasts.error("Updating the team failed", e),
            }
        });
    };

    let on_delete = move |_| {
        delete_task.run(async move {
            let result = { storage.write().await.delete_team_of_cook_and_run(cook_and_run_id, team_id).await };
            match result {
                Ok(()) => {
                    storage.invalidate();
                    toasts.success("Team deleted", "");
                    onclose.call(());
                }
                Err(e) => toasts.error("Deleting the team failed", e),
            }
        });
    };

    let header_actions = rsx! {
        if canceled {
            Pill { tone: PillTone::Neutral, "Canceled" }
        } else {
            Checkbox {
                label: "Needs review",
                checked: needs_check(),
                onchange: move |checked| needs_check.set(checked),
            }
            if confirm_delete() {
                span { class: "text-xs text-red-600 font-medium", "Delete this team?" }
                Button {
                    label: "Delete",
                    variant: ButtonVariant::Danger,
                    loading: delete_task.is_busy(),
                    onclick: on_delete,
                }
                Button {
                    label: "Cancel",
                    variant: ButtonVariant::Secondary,
                    disabled: delete_task.is_busy(),
                    onclick: move |_| confirm_delete.set(false),
                }
            } else {
                DeleteButton { onclick: move |_| confirm_delete.set(true) }
            }
        }
    };

    rsx! {
        div { class: "space-y-5",
            if canceled {
                CanceledNotice { team: team.clone() }
            }

            Tabs {
                tabs: vec!["Team Data", "Notes"],
                selected: tab(),
                onselect: move |index| tab.set(index),
                actions: header_actions,
            }

            if tab() == 0 {
                TeamFields { draft, address, show_all_errors: true, disabled: canceled }
                if let Some(cloud) = &team.cloud {
                    CopyField {
                        label: "Team link",
                        value: team_url(cook_and_run_id, team_id, cloud.access_token),
                        hint: "Personal link of the team: it opens the team page where the team can edit its data, see its plan or cancel. Only share it with the team."
                            .to_string(),
                    }
                }
                if !canceled {
                    div { class: "flex justify-center",
                        Button {
                            label: "Update Team",
                            loading: update_task.is_busy(),
                            disabled: !can_update,
                            onclick: on_update,
                        }
                    }
                }
            } else {
                TeamNotes { cook_and_run_id, team_id, notes: team.note_list.clone() }
            }
        }
    }
}

#[component]
fn CanceledNotice(team: TeamData) -> Element {
    let cloud = team.cloud.as_ref();
    let when = cloud.and_then(|c| c.canceled_at).map(format_local);
    let reason = cloud.and_then(|c| c.cancel_reason.clone()).filter(|r| !r.trim().is_empty());

    rsx! {
        div { class: "rounded-xl border border-red-200 bg-red-50 px-4 py-3 space-y-1",
            p { class: "text-sm font-semibold text-red-700",
                "This team canceled its participation and can no longer be edited."
            }
            if let Some(when) = when {
                p { class: "text-xs text-red-600", "Canceled on {when}" }
            }
            if let Some(reason) = reason {
                p { class: "text-xs text-red-600 whitespace-pre-wrap", "Reason: {reason}" }
            }
        }
    }
}
