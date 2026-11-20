//! Project list: search, sort, create, import.

use chrono::{DateTime, Local, NaiveDateTime};
use dioxus::prelude::*;
use uuid::Uuid;

use crate::data::use_storage;
use crate::keycloak::AuthState;
use crate::storage::{CookAndRunCreate, CookAndRunData, CookAndRunMetaData};
use crate::time::{format_date, to_local};
use crate::ui::*;
use crate::Route;

// ─────────────────────────────────────────────
//  Date/time helpers
// ─────────────────────────────────────────────
//  Storage timestamps are naive UTC; `crate::time` converts them to the viewer's local time. `now` is captured once per page visit
//  (`use_hook`) and passed in – reading the clock inside the render body
//  would make every render produce different output.

fn relative_time(now: DateTime<Local>, then: DateTime<Local>) -> String {
    let diff = now.signed_duration_since(then);
    if diff.num_seconds() < 60 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{}d ago", diff.num_days())
    } else if diff.num_weeks() < 5 {
        format!("{}w ago", diff.num_weeks())
    } else {
        then.format("%d.%m.%Y").to_string()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum OccurStatus {
    Past,
    Today,
    Upcoming,
}

impl OccurStatus {
    fn of(now: DateTime<Local>, occur_utc: NaiveDateTime) -> Self {
        let occur = to_local(occur_utc).date_naive();
        let today = now.date_naive();
        if occur < today {
            Self::Past
        } else if occur == today {
            Self::Today
        } else {
            Self::Upcoming
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Past => "Past",
            Self::Today => "Today",
            Self::Upcoming => "Upcoming",
        }
    }

    fn tone(self) -> PillTone {
        match self {
            Self::Past => PillTone::Neutral,
            Self::Today => PillTone::Green,
            Self::Upcoming => PillTone::Amber,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SortOption {
    Recent,
    Name,
    Created,
}

impl SortOption {
    const ALL: [(SortOption, &'static str); 3] = [
        (SortOption::Recent, "Recent"),
        (SortOption::Name, "Name"),
        (SortOption::Created, "Created"),
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|(option, _)| *option == self).unwrap_or(0)
    }
}

// ─────────────────────────────────────────────
//  Page
// ─────────────────────────────────────────────

#[component]
pub fn Dashboard() -> Element {
    let storage = use_storage();
    let mut projects = use_resource(move || async move {
        let _ = storage.revision();
        storage.read().await.select_cook_and_run_meta_list().await
    });
    let mut show_create = use_signal(|| false);

    let count = match &*projects.read_unchecked() {
        Some(Ok(list)) => Some(list.len()),
        _ => None,
    };
    let subtitle = count.map(|n| format!("{n} {}", if n == 1 { "project" } else { "projects" }));

    rsx! {
        PageShell { class: "max-w-7xl mx-auto",
            PageHeader {
                title: "Projects",
                subtitle,
                actions: rsx! {
                    Button { label: "New Project", onclick: move |_| show_create.set(true) }
                },
            }

            match &*projects.read_unchecked() {
                None => rsx! {
                    div { class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4 sm:gap-5",
                        for slot in 0..3 {
                            LoadingCard { key: "{slot}" }
                        }
                    }
                },
                Some(Err(details)) => rsx! {
                    ErrorView {
                        error_text: "Failed to load the projects.",
                        error_details: details.clone(),
                        onretry: move |_| projects.restart(),
                    }
                },
                Some(Ok(list)) => rsx! {
                    DashboardContent { projects: list.clone(), on_create: move |_| show_create.set(true) }
                },
            }
        }

        if show_create() {
            CreateProjectDialog { onclose: move |_| show_create.set(false) }
        }
    }
}

// ─────────────────────────────────────────────
//  List with toolbar
// ─────────────────────────────────────────────

#[component]
fn DashboardContent(projects: Vec<CookAndRunMetaData>, on_create: EventHandler<()>) -> Element {
    let now = use_hook(Local::now);
    let mut search = use_signal(String::new);
    let mut sort = use_signal(|| SortOption::Recent);

    if projects.is_empty() {
        return rsx! {
            EmptyState {
                title: "No projects yet",
                text: "Create your first project to start planning courses, teams and routes.",
                icon: rsx! { FolderSVG {} },
                Button { label: "Create Project", onclick: move |_| on_create.call(()) }
            }
        };
    }

    let query = search.read().trim().to_lowercase();
    let mut items: Vec<&CookAndRunMetaData> = projects
        .iter()
        .filter(|p| query.is_empty() || p.name.to_lowercase().contains(&query))
        .collect();
    match sort() {
        SortOption::Name => items.sort_by_key(|p| p.name.to_lowercase()),
        SortOption::Created => items.sort_by(|a, b| b.created.cmp(&a.created)),
        SortOption::Recent => items.sort_by(|a, b| b.edited.cmp(&a.edited)),
    }

    rsx! {
        Toolbar {
            SearchField {
                value: search(),
                placeholder: "Search projects…",
                oninput: move |e: Event<FormData>| search.set(e.value()),
            }
            SegmentedTabs {
                label: "Sort:",
                options: SortOption::ALL.iter().map(|(_, label)| *label).collect::<Vec<_>>(),
                selected: sort().index(),
                onselect: move |index: usize| sort.set(SortOption::ALL[index].0),
            }
        }

        if items.is_empty() {
            div { class: "text-center py-8 bg-amber-50/40 rounded-2xl border border-amber-100/60",
                p { class: "text-sm text-zinc-500 italic", "No projects match '{query}'." }
            }
        } else {
            div { class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4 sm:gap-5",
                for project in items {
                    ProjectCard { key: "{project.id}", project: project.clone(), now }
                }
                AddCardButton { label: "New Project", onclick: move |_| on_create.call(()) }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Project card
// ─────────────────────────────────────────────

#[component]
fn ProjectCard(project: CookAndRunMetaData, now: DateTime<Local>) -> Element {
    let status = OccurStatus::of(now, project.occur);
    let edited = (project.edited != project.created).then(|| {
        let local = to_local(project.edited);
        (relative_time(now, local), local.format("%d.%m.%Y · %H:%M").to_string())
    });

    rsx! {
        Link {
            to: Route::Overview { cook_and_run_id: project.id },
            class: "group flex flex-col h-full min-h-[148px] bg-white rounded-2xl border border-amber-100 \
                    shadow-sm overflow-hidden hover:border-amber-300 hover:shadow-md hover:-translate-y-0.5 \
                    transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400",
            "aria-label": "Open project {project.name}",

            header { class: "px-5 py-3.5 bg-amber-50/70 border-b border-amber-100 flex items-center justify-between gap-2.5",
                div { class: "flex items-center gap-2.5 min-w-0",
                    div { class: "w-1.5 h-5 rounded-full bg-amber-400/70 shrink-0" }
                    h3 { class: "truncate text-sm font-semibold text-zinc-800 group-hover:text-amber-900 transition-colors",
                        "{project.name}"
                    }
                }
                Pill {
                    tone: if project.is_in_cloud { PillTone::Amber } else { PillTone::Neutral },
                    title: if project.is_in_cloud { "Saved to cloud" } else { "Stored locally only" },
                    if project.is_in_cloud { "Cloud" } else { "Local" }
                }
            }

            div { class: "px-5 py-4 space-y-2 flex-1",
                div { class: "flex items-center justify-between gap-2",
                    MetaRow {
                        path: "M10 2a8 8 0 100 16 8 8 0 000-16zm1 4a1 1 0 10-2 0v4a1 1 0 00.293.707l2.828 2.829a1 1 0 101.415-1.415L11 9.586V6z",
                        text: "Event {format_date(project.occur)}",
                    }
                    Pill { tone: status.tone(), "{status.label()}" }
                }
                MetaRow {
                    path: "M6 2a1 1 0 00-1 1v1H4a2 2 0 00-2 2v1h16V6a2 2 0 00-2-2h-1V3a1 1 0 10-2 0v1H7V3a1 1 0 00-1-1zM2 9v7a2 2 0 002 2h12a2 2 0 002-2V9H2z",
                    text: "Created {format_date(project.created)}",
                }
                if let Some((relative, absolute)) = edited {
                    MetaRow {
                        muted: true,
                        title: absolute,
                        path: "M17.414 2.586a2 2 0 010 2.828l-8.586 8.586a2 2 0 01-.879.515l-4 1a1 1 0 01-1.213-1.213l1-4a2 2 0 01.515-.879l8.586-8.586a2 2 0 012.828 0zM15 5l-1-1L6 12l-.5 2 .5.5 2-.5L15 5z",
                        text: "Edited {relative}",
                    }
                }
            }
        }
    }
}

#[component]
fn MetaRow(
    path: &'static str,
    #[props(into)] text: String,
    #[props(default)] muted: bool,
    #[props(default, into)] title: String,
) -> Element {
    let (text_tone, icon_tone) = if muted {
        ("text-zinc-400", "text-zinc-300")
    } else {
        ("font-medium text-zinc-500", "text-amber-600/70")
    };
    rsx! {
        div { class: "flex items-center gap-1.5 text-xs {text_tone}", title: "{title}",
            svg {
                class: "w-3.5 h-3.5 shrink-0 {icon_tone}",
                fill: "currentColor",
                view_box: "0 0 20 20",
                xmlns: "http://www.w3.org/2000/svg",
                path { d: path }
            }
            span { "{text}" }
        }
    }
}

#[component]
fn FolderSVG() -> Element {
    rsx! {
        svg {
            class: "w-8 h-8",
            fill: "currentColor",
            view_box: "0 0 20 20",
            xmlns: "http://www.w3.org/2000/svg",
            path { d: "M4.083 9h1.946c.089 0 .173.024.25.067l1.591 1.591c.114.114.267.184.43.184h3.4a.6.6 0 00.43-.184l1.591-1.591a.6.6 0 01.25-.067h1.946M4.083 9L2.145 5.5A2 2 0 013.87 3h12.26a2 2 0 011.725 2.5L15.917 9M4.083 9L2 15a2 2 0 002 2h12a2 2 0 002-2l-2.083-6" }
        }
    }
}

#[component]
fn LoadingCard() -> Element {
    rsx! {
        div { class: "bg-white rounded-2xl border border-amber-100 shadow-xs overflow-hidden h-full min-h-[148px]",
            div { class: "px-5 py-3.5 bg-amber-50/60 border-b border-amber-100/70 flex items-center justify-between",
                div { class: "h-4 w-28 bg-amber-200/60 rounded-full animate-pulse" }
                div { class: "h-5 w-14 bg-amber-100 rounded-full animate-pulse" }
            }
            div { class: "px-5 py-4 space-y-2.5",
                div { class: "h-3.5 w-32 bg-zinc-200/70 rounded-full animate-pulse" }
                div { class: "h-3 w-24 bg-zinc-100 rounded-full animate-pulse" }
            }
            span { class: "sr-only", "Loading…" }
        }
    }
}

// ─────────────────────────────────────────────
//  Create / import dialog
// ─────────────────────────────────────────────

/// Same rule as in the Overview: the backend needs an address that is notified about the project.
fn email_error(email: &str) -> String {
    if email.trim().is_empty() {
        return "Please enter an email address.".to_string();
    }
    CookAndRunCreate { name: "Project".to_string(), admin_notification_email: Some(email.trim().to_string()) }
        .validate()
        .err()
        .unwrap_or_default()
}

#[component]
fn CreateProjectDialog(onclose: EventHandler<()>) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let navigator = use_navigator();
    let auth = use_context::<Signal<Option<AuthState>>>();
    let create_task = use_busy();
    let import_task = use_busy();

    let mut name = use_signal(String::new);
    let mut touched = use_signal(|| false);

    // ── Cloud option ─────────────────────────────────────────────────────────
    // Offered when the user is logged in *and* the cloud connection is up. The box is ticked
    // automatically in that case; `cloud_choice` only remembers an explicit click.
    let connected = use_resource(move || async move {
        let _ = storage.revision();
        storage.is_cloud_connected().await
    });
    let user_email = match auth.read().as_ref() {
        Some(AuthState::LoggedIn(_, _, session)) => Some(session.user.email.clone().unwrap_or_default()),
        _ => None,
    };
    let logged_in = user_email.is_some();
    let cloud_ready = logged_in && matches!(&*connected.read_unchecked(), Some(true));
    let mut cloud_choice = use_signal(|| None::<bool>);
    let mut email = use_signal(move || user_email.clone().unwrap_or_default());
    let use_cloud = cloud_ready && cloud_choice().unwrap_or(true);
    let cloud_hint = if !logged_in {
        Some("Log in to store projects in the cloud. Until then the project is stored on this device.")
    } else if matches!(&*connected.read_unchecked(), Some(false)) {
        Some("The cloud connection is not available. The project is stored on this device.")
    } else {
        None
    };

    let name_text = name.read().trim().to_string();
    let name_validation = CookAndRunCreate::new(name_text.clone()).validate().err();
    let shown_error = if touched() { name_validation.clone().unwrap_or_default() } else { String::new() };
    let email_value = email.read().clone();
    let email_validation = if use_cloud { email_error(&email_value) } else { String::new() };
    let shown_email_error = if email_value.is_empty() { String::new() } else { email_validation.clone() };
    let can_create = name_validation.is_none() && email_validation.is_empty();

    // Shared by the button and the Enter key.
    let create = move || {
        let project = CookAndRunCreate {
            name: name.peek().trim().to_string(),
            admin_notification_email: use_cloud.then(|| email.peek().trim().to_string()),
        };
        create_task.run(async move {
            let id = Uuid::new_v4();
            let result = {
                let mut manager = storage.write().await;
                if use_cloud {
                    manager.create_cook_and_run_in_cloud(id, &project).await
                } else {
                    manager.create_cook_and_run(id, &project).await
                }
            };
            match result {
                Ok(()) => {
                    storage.invalidate();
                    onclose.call(());
                    navigator.push(Route::Overview { cook_and_run_id: id });
                }
                Err(e) => toasts.error("Creating the project failed", e),
            }
        });
    };

    let on_import = move |event: Event<FormData>| {
        let files = event.files();
        import_task.run(async move {
            let Some(file) = files.into_iter().next() else { return };
            let result: Result<Uuid, String> = async {
                let content = file
                    .read_string()
                    .await
                    .map_err(|e| format!("The file could not be read: {e}"))?;
                let project = CookAndRunData::from_json(&content)?;
                let id = { storage.write().await.create_from_file(project).await }?;
                Ok(id)
            }
            .await;
            match result {
                Ok(id) => {
                    storage.invalidate();
                    onclose.call(());
                    navigator.push(Route::Overview { cook_and_run_id: id });
                }
                Err(e) => toasts.error("Import failed", e),
            }
        });
    };

    rsx! {
        Dialog { title: "New Project", onclose,
            Field { label: "Project name", error: shown_error.clone(),
                TextInput {
                    value: name(),
                    placeholder: "e.g. Summer Cook & Run 2025",
                    error: !shown_error.is_empty(),
                    oninput: move |e: Event<FormData>| {
                        touched.set(true);
                        name.set(e.value());
                    },
                    // Enter creates the project as soon as the form is valid.
                    onenter: move |_| {
                        touched.set(true);
                        if can_create && !create_task.is_busy() {
                            create();
                        }
                    },
                }
            }

            Checkbox {
                label: "Upload to the cloud",
                checked: use_cloud,
                disabled: !cloud_ready,
                onchange: move |checked| cloud_choice.set(Some(checked)),
            }
            if let Some(hint) = cloud_hint {
                p { class: "text-xs text-zinc-400 -mt-2", "{hint}" }
            }
            if use_cloud {
                Field {
                    label: "Admin notification email",
                    error: shown_email_error.clone(),
                    hint: "Receives notifications about this project.".to_string(),
                    TextInput {
                        kind: InputKind::Email,
                        value: email_value.clone(),
                        placeholder: "admin@example.com",
                        error: !shown_email_error.is_empty(),
                        oninput: move |e: Event<FormData>| email.set(e.value()),
                    }
                }
            }

            div { class: "flex items-center gap-3 pt-2",
                // A <label> wrapping the hidden file input opens the file picker natively.
                label { class: "{button_class(ButtonVariant::Secondary)}",
                    if import_task.is_busy() {
                        Spinner {}
                    }
                    "Import file"
                    FileInput { id: "project-import-input", accept: ".tcc", onpick: on_import }
                }
                Button {
                    label: "Create",
                    loading: create_task.is_busy(),
                    disabled: !can_create,
                    onclick: move |_| create(),
                }
            }
        }
    }
}
