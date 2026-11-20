//! "Add team" and "Edit team" dialogs.

use dioxus::prelude::*;
use uuid::Uuid;

use super::draft::TeamDraft;
use super::form::TeamFields;
use super::history::TeamHistory;
use super::notes::TeamNotes;
use crate::data::use_storage;
use crate::side::details::address::AddressParam;
use crate::side::participant::team_url;
use crate::storage::{AddressData, TeamAccess, TeamData, TeamStatus};
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
    let address = AddressParam::new(&AddressData::empty());
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
            Pill { tone: PillTone::Red, "Canceled" }
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
                CanceledNotice { cook_and_run_id, team: team.clone(), onclose }
            }

            Tabs {
                // The change log exists for teams of a cloud project only.
                tabs: if team.cloud.is_some() { vec!["Team Data", "Notes", "History"] } else { vec!["Team Data", "Notes"] },
                selected: tab(),
                onselect: move |index| tab.set(index),
                actions: header_actions,
            }

            if tab() == 0 {
                TeamFields { draft, address, show_all_errors: true, disabled: canceled }
                if team.cloud.is_some() && !canceled {
                    InvitationBlock {
                        cook_and_run_id,
                        team_id,
                        stored_mail: team.mail.clone(),
                        verified_at: team.cloud.as_ref().and_then(|cloud| cloud.email_verified_at),
                        // A new address has to be saved first: the invitation goes to the stored one.
                        mail_unsaved: current.mail.trim() != team.mail.clone().unwrap_or_default().trim(),
                    }
                }
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
            } else if tab() == 1 {
                TeamNotes { cook_and_run_id, team_id, notes: team.note_list.clone() }
            } else {
                TeamHistory { cook_and_run_id, team_id }
            }
        }
    }
}

/// The invitation mail of a team: whether the address is confirmed, and sending the mail again.
/// Sending is always possible (also when the address is confirmed: the mail then only carries the
/// team link). While it is not confirmed every mail has a new confirmation link, the old one
/// stops working.
#[component]
fn InvitationBlock(
    cook_and_run_id: Uuid,
    team_id: Uuid,
    stored_mail: Option<String>,
    verified_at: Option<chrono::NaiveDateTime>,
    mail_unsaved: bool,
) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let send_task = use_busy();
    let mut confirm = use_signal(|| false);

    let has_mail = stored_mail.is_some();
    let blocked = !has_mail || mail_unsaved;
    let (tone, state) = match (&stored_mail, verified_at) {
        (None, _) => (PillTone::Neutral, "No email address".to_string()),
        (Some(_), Some(at)) => (PillTone::Green, format!("Confirmed on {}", format_local(at))),
        (Some(_), None) => (PillTone::Amber, "Not confirmed yet".to_string()),
    };
    let explanation = if !has_mail {
        "Enter an email address and save the team to be able to send the invitation."
    } else if mail_unsaved {
        "Save the new email address first: the invitation goes to the stored address."
    } else if verified_at.is_some() {
        "The team confirmed its address. The invitation can still be sent again; it then only contains the team link."
    } else {
        "The team has not confirmed its address yet. Sending the invitation again creates a new confirmation link and invalidates the previous one."
    };
    let button = if verified_at.is_some() { "Send invitation again" } else { "Send invitation again (new link)" };

    let on_send = move |_| {
        send_task.run(async move {
            let result = {
                storage
                    .read()
                    .await
                    .resend_team_verification(cook_and_run_id, team_id, &TeamAccess::Admin)
                    .await
            };
            match result {
                Ok(()) => {
                    confirm.set(false);
                    toasts.success("Invitation sent", "The email is queued and goes out shortly.");
                    // The history shows the new entry.
                    storage.invalidate();
                }
                Err(e) => toasts.error("Sending the invitation failed", e),
            }
        });
    };

    rsx! {
        Panel { class: "space-y-2.5",
            div { class: "flex flex-wrap items-center gap-2",
                Eyebrow { text: "Invitation email" }
                Pill { tone, "{state}" }
            }
            Hint { "{explanation}" }
            if confirm() {
                div { class: "flex flex-wrap items-center gap-3",
                    span { class: "text-xs text-zinc-700", "Send the invitation to {stored_mail.clone().unwrap_or_default()}?" }
                    Button { label: "Send", loading: send_task.is_busy(), onclick: on_send }
                    Button {
                        label: "Cancel",
                        variant: ButtonVariant::Secondary,
                        disabled: send_task.is_busy(),
                        onclick: move |_| confirm.set(false),
                    }
                }
            } else {
                div {
                    Button {
                        label: button,
                        variant: ButtonVariant::Secondary,
                        disabled: blocked,
                        onclick: move |_| confirm.set(true),
                    }
                }
            }
        }
    }
}

/// A team that canceled: everything is read-only and the organiser decides what happens next.
/// The diets, member count and address are already deleted; confirming the cancellation deletes
/// the rest (the team with its contact data and history).
#[component]
fn CanceledNotice(cook_and_run_id: Uuid, team: TeamData, onclose: EventHandler<()>) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let delete_task = use_busy();
    let mut confirm = use_signal(|| false);

    let cloud = team.cloud.as_ref();
    let when = cloud.and_then(|c| c.canceled_at).map(format_local);
    let reason = cloud.and_then(|c| c.cancel_reason.clone()).filter(|r| !r.trim().is_empty());
    let team_id = team.id;

    let on_confirm = move |_| {
        delete_task.run(async move {
            let result = { storage.write().await.delete_team_of_cook_and_run(cook_and_run_id, team_id).await };
            match result {
                Ok(()) => {
                    storage.invalidate();
                    toasts.success("Cancellation confirmed", "The team and its remaining data are deleted.");
                    onclose.call(());
                }
                Err(e) => toasts.error("Confirming the cancellation failed", e),
            }
        });
    };

    rsx! {
        Notice { tone: NoticeTone::Danger, roomy: true, class: "space-y-2", title: "This team canceled its participation and can no longer be edited.",
            if let Some(when) = when {
                p { class: "text-xs", "Canceled on {when}" }
            }
            if let Some(reason) = reason {
                p { class: "text-xs whitespace-pre-wrap", "Reason: {reason}" }
            }
            p { class: "text-xs",
                "Address, dietary requirements and number of members are already deleted. Name, email and phone are kept until you confirm the cancellation: that deletes the team for good."
            }
            if confirm() {
                div { class: "flex flex-wrap items-center gap-3",
                    span { class: "text-xs font-medium text-red-700", "Delete this team for good?" }
                    Button {
                        label: "Confirm and delete",
                        variant: ButtonVariant::Danger,
                        loading: delete_task.is_busy(),
                        onclick: on_confirm,
                    }
                    Button {
                        label: "Back",
                        variant: ButtonVariant::Secondary,
                        disabled: delete_task.is_busy(),
                        onclick: move |_| confirm.set(false),
                    }
                }
            } else {
                div {
                    Button {
                        label: "Confirm cancellation",
                        variant: ButtonVariant::DangerOutline,
                        onclick: move |_| confirm.set(true),
                    }
                }
            }
        }
    }
}
