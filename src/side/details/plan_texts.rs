//! Title, description, date and language of the plan (shown on the Calculation page). They are the
//! texts behind `{event_title}`, `{event_description}` and `{event_date}` on the schedule sheets.

use chrono::NaiveDate;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::data::use_storage;
use crate::storage::{Language, PlanConfigData};
use crate::ui::*;

const DATE_FORMAT: &str = "%Y-%m-%d";

#[component]
pub(crate) fn TextsCard(
    cook_and_run_id: Uuid,
    saved_config: PlanConfigData,
    draft: Signal<PlanConfigData>,
) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let flash = use_flash();
    let save_task = use_busy();
    let mut saved = use_signal(|| saved_config);

    let current = draft.read().clone();

    // Field errors come from the storage validation; the other field is neutralized so that only
    // one message is produced at a time.
    let title_error = {
        let mut probe = current.clone();
        probe.description = String::new();
        probe.validate().err().unwrap_or_default()
    };
    let description_error = {
        let mut probe = current.clone();
        probe.title = "Plan".to_string();
        probe.validate().err().unwrap_or_default()
    };
    let can_save = current != *saved.read() && title_error.is_empty() && description_error.is_empty();

    let on_save = move |_| {
        let config = draft.peek().clone();
        save_task.run(async move {
            let result = {
                storage.write().await.update_plan_config_of_cook_and_run(cook_and_run_id, &config).await
            };
            match result {
                Ok(()) => {
                    saved.set(config);
                    flash.trigger();
                }
                Err(e) => toasts.error("Saving the plan settings failed", e),
            }
        });
    };

    rsx! {
        Card { title: "Plan settings", flash: flash.is_active(),
            p { class: "text-xs text-zinc-500",
                "Title, description and date appear on the schedule sheets."
            }
            Field { label: "Title", error: title_error.clone(),
                TextInput {
                    value: current.title.clone(),
                    placeholder: "Plan title",
                    error: !title_error.is_empty(),
                    oninput: move |e: Event<FormData>| draft.write().title = e.value(),
                }
            }
            Field { label: "Description", error: description_error.clone(),
                TextArea {
                    value: current.description.clone(),
                    placeholder: "Short description",
                    rows: 3,
                    error: !description_error.is_empty(),
                    oninput: move |e: Event<FormData>| draft.write().description = e.value(),
                }
            }
            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                Field { label: "Date",
                    TextInput {
                        kind: InputKind::Date,
                        value: current.date.format(DATE_FORMAT).to_string(),
                        oninput: move |e: Event<FormData>| {
                            if let Ok(date) = NaiveDate::parse_from_str(&e.value(), DATE_FORMAT) {
                                draft.write().date = date;
                            }
                        },
                    }
                }
                Field { label: "Language",
                    Select {
                        value: current.language.to_string(),
                        options: vec![("eng", "English"), ("deu", "German")],
                        onchange: move |value: String| draft.write().language = Language::from_string(value),
                    }
                }
            }
            div {
                Button {
                    label: "Save texts",
                    loading: save_task.is_busy(),
                    disabled: !can_save,
                    onclick: on_save,
                }
            }
        }
    }
}
