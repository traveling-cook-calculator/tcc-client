//! Courses of a project: list, add, rename, set time, delete.

use chrono::NaiveTime;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::data::use_storage;
use crate::storage::{CourseCreate, CourseData, CourseUpdate, MAX_COURSES_PER_PROJECT};
use crate::ui::*;

const TIME_FORMAT: &str = "%H:%M";

// ─────────────────────────────────────────────
//  Row model (what the user edits)
// ─────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct CourseRow {
    id: Uuid,
    name: String,
    /// `HH:MM`, exactly what the time input shows.
    time: String,
}

impl CourseRow {
    fn from_data(data: &CourseData) -> Self {
        Self {
            id: data.id,
            name: data.name.clone(),
            time: data.time.format(TIME_FORMAT).to_string(),
        }
    }

    fn blank() -> Self {
        Self { id: Uuid::new_v4(), name: String::new(), time: "00:00".to_string() }
    }

    fn parsed_time(&self) -> Result<NaiveTime, String> {
        NaiveTime::parse_from_str(&self.time, TIME_FORMAT)
            .or_else(|_| NaiveTime::parse_from_str(&self.time, "%H:%M:%S"))
            .map_err(|_| "Please enter a valid time.".to_string())
    }

    /// Name rules come from the storage layer (single source of truth).
    fn name_error(&self) -> String {
        let time = self.parsed_time().unwrap_or(NaiveTime::MIN);
        CourseCreate { name: self.name.trim().to_string(), time }
            .validate()
            .err()
            .unwrap_or_default()
    }

    fn time_error(&self) -> String {
        self.parsed_time().err().unwrap_or_default()
    }

    fn is_valid(&self) -> bool {
        self.name_error().is_empty() && self.time_error().is_empty()
    }
}

// ─────────────────────────────────────────────
//  Root
// ─────────────────────────────────────────────

#[component]
pub fn Courses(cook_and_run_id: ReadSignal<Uuid>) -> Element {
    let storage = use_storage();
    let courses = use_resource(move || {
        let id = cook_and_run_id();
        async move {
            let _ = storage.revision();
            storage.read().await.select_cook_and_run_course_list(id).await
        }
    });

    let id = cook_and_run_id();
    render_resource(
        courses,
        "Could not load courses. You may need to log in or the servers may be offline.",
        move |courses| rsx! { CoursesContent { key: "{id}", cook_and_run_id: id, courses } },
    )
}

// ─────────────────────────────────────────────
//  Content
// ─────────────────────────────────────────────

#[component]
fn CoursesContent(cook_and_run_id: Uuid, courses: Vec<CourseData>) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let flash = use_flash();
    let save_task = use_busy();

    let mut rows = use_signal(|| courses.iter().map(CourseRow::from_data).collect::<Vec<_>>());
    let mut saved = use_signal(|| courses.iter().map(CourseRow::from_data).collect::<Vec<_>>());

    let id = cook_and_run_id;
    let list = rows.read().clone();
    let known: Vec<Uuid> = saved.read().iter().map(|row| row.id).collect();
    let can_save = list != *saved.read() && list.iter().all(CourseRow::is_valid);
    let can_add = list.len() < MAX_COURSES_PER_PROJECT;

    let on_save = move |_| {
        let target = rows.peek().clone();
        save_task.run(async move {
            let mut persisted = saved.peek().clone();
            for row in &target {
                let previous = persisted.iter().find(|p| p.id == row.id).cloned();
                if previous.as_ref() == Some(row) {
                    continue;
                }
                let time = match row.parsed_time() {
                    Ok(time) => time,
                    Err(e) => return toasts.error("Invalid time", e),
                };
                let name = row.name.trim().to_string();

                let result = {
                    let mut storage = storage.write().await;
                    if previous.is_some() {
                        storage
                            .update_course_of_cook_and_run(id, row.id, &CourseUpdate { name, time })
                            .await
                    } else {
                        storage
                            .create_course_of_cook_and_run(id, row.id, &CourseCreate { name, time })
                            .await
                    }
                };

                match result {
                    Ok(()) => {
                        match persisted.iter_mut().find(|p| p.id == row.id) {
                            Some(slot) => *slot = row.clone(),
                            None => persisted.push(row.clone()),
                        }
                        // Progress is kept even if a later row fails.
                        saved.set(persisted.clone());
                    }
                    Err(e) => return toasts.error(format!("Saving \"{}\" failed", row.name), e),
                }
            }
            flash.trigger();
        });
    };

    rsx! {
        PageShell {
            PageHeader { title: "Courses" }

            if list.is_empty() {
                p { class: "text-sm text-zinc-500", "No courses yet. Add the first one below." }
            }

            for (position, row) in list.into_iter().enumerate() {
                CourseCard {
                    key: "{row.id}",
                    cook_and_run_id: id,
                    position,
                    persisted: known.contains(&row.id),
                    flash: flash.is_active(),
                    row: row.clone(),
                    on_name: move |value| update_row(rows, row.id, |r| r.name = value),
                    on_time: move |value| update_row(rows, row.id, |r| r.time = value),
                    on_removed: move |removed: Uuid| {
                        rows.write().retain(|r| r.id != removed);
                        saved.write().retain(|r| r.id != removed);
                    },
                }
            }

            if can_add {
                AddCardButton {
                    label: "Add course",
                    onclick: move |_| rows.write().push(CourseRow::blank()),
                }
            } else {
                p { class: "text-xs text-zinc-400",
                    "A project can have at most {MAX_COURSES_PER_PROJECT} courses."
                }
            }

            div { class: "flex items-center gap-3",
                Button {
                    label: "Save",
                    loading: save_task.is_busy(),
                    disabled: !can_save,
                    onclick: on_save,
                }
            }
        }
    }
}

fn update_row(mut rows: Signal<Vec<CourseRow>>, id: Uuid, change: impl FnOnce(&mut CourseRow)) {
    if let Some(row) = rows.write().iter_mut().find(|r| r.id == id) {
        change(row);
    }
}

#[component]
fn CourseCard(
    cook_and_run_id: Uuid,
    position: usize,
    row: CourseRow,
    persisted: bool,
    flash: bool,
    on_name: EventHandler<String>,
    on_time: EventHandler<String>,
    on_removed: EventHandler<Uuid>,
) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let task = use_busy();

    // A freshly added, still empty row does not complain yet (Save stays disabled).
    let name_error = if row.name.is_empty() && !persisted { String::new() } else { row.name_error() };
    let time_error = row.time_error();
    let row_id = row.id;

    rsx! {
        Card {
            title: format!("Course {}", position + 1),
            eyebrow: true,
            flash,
            actions: rsx! {
                DeleteButton {
                    loading: task.is_busy(),
                    onclick: move |_| {
                        if !persisted {
                            on_removed.call(row_id);
                            return;
                        }
                        task.run(async move {
                            let result = {
                                storage.write().await.delete_course_of_cook_and_run(cook_and_run_id, row_id).await
                            };
                            match result {
                                Ok(()) => on_removed.call(row_id),
                                Err(e) => toasts.error("Deleting failed", e),
                            }
                        });
                    },
                }
            },
            div { class: "grid grid-cols-1 sm:grid-cols-[1fr_10rem] gap-4",
                Field { label: "Course name", error: name_error.clone(),
                    TextInput {
                        value: row.name.clone(),
                        placeholder: "e.g. Starter",
                        error: !name_error.is_empty(),
                        oninput: move |e: Event<FormData>| on_name.call(e.value()),
                    }
                }
                Field { label: "Time", error: time_error.clone(),
                    TextInput {
                        kind: InputKind::Time,
                        value: row.time.clone(),
                        error: !time_error.is_empty(),
                        oninput: move |e: Event<FormData>| on_time.call(e.value()),
                    }
                }
            }
        }
    }
}
