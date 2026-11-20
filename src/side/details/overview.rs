//! Project overview: name, date, cloud/local transfer, export, delete.

use chrono::{NaiveDate, NaiveTime};
use dioxus::prelude::*;
use uuid::Uuid;

use crate::config::Config;
use crate::data::use_storage;
use crate::keycloak::AuthState;
use crate::storage::{CookAndRunMetaData, CookAndRunMetaUpdate};
use crate::ui::*;
use crate::web::download_text;
use crate::Route;

const DATE_FORMAT: &str = "%Y-%m-%d";

// ─────────────────────────────────────────────
//  Validation (single source of truth: the storage layer)
// ─────────────────────────────────────────────

fn update_for(name: &str, email: Option<&str>) -> CookAndRunMetaUpdate {
    CookAndRunMetaUpdate {
        name: name.trim().to_string(),
        occur: NaiveDate::default().and_time(NaiveTime::MIN),
        admin_notification_email: email.map(|e| e.trim().to_string()),
    }
}

fn name_error(name: &str) -> String {
    update_for(name, None).validate().err().unwrap_or_default()
}

/// An empty address is valid for updates ("keep the current one").
fn email_error(email: &str) -> String {
    if email.trim().is_empty() {
        return String::new();
    }
    update_for("Project", Some(email)).validate().err().unwrap_or_default()
}

// ─────────────────────────────────────────────
//  Root
// ─────────────────────────────────────────────

#[component]
pub fn Overview(cook_and_run_id: ReadSignal<Uuid>) -> Element {
    let storage = use_storage();
    let meta = use_resource(move || {
        let id = cook_and_run_id();
        async move {
            let _ = storage.revision();
            storage.read().await.select_cook_and_run_meta(id).await
        }
    });

    render_resource(
        meta,
        "Could not load project. You may need to log in or the servers may be offline.",
        |meta| rsx! { OverviewContent { key: "{meta.id}", meta } },
    )
}

// ─────────────────────────────────────────────
//  Content
// ─────────────────────────────────────────────

/// Hint for the organizer: when the data of the participating teams is deleted. Only for cloud
/// projects (teams exist there) and only if the period is configured; the API decides, this is
/// information.
fn retention_notice(is_cloud: bool, occur: NaiveDate, days: Option<u32>) -> Option<String> {
    let days = days.filter(|_| is_cloud)?;
    let date = occur.checked_add_signed(chrono::Duration::days(i64::from(days)))?;
    Some(format!(
        "The data of the participating teams is deleted automatically on {} ({days} days after the event).",
        date.format("%d.%m.%Y")
    ))
}

#[derive(Clone, PartialEq)]
struct Draft {
    name: String,
    occur: NaiveDate,
    email: String,
}

impl Draft {
    fn from_meta(meta: &CookAndRunMetaData) -> Self {
        Self {
            name: meta.name.clone(),
            occur: meta.occur.date(),
            email: meta.admin_notification_email.clone().unwrap_or_default(),
        }
    }
}

#[component]
fn OverviewContent(meta: CookAndRunMetaData) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let navigator = use_navigator();
    let auth = use_context::<Signal<Option<AuthState>>>();

    let flash = use_flash();
    let save_task = use_busy();
    let download_task = use_busy();
    let export_task = use_busy();

    let mut draft = use_signal(|| Draft::from_meta(&meta));
    let mut saved = use_signal(|| Draft::from_meta(&meta));
    let mut show_delete = use_signal(|| false);
    let mut show_upload = use_signal(|| false);

    let id = meta.id;
    let is_cloud = meta.is_in_cloud;

    // Everything below is derived from `draft` / `saved` / `auth` – no extra
    // error or "dirty" signals to keep in sync.
    let current = draft.read().clone();
    let name_err = name_error(&current.name);
    let email_err = if is_cloud { email_error(&current.email) } else { String::new() };
    let can_save = current != *saved.read() && name_err.is_empty() && email_err.is_empty();
    let logged_in = matches!(auth.read().as_ref(), Some(AuthState::LoggedIn(..)));
    let user_email = match auth.read().as_ref() {
        Some(AuthState::LoggedIn(_, _, session)) => session.user.email.clone().unwrap_or_default(),
        _ => String::new(),
    };

    let on_save = move |_| {
        let d = draft.peek().clone();
        save_task.run(async move {
            let email = (is_cloud && !d.email.trim().is_empty()).then_some(d.email.as_str());
            let mut update = update_for(&d.name, email);
            update.occur = d.occur.and_time(NaiveTime::MIN);

            let result = { storage.write().await.update_meta_of_cook_and_run(id, &update).await };
            match result {
                Ok(()) => {
                    saved.set(d);
                    flash.trigger();
                }
                Err(e) => toasts.error("Saving failed", e),
            }
        });
    };

    let on_download = move |_| {
        download_task.run(async move {
            let result = { storage.write().await.download_from_cloud(id).await };
            match result {
                Ok(new_id) => {
                    storage.invalidate();
                    toasts.success("Project downloaded", "A local copy was created.");
                    navigator.push(Route::Overview { cook_and_run_id: new_id });
                }
                Err(e) => toasts.error("Download failed", e),
            }
        });
    };

    let on_export = move |_| {
        export_task.run(async move {
            let result: Result<(), String> = async {
                let project = storage.read().await.select_cook_and_run(id).await?;
                let json = serde_json::to_string(&project)
                    .map_err(|e| format!("Could not serialize project: {e}"))?;
                download_text(&format!("{}.tcc", project.name), &json)
            }
            .await;
            if let Err(e) = result {
                toasts.error("Export failed", e);
            }
        });
    };

    let config = use_context::<Signal<Option<Config>>>();
    let retention_days = config.read().as_ref().and_then(|config| config.legal.retention_participant_data_days);

    rsx! {
        PageShell {
            PageHeader { title: "Overview" }

            Card { title: "Project Settings", flash: flash.is_active(),
                div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                    Field { label: "Project name", error: name_err.clone(),
                        TextInput {
                            value: current.name.clone(),
                            placeholder: "e.g. Summer Cook & Run 2025",
                            error: !name_err.is_empty(),
                            oninput: move |e: Event<FormData>| draft.write().name = e.value(),
                        }
                    }
                    Field { label: "Occurring",
                        TextInput {
                            kind: InputKind::Date,
                            value: current.occur.format(DATE_FORMAT).to_string(),
                            oninput: move |e: Event<FormData>| {
                                // An empty/partial value while typing keeps the last valid date.
                                if let Ok(date) = NaiveDate::parse_from_str(&e.value(), DATE_FORMAT) {
                                    draft.write().occur = date;
                                }
                            },
                        }
                    }
                }

                if is_cloud {
                    Field {
                        label: "Admin notification email",
                        error: email_err.clone(),
                        hint: "Receives notifications about this project. Leave empty to keep the current address."
                            .to_string(),
                        TextInput {
                            kind: InputKind::Email,
                            value: current.email.clone(),
                            placeholder: "admin@example.com",
                            error: !email_err.is_empty(),
                            oninput: move |e: Event<FormData>| draft.write().email = e.value(),
                        }
                    }
                }

                div { class: "flex flex-wrap items-center gap-3 pt-1",
                    Button {
                        label: "Save",
                        loading: save_task.is_busy(),
                        disabled: !can_save,
                        onclick: on_save,
                    }
                    if is_cloud {
                        Button {
                            label: "Download",
                            variant: ButtonVariant::Secondary,
                            loading: download_task.is_busy(),
                            disabled: !logged_in,
                            onclick: on_download,
                        }
                    } else {
                        Button {
                            label: "Upload",
                            variant: ButtonVariant::Secondary,
                            disabled: !logged_in,
                            onclick: move |_| show_upload.set(true),
                        }
                    }
                    Button {
                        label: "Export",
                        variant: ButtonVariant::Secondary,
                        loading: export_task.is_busy(),
                        onclick: on_export,
                    }
                    div { class: "ml-auto",
                        Button {
                            label: "Delete Project",
                            variant: ButtonVariant::Danger,
                            onclick: move |_| show_delete.set(true),
                        }
                    }
                }
                if !logged_in {
                    p { class: "text-xs text-zinc-400",
                        "You need to be logged in to upload or download projects."
                    }
                }
            }

            if let Some(notice) = retention_notice(is_cloud, current.occur, retention_days) {
                Card { title: "Data retention",
                    p { class: "text-sm text-zinc-600 leading-relaxed", "{notice}" }
                    p { class: "text-xs text-zinc-400", "Download your data under \"Account & data\" if you want to keep it." }
                }
            }

            if is_cloud {
                InfoCard {
                    title: "Cloud Project",
                    lines: vec![
                        "This project is stored in the cloud.",
                        "Your data is synced across devices and backed up automatically.",
                        "Offline work is not available. Use the Download button to create a local copy.",
                    ],
                }
            } else {
                InfoCard {
                    title: "Local Project",
                    lines: vec![
                        "This project is stored only on this device.",
                        "Upload it to the cloud to enable syncing, backups, and collaboration.",
                        "Cloud features require you to be logged in.",
                    ],
                }
            }
        }

        if show_delete() {
            DeleteProjectDialog { project_id: id, onclose: move |_| show_delete.set(false) }
        }
        if show_upload() {
            UploadDialog {
                project_id: id,
                default_email: user_email,
                onclose: move |_| show_upload.set(false),
            }
        }
    }
}

#[component]
fn InfoCard(title: &'static str, lines: Vec<&'static str>) -> Element {
    let last = lines.len().saturating_sub(1);
    rsx! {
        Card { title,
            div { class: "space-y-1.5",
                for (position, line) in lines.into_iter().enumerate() {
                    p {
                        key: "{line}",
                        class: if position == last {
                            "text-sm font-medium text-zinc-700"
                        } else {
                            "text-sm text-zinc-500"
                        },
                        "{line}"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Dialogs
// ─────────────────────────────────────────────

#[component]
fn DeleteProjectDialog(project_id: Uuid, onclose: EventHandler<()>) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let navigator = use_navigator();
    let task = use_busy();

    rsx! {
        Dialog { title: "Delete Project", onclose,
            p { class: "text-sm text-zinc-600 leading-relaxed",
                "Deleting this project will "
                span { class: "font-semibold text-red-600", "permanently" }
                " remove all data. This action "
                span { class: "font-semibold text-red-600", "cannot be undone." }
            }
            div { class: "flex justify-end gap-3",
                Button {
                    label: "Cancel",
                    variant: ButtonVariant::Secondary,
                    disabled: task.is_busy(),
                    onclick: move |_| onclose.call(()),
                }
                Button {
                    label: "Delete Project",
                    variant: ButtonVariant::Danger,
                    loading: task.is_busy(),
                    onclick: move |_| {
                        task.run(async move {
                            let result = { storage.write().await.delete_cook_and_run(project_id).await };
                            match result {
                                Ok(()) => {
                                    storage.invalidate();
                                    navigator.push(Route::Dashboard {});
                                }
                                Err(e) => toasts.error("Deleting failed", e),
                            }
                        });
                    },
                }
            }
        }
    }
}

/// Uploading needs an address that is notified about the project (required by the backend).
#[component]
fn UploadDialog(project_id: Uuid, default_email: String, onclose: EventHandler<()>) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let navigator = use_navigator();
    let task = use_busy();
    let mut email = use_signal(move || default_email.clone());

    let value = email.read().clone();
    let error = if value.trim().is_empty() {
        "Please enter an email address.".to_string()
    } else {
        email_error(&value)
    };
    let can_upload = error.is_empty();

    rsx! {
        Dialog { title: "Upload to Cloud", onclose,
            p { class: "text-sm text-zinc-600 leading-relaxed",
                "The project is moved to the cloud. Enter the address that should receive notifications about it."
            }
            Field { label: "Admin notification email", error: if value.is_empty() { String::new() } else { error.clone() },
                TextInput {
                    kind: InputKind::Email,
                    value: value.clone(),
                    placeholder: "admin@example.com",
                    error: !value.is_empty() && !can_upload,
                    oninput: move |e: Event<FormData>| email.set(e.value()),
                }
            }
            div { class: "flex justify-end gap-3",
                Button {
                    label: "Cancel",
                    variant: ButtonVariant::Secondary,
                    disabled: task.is_busy(),
                    onclick: move |_| onclose.call(()),
                }
                Button {
                    label: "Upload",
                    loading: task.is_busy(),
                    disabled: !can_upload,
                    onclick: move |_| {
                        let address = email.peek().trim().to_string();
                        task.run(async move {
                            let result = { storage.write().await.upload_to_cloud(project_id, address).await };
                            match result {
                                Ok(new_id) => {
                                    storage.invalidate();
                                    toasts.success("Project uploaded", "It is now stored in the cloud.");
                                    navigator.push(Route::Overview { cook_and_run_id: new_id });
                                }
                                Err(e) => toasts.error("Upload failed", e),
                            }
                        });
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    #[test]
    fn the_notice_names_the_deletion_date() {
        let occur = NaiveDate::from_ymd_opt(2026, 6, 20).unwrap();
        let text = retention_notice(true, occur, Some(30)).unwrap();
        assert!(text.contains("20.07.2026"), "{text}");
    }

    #[test]
    fn there_is_no_notice_for_local_projects_or_without_a_configured_period() {
        let occur = NaiveDate::from_ymd_opt(2026, 6, 20).unwrap();
        assert!(retention_notice(false, occur, Some(30)).is_none());
        assert!(retention_notice(true, occur, None).is_none());
    }
}
