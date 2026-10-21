//! Teams of a project: grid, add/edit dialogs, notes and the share dialog.
//!
//! | file         | contents                                              |
//! |--------------|-------------------------------------------------------|
//! | `mod.rs`     | page, team grid, which dialog is open                 |
//! | `draft.rs`   | form model + validation (pure, tested)                |
//! | `filter.rs`  | search and sort of the list (pure, tested)            |
//! | `form.rs`    | `TeamFields` – the form shared by add and edit        |
//! | `dialogs.rs` | `AddTeamDialog`, `EditTeamDialog`                     |
//! | `notes.rs`   | notes tab                                             |
//! | `share.rs`   | share dialog (link, QR code, registration settings)   |

mod dialogs;
mod draft;
mod filter;
mod form;
mod notes;
mod share;

use dioxus::prelude::*;
use uuid::Uuid;

use crate::data::use_storage;
use crate::storage::{ShareTeamConfig, TeamMetaData, TeamStatus, MAX_TEAMS_PER_PROJECT};
use crate::time::{format_date, format_local};
use crate::ui::*;
use dialogs::{AddTeamDialog, EditTeamDialog};
use filter::{visible_teams, TeamSort};
use share::ShareDialog;

/// Everything the page needs, loaded in one go. A failing share configuration must not hide the teams.
///
/// The list uses the *short form* of the teams (one request per 100 teams in the cloud). Full team
/// data is loaded per team when its dialog opens: loading all teams in full costs one request each
/// and exceeded the rate limit.
#[derive(Clone, PartialEq)]
struct TeamsData {
    teams: Vec<TeamMetaData>,
    is_cloud: bool,
    share_config: Result<Option<ShareTeamConfig>, String>,
}

#[component]
pub fn Teams(cook_and_run_id: ReadSignal<Uuid>) -> Element {
    let storage = use_storage();
    let data = use_resource(move || {
        let id = cook_and_run_id();
        async move {
            let _ = storage.revision();
            let storage = storage.read().await;
            let teams = storage.select_cook_and_run_team_meta_list(id).await?;
            let is_cloud = storage.select_cook_and_run_meta(id).await?.is_in_cloud;
            let share_config = if is_cloud {
                storage.select_cook_and_run_share_config(id).await
            } else {
                Ok(None)
            };
            Ok::<_, String>(TeamsData { teams, is_cloud, share_config })
        }
    });

    let id = cook_and_run_id();
    render_resource(
        data,
        "Could not load project. You may need to log in or the servers may be offline.",
        move |data| rsx! { TeamsContent { key: "{id}", cook_and_run_id: id, data } },
    )
}

/// Which dialog is open. Edit holds only the id: the dialog always gets the
/// current team from the list, never a stale copy.
#[derive(Clone, Copy, PartialEq)]
enum Open {
    Nothing,
    Add,
    Edit(Uuid),
    Share,
}

#[component]
fn TeamsContent(cook_and_run_id: Uuid, data: TeamsData) -> Element {
    let mut open = use_signal(|| Open::Nothing);
    let mut search = use_signal(String::new);
    let mut sort = use_signal(TeamSort::default);
    let close = move |_| open.set(Open::Nothing);

    let team_count = data.teams.len();
    let can_add = team_count < MAX_TEAMS_PER_PROJECT;
    let subtitle = format!("{team_count} {}", if team_count == 1 { "team" } else { "teams" });
    let query = search.read().trim().to_string();
    let shown = visible_teams(&data.teams, &query, sort());

    let share_button = match (&data.is_cloud, &data.share_config) {
        (false, _) => rsx! {
            span { title: "Sharing is only available when the project is stored in the cloud.",
                Button { label: "Share", variant: ButtonVariant::Secondary, disabled: true, onclick: move |_| {} }
            }
        },
        (true, Err(e)) => rsx! {
            span { title: "The share configuration could not be loaded: {e}",
                Button { label: "Share", variant: ButtonVariant::Secondary, disabled: true, onclick: move |_| {} }
            }
        },
        (true, Ok(_)) => rsx! {
            Button { label: "Share", onclick: move |_| open.set(Open::Share) }
        },
    };

    rsx! {
        PageShell {
            PageHeader { title: "Teams", subtitle, actions: share_button }

            if data.teams.is_empty() {
                EmptyState {
                    title: "No teams yet",
                    text: "Add the teams that take part, or share a link so they can register themselves.",
                    Button { label: "Add team", onclick: move |_| open.set(Open::Add) }
                }
            } else {
                Toolbar {
                    SearchField {
                        value: search(),
                        placeholder: "Search teams…",
                        oninput: move |e: Event<FormData>| search.set(e.value()),
                    }
                    SegmentedTabs {
                        label: "Sort:",
                        options: TeamSort::ALL.iter().map(|(_, label)| *label).collect::<Vec<_>>(),
                        selected: sort().index(),
                        onselect: move |index: usize| sort.set(TeamSort::ALL[index].0),
                    }
                }

                if shown.is_empty() {
                    div { class: "text-center py-8 bg-amber-50/40 rounded-2xl border border-amber-100/60",
                        p { class: "text-sm text-zinc-500 italic", "No teams match '{query}'." }
                    }
                } else {
                    div { class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4",
                        for team in shown {
                            TeamCard {
                                key: "{team.id}",
                                team: team.clone(),
                                show_verification: data.is_cloud,
                                onopen: {
                                    let team_id = team.id;
                                    move |_| open.set(Open::Edit(team_id))
                                },
                            }
                        }
                        if can_add {
                            AddCardButton { label: "Add team", onclick: move |_| open.set(Open::Add) }
                        }
                    }
                }
                if !can_add {
                    p { class: "text-xs text-zinc-400",
                        "A project can have at most {MAX_TEAMS_PER_PROJECT} teams."
                    }
                }
            }
        }

        match open() {
            Open::Nothing => rsx! {},
            Open::Add => rsx! { AddTeamDialog { cook_and_run_id, onclose: close } },
            // Only the id: the dialog loads the full team itself.
            Open::Edit(team_id) => rsx! {
                EditTeamDialog { key: "{team_id}", cook_and_run_id, team_id, onclose: close }
            },
            Open::Share => rsx! {
                ShareDialog {
                    cook_and_run_id,
                    config: data.share_config.clone().ok().flatten(),
                    team_count,
                    onclose: close,
                }
            },
        }
    }
}

// ─────────────────────────────────────────────
//  Team card
// ─────────────────────────────────────────────

/// How a piece of contact information is stored; decides icon color and tooltip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Presence {
    Missing,
    Present,
    /// Present *and* confirmed by the team (email, cloud teams only).
    Verified,
    /// Present, but not confirmed yet (email, cloud teams only).
    Unverified,
}

impl Presence {
    /// Email: verification only exists for cloud teams; a local team can never be "unverified".
    fn email(team: &TeamMetaData, is_cloud: bool) -> Self {
        match (team.mail, is_cloud, team.email_verified) {
            (false, _, _) => Self::Missing,
            (true, false, _) => Self::Present,
            (true, true, true) => Self::Verified,
            (true, true, false) => Self::Unverified,
        }
    }

    fn of(present: bool) -> Self {
        if present { Self::Present } else { Self::Missing }
    }
}

const MAIL_PATH: &str = "M21.75 6.75v10.5a2.25 2.25 0 01-2.25 2.25h-15a2.25 2.25 0 01-2.25-2.25V6.75m19.5 0A2.25 2.25 0 0019.5 4.5h-15a2.25 2.25 0 00-2.25 2.25m19.5 0v.243a2.25 2.25 0 01-1.07 1.916l-7.5 4.615a2.25 2.25 0 01-2.36 0L3.32 8.91a2.25 2.25 0 01-1.07-1.916V6.75";
const PHONE_PATH: &str = "M2.25 6.75c0 8.284 6.716 15 15 15h2.25a2.25 2.25 0 002.25-2.25v-1.372c0-.516-.351-.966-.852-1.091l-4.423-1.106c-.44-.11-.902.055-1.173.417l-.97 1.293c-.282.376-.769.542-1.21.38a12.035 12.035 0 01-7.143-7.143c-.162-.441.004-.928.38-1.21l1.293-.97c.363-.271.527-.734.417-1.173L6.963 3.102a1.125 1.125 0 00-1.091-.852H4.5A2.25 2.25 0 002.25 4.5v2.25z";
const DIETS_PATH: &str = "M11 20A7 7 0 019.8 6.1C15.5 5 17 4.48 19 2c1 2 2 4.18 2 8 0 5.5-4.78 10-10 10zM2 21c0-3 1.85-5.36 5.08-6C9.5 14.52 12 13 13 12";

/// One icon of the contact row. All three icons are always shown, so cards line up and a missing
/// piece of information is visible at a glance (dashed, pale) instead of simply absent.
#[component]
fn MetaIcon(path: &'static str, presence: Presence, #[props(into)] label: String) -> Element {
    let (frame, tone) = match presence {
        Presence::Missing => ("border-dashed border-zinc-200", "text-zinc-300"),
        Presence::Present => ("border-amber-200 bg-amber-50", "text-amber-700"),
        Presence::Verified => ("border-emerald-200 bg-emerald-50", "text-emerald-600"),
        Presence::Unverified => ("border-amber-300 bg-amber-50", "text-amber-700"),
    };

    rsx! {
        span {
            role: "img",
            title: "{label}",
            "aria-label": "{label}",
            class: "relative inline-flex items-center justify-center w-7 h-7 rounded-lg border {frame} {tone}",
            svg {
                class: "w-4 h-4",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                view_box: "0 0 24 24",
                path { d: path }
            }
            match presence {
                // Small badge in the corner: green check = confirmed, amber dot = still to confirm.
                Presence::Verified => rsx! {
                    span { class: "absolute -right-1 -bottom-1 w-3.5 h-3.5 rounded-full bg-emerald-500 ring-2 ring-white flex items-center justify-center",
                        svg { class: "w-2.5 h-2.5 text-white", fill: "none", stroke: "currentColor", stroke_width: "4", view_box: "0 0 24 24",
                            path { d: "M5 13l4 4L19 7", stroke_linecap: "round", stroke_linejoin: "round" }
                        }
                    }
                },
                Presence::Unverified => rsx! {
                    span { class: "absolute -right-0.5 -top-0.5 w-2.5 h-2.5 rounded-full bg-amber-500 ring-2 ring-white" }
                },
                _ => rsx! {},
            }
        }
    }
}

#[component]
fn TeamCard(team: TeamMetaData, show_verification: bool, onopen: EventHandler<()>) -> Element {
    let (border, header, accent) = match team.status {
        TeamStatus::Review => ("border-amber-400", "bg-amber-100/80 border-amber-300", "bg-amber-500"),
        TeamStatus::Canceled => ("border-red-200 opacity-70", "bg-red-50/70 border-red-200", "bg-red-400/70"),
        TeamStatus::Active => ("border-amber-100", "bg-amber-50/70 border-amber-100", "bg-amber-400/70"),
    };

    let email = Presence::email(&team, show_verification);
    let email_label = match email {
        Presence::Missing => "No email address",
        Presence::Present => "Email address stored",
        Presence::Verified => "Email address stored and confirmed",
        Presence::Unverified => "Email address stored, not confirmed yet",
    };
    let phone = Presence::of(team.phone);
    let diets = Presence::of(team.diets);

    rsx! {
        button {
            r#type: "button",
            class: "w-full text-left relative bg-white rounded-2xl border {border} shadow-sm \
                    hover:shadow-md transition-all duration-150 cursor-pointer overflow-hidden \
                    focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400",
            onclick: move |_| onopen.call(()),

            div { class: "px-4 py-2.5 border-b flex items-center gap-2 {header}",
                div { class: "w-1.5 h-4 rounded-full shrink-0 {accent}" }
                span { class: "text-sm font-semibold text-zinc-800 truncate", "{team.name}" }
                match team.status {
                    TeamStatus::Review => rsx! {
                        span { class: "ml-auto text-[10px] font-bold px-1.5 py-0.5 rounded-full bg-amber-500 text-white tracking-wide shrink-0",
                            "REVIEW"
                        }
                    },
                    TeamStatus::Canceled => rsx! {
                        span { class: "ml-auto text-[10px] font-bold px-1.5 py-0.5 rounded-full bg-red-400 text-white tracking-wide shrink-0",
                            "CANCELED"
                        }
                    },
                    TeamStatus::Active => rsx! {},
                }
            }

            div { class: "px-4 py-3 space-y-2.5",
                div { class: "flex items-center gap-2",
                    MetaIcon { path: MAIL_PATH, presence: email, label: email_label }
                    MetaIcon {
                        path: PHONE_PATH,
                        presence: phone,
                        label: if team.phone { "Phone number stored" } else { "No phone number" },
                    }
                    MetaIcon {
                        path: DIETS_PATH,
                        presence: diets,
                        label: if team.diets { "Dietary requirements stored" } else { "No dietary requirements" },
                    }
                    if let Some(members) = team.members {
                        span { class: "ml-auto text-xs font-medium text-zinc-600",
                            "{members} {members_word(members)}"
                        }
                    }
                }
                div { class: "flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px] text-zinc-400",
                    span { title: "{format_local(team.created)}", "Created {format_date(team.created)}" }
                    span { title: "{format_local(team.edited)}", "Edited {format_date(team.edited)}" }
                }
            }
        }
    }
}

fn members_word(count: u16) -> &'static str {
    if count == 1 { "member" } else { "members" }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use uuid::Uuid;

    use super::*;

    fn meta(mail: bool, verified: bool) -> TeamMetaData {
        let day = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap().and_hms_opt(12, 0, 0).unwrap();
        TeamMetaData {
            id: Uuid::nil(),
            name: "Team".into(),
            mail,
            phone: false,
            members: None,
            diets: false,
            created_by_user: None,
            created: day,
            edited: day,
            status: TeamStatus::Active,
            canceled_at: None,
            email_verified: verified,
        }
    }

    #[test]
    fn no_mail_is_missing_regardless_of_verification() {
        assert_eq!(Presence::email(&meta(false, false), true), Presence::Missing);
        assert_eq!(Presence::email(&meta(false, true), false), Presence::Missing);
    }

    #[test]
    fn cloud_teams_distinguish_verified_and_unverified() {
        assert_eq!(Presence::email(&meta(true, true), true), Presence::Verified);
        assert_eq!(Presence::email(&meta(true, false), true), Presence::Unverified);
    }

    #[test]
    fn local_teams_are_never_unverified() {
        // Verification only exists in the cloud, a local team would otherwise always look "unconfirmed".
        assert_eq!(Presence::email(&meta(true, false), false), Presence::Present);
        assert_eq!(Presence::email(&meta(true, true), false), Presence::Present);
    }

    #[test]
    fn member_count_is_pluralized() {
        assert_eq!(members_word(1), "member");
        assert_eq!(members_word(0), "members");
        assert_eq!(members_word(4), "members");
    }
}
