//! "Send route mails": every selected team gets a mail with its route (the plan as the team sees
//! it on its personal page). Cloud projects only; the API writes the mails to its outbox in one
//! transaction and sends them in the background.
//!
//! The mails wear the look of the project's design, and teams only open their plan once a design
//! is in use: without one nothing can be sent (the dialog says so and the button stays off).
//!
//! The API refuses the **whole** request if one team has no email address or has canceled, so
//! those teams are not offered. It answers 409 while the plan is outdated: the plan has to be
//! confirmed or saved first (the panel above the board offers both).

use std::collections::HashSet;

use dioxus::prelude::*;
use uuid::Uuid;

use super::super::design_notice::{use_design_in_use, NoDesignNotice};
use crate::data::use_storage;
use crate::storage::{TeamMetaData, TeamStatus};
use crate::ui::*;

/// At most this many teams per request of the API.
const MAX_PER_REQUEST: usize = 128;

/// Teams that can get a mail (taking part, with an email address) and the names of those that
/// cannot (no address).
pub(super) fn split_recipients(teams: &[TeamMetaData]) -> (Vec<(Uuid, String)>, Vec<String>) {
    let mut with_mail = Vec::new();
    let mut without_mail = Vec::new();
    for team in teams.iter().filter(|team| team.status != TeamStatus::Canceled) {
        if team.mail {
            with_mail.push((team.id, team.name.clone()));
        } else {
            without_mail.push(team.name.clone());
        }
    }
    (with_mail, without_mail)
}

/// The ids in portions the API accepts.
fn portions(ids: &[Uuid]) -> Vec<&[Uuid]> {
    ids.chunks(MAX_PER_REQUEST).collect()
}

#[component]
pub(super) fn RouteMailsDialog(
    cook_and_run_id: Uuid,
    teams: Vec<TeamMetaData>,
    /// The plan is marked outdated (nothing can be sent until it is confirmed or saved).
    outdated: bool,
    /// There are changes on the board that are not saved: the teams would get the stored plan.
    unsaved: bool,
    onclose: EventHandler<()>,
) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let task = use_busy();
    // Whether a design is in use for the teams. While it is not known (loading, or the list could
    // not be read) the button stays off; the server decides in the end.
    let design_in_use = use_design_in_use(cook_and_run_id, true);
    let (recipients, without_mail) = split_recipients(&teams);
    let mut selected = use_signal({
        let all: HashSet<Uuid> = recipients.iter().map(|(id, _)| *id).collect();
        move || all
    });

    let chosen: Vec<Uuid> = recipients.iter().map(|(id, _)| *id).filter(|id| selected.read().contains(id)).collect();
    let count = chosen.len();
    let blocked = outdated || unsaved || count == 0 || design_in_use != Some(true);

    let on_send = {
        let chosen = chosen.clone();
        move |_| {
            let chosen = chosen.clone();
            task.run(async move {
                for part in portions(&chosen) {
                    let result = { storage.write().await.send_route_mails_of_cook_and_run(cook_and_run_id, part).await };
                    if let Err(e) = result {
                        return toasts.error("Route mails were not sent", e);
                    }
                }
                toasts.success("Route mails queued", format!("{} team(s) get their route by email.", chosen.len()));
                onclose.call(());
            });
        }
    };

    rsx! {
        Dialog { title: "Send route mails", onclose: move |_| onclose.call(()),
            p { class: "text-sm text-zinc-600 leading-relaxed",
                "Each selected team gets an email with a link to its personal page, where it finds its route. "
                "Send them when the plan is final; sending again later is possible."
            }
            NoDesignNotice { cook_and_run_id, in_use: design_in_use }
            if outdated {
                Notice { tone: NoticeTone::Attention,
                    "The plan is outdated. Confirm it (\"The plan is still fine\") or save the changed plan first."
                }
            }
            if unsaved {
                Notice { tone: NoticeTone::Attention,
                    "There are unsaved changes. The teams would get the stored plan: save or discard the changes first."
                }
            }
            if recipients.is_empty() {
                p { class: "text-sm text-zinc-500", "No team with an email address takes part." }
            } else {
                div { class: "flex flex-wrap gap-2 text-xs",
                    TextButton {
                        label: "Select all",
                        onclick: {
                            let all: HashSet<Uuid> = recipients.iter().map(|(id, _)| *id).collect();
                            move |_| selected.set(all.clone())
                        },
                    }
                    TextButton { label: "Select none", onclick: move |_| selected.set(HashSet::new()) }
                }
                div { class: "max-h-64 overflow-y-auto space-y-1.5 pr-1",
                    for (id, name) in recipients.iter().cloned() {
                        Checkbox {
                            key: "{id}",
                            label: name,
                            checked: selected.read().contains(&id),
                            onchange: move |checked| {
                                let mut set = selected.write();
                                if checked { set.insert(id); } else { set.remove(&id); }
                            },
                        }
                    }
                }
            }
            if !without_mail.is_empty() {
                p { class: "text-xs text-zinc-500",
                    "No email address, so no mail: {without_mail.join(\", \")}"
                }
            }
            div { class: "flex flex-wrap justify-end gap-2 pt-1",
                Button { label: "Close", variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()) }
                Button {
                    label: format!("Send to {count} team(s)"),
                    loading: task.is_busy(),
                    disabled: blocked,
                    onclick: on_send,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn team(id: u128, name: &str, mail: bool, status: TeamStatus) -> TeamMetaData {
        TeamMetaData {
            id: Uuid::from_u128(id),
            name: name.to_string(),
            mail,
            phone: false,
            members: None,
            diets: false,
            created_by_user: None,
            created: NaiveDateTime::default(),
            edited: NaiveDateTime::default(),
            status,
            canceled_at: None,
            email_verified: false,
        }
    }

    #[test]
    fn only_teams_that_take_part_and_have_a_mail_address_get_a_mail() {
        let teams = vec![
            team(1, "A", true, TeamStatus::Active),
            team(2, "B", false, TeamStatus::Active),
            team(3, "C", true, TeamStatus::Canceled),
        ];
        let (with, without) = split_recipients(&teams);
        assert_eq!(with, vec![(Uuid::from_u128(1), "A".to_string())]);
        assert_eq!(without, vec!["B".to_string()]);
    }

    #[test]
    fn large_projects_are_sent_in_portions_of_the_api_limit() {
        let ids: Vec<Uuid> = (1..=300).map(Uuid::from_u128).collect();
        let sizes: Vec<usize> = portions(&ids).iter().map(|part| part.len()).collect();
        assert_eq!(sizes, vec![128, 128, 44]);
    }
}
