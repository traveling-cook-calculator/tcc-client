//! "Account & data": the organizer's data rights (Art. 15, 17, 20 GDPR).
//!
//! * see who you are logged in as and change that data at the login provider (Keycloak),
//! * download everything the API stores about the account (`GET /me/export`),
//! * delete all cloud projects (`DELETE /me?confirm=true`) **and** the login account itself
//!   (Keycloak's "delete account" action), optionally together with the data in this browser,
//! * delete only the data in this browser (works without login).
//!
//! The order of the deletion matters: first the API data (it needs the login), then the browser
//! data, last the account at the auth server (it ends the session).

use dioxus::prelude::*;

use crate::config::Config;
use crate::data::use_storage;
use crate::keycloak::AuthState;
use crate::ui::*;
use crate::Route;

#[component]
pub fn Account() -> Element {
    let auth = use_context::<Signal<Option<AuthState>>>();
    let current = auth.read().clone();

    rsx! {
        PageShell {
            PageHeader {
                title: "Account & data",
                subtitle: "Download or delete what is stored about you.".to_string(),
            }
            match &current {
                None | Some(AuthState::Loading(..)) => rsx! { LoadingView {} },
                Some(state @ AuthState::LoggedIn(..)) => rsx! {
                    ProfileCard {
                        username: state.user().and_then(|user| user.preferred_username.clone()),
                        email: state.user().and_then(|user| user.email.clone()),
                        console_url: state.account_console_url(),
                    }
                    CloudDataCard {}
                    DeleteAccountCard {}
                    LocalDataCard {}
                },
                Some(_) => rsx! {
                    Card { title: "Not logged in",
                        p { class: "text-sm text-zinc-600 leading-relaxed",
                            "Log in to download or delete your cloud data and your account. "
                            "The data in this browser can be deleted without logging in."
                        }
                    }
                    LocalDataCard {}
                },
            }
        }
    }
}

/// What the login server says about the user, and the way to change it. The app keeps no copy
/// of name or email: the data lives at Keycloak and is changed there.
#[component]
fn ProfileCard(username: Option<String>, email: Option<String>, console_url: Option<String>) -> Element {
    let name = username.unwrap_or_else(|| "-".to_string());
    let email = email.unwrap_or_else(|| "-".to_string());

    rsx! {
        Card { title: "Your profile",
            dl { class: "grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-3 text-sm",
                div {
                    dt { class: EYEBROW, "Username" }
                    dd { class: "text-zinc-800 break-words", "{name}" }
                }
                div {
                    dt { class: EYEBROW, "Email" }
                    dd { class: "text-zinc-800 break-words", "{email}" }
                }
            }
            p { class: "text-sm text-zinc-600 leading-relaxed",
                "Name, email address, password and active sessions are managed at the login provider. "
                "Changes show up here the next time you log in."
            }
            if let Some(url) = console_url {
                div {
                    a {
                        href: "{url}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: button_class(ButtonVariant::Secondary),
                        "Edit profile at the login provider"
                    }
                }
            }
        }
    }
}

/// File name of the export, e.g. `tcc-export.json`.
const EXPORT_FILE: &str = "tcc-export.json";

#[component]
fn CloudDataCard() -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let task = use_busy();

    let on_download = move |_| {
        task.run(async move {
            let result = { storage.read().await.export_account_data().await };
            match result {
                Ok(json) => match crate::web::download_text(EXPORT_FILE, &json) {
                    Ok(()) => toasts.success("Export ready", "The file contains all projects with their teams, notes and plans."),
                    Err(e) => toasts.error("Download failed", e),
                },
                Err(e) => toasts.error("Export failed", e),
            }
        });
    };

    rsx! {
        Card { title: "Your data in the cloud",
            p { class: "text-sm text-zinc-600 leading-relaxed",
                "One file with all your projects: settings, courses, teams with notes and change log, and the plan. "
                "Access tokens of teams are not part of it (they are credentials), and neither are uploaded design images."
            }
            div {
                Button { label: "Download my data", variant: ButtonVariant::Secondary, loading: task.is_busy(), onclick: on_download }
            }
        }
    }
}

#[component]
fn DeleteAccountCard() -> Element {
    let storage = use_storage();
    let auth = use_context::<Signal<Option<AuthState>>>();
    let toasts = use_toasts();
    let task = use_busy();
    let mut confirm = use_signal(|| false);
    let mut wipe_local = use_signal(|| true);
    let config = use_context::<Signal<Option<Config>>>();
    let project_days = config.read().as_ref().and_then(|config| config.legal.retention_project_days);

    let on_delete = move |_| {
        let state = auth.peek().clone();
        task.run(async move {
            // 1. The API data (needs the login).
            let erased = { storage.read().await.erase_account_data().await };
            let projects = match erased {
                Ok(projects) => projects,
                Err(e) => return toasts.error("Deleting your data failed", e),
            };

            // 2. The data in this browser, if wanted.
            if wipe_local() {
                crate::web::clear_local_data();
            }

            // 3. The login account at the auth server. It ends the session and sends the browser back.
            let Some(state) = state else { return };
            let (next, url) = state.request_account_deletion(Route::Dashboard {}.to_state()).await;
            let mut auth = auth;
            auth.set(Some(next));
            match url {
                Some(url) => {
                    crate::web::set_account_deletion_pending(true);
                    crate::web::reload_to(&url);
                }
                None => toasts.error(
                    "Account not deleted",
                    format!("{projects} project(s) were deleted, but the login account could not be removed. Delete it in your account settings at the login provider."),
                ),
            }
        });
    };

    rsx! {
        Card { title: "Delete my account", tone: Tone::Danger,
            p { class: "text-sm text-zinc-600 leading-relaxed",
                "Deletes all your cloud projects at once, with their teams, notes, plans, designs and uploaded images "
                "(the data of the participating teams goes with them), and then your login account. This cannot be undone."
            }
            if let Some(days) = project_days {
                p { class: "text-xs text-zinc-500 leading-relaxed",
                    "Without this, projects are deleted automatically {days} days after the event (or the last change). Download your data first if you want to keep it."
                }
            }
            Checkbox {
                label: "Also delete the data stored in this browser (local projects and designs)",
                checked: wipe_local(),
                onchange: move |checked| wipe_local.set(checked),
                disabled: task.is_busy(),
            }
            if confirm() {
                div { class: "flex flex-wrap gap-3",
                    Button { label: "Yes, delete everything", variant: ButtonVariant::Danger, loading: task.is_busy(), onclick: on_delete }
                    Button { label: "Back", variant: ButtonVariant::Secondary, disabled: task.is_busy(), onclick: move |_| confirm.set(false) }
                }
            } else {
                div {
                    Button { label: "Delete my account", variant: ButtonVariant::DangerOutline, onclick: move |_| confirm.set(true) }
                }
            }
        }
    }
}

#[component]
fn LocalDataCard() -> Element {
    let mut confirm = use_signal(|| false);

    let on_delete = move |_| {
        crate::web::clear_local_data();
        // Everything in memory goes with the reload.
        crate::web::reload_to("/");
    };

    rsx! {
        Card { title: "Data in this browser",
            p { class: "text-sm text-zinc-600 leading-relaxed",
                "Local projects, saved designs and your login session are stored in this browser only. "
                "Deleting them does not touch anything in the cloud. Your cookie choice is kept."
            }
            if confirm() {
                div { class: "flex flex-wrap gap-3",
                    Button { label: "Yes, delete local data", variant: ButtonVariant::Danger, onclick: on_delete }
                    Button { label: "Back", variant: ButtonVariant::Secondary, onclick: move |_| confirm.set(false) }
                }
            } else {
                div {
                    Button { label: "Delete local data", variant: ButtonVariant::DangerOutline, onclick: move |_| confirm.set(true) }
                }
            }
        }
    }
}
