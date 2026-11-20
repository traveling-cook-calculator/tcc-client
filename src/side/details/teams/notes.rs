//! Notes tab of a team.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::data::use_storage;
use crate::storage::{NoteCreate, NoteData};
use crate::time::format_local;
use crate::ui::*;

/// Field errors from the storage validation (each field checked against a valid other one).
fn note_errors(headline: &str, content: &str) -> (String, String) {
    let check = |headline: &str, content: &str| {
        NoteCreate { headline: headline.to_string(), content: content.to_string() }
            .validate()
            .err()
            .unwrap_or_default()
    };
    (check(headline, "x"), check("x", content))
}

/// `notes` is the single source of truth: after posting, the page reloads and passes the stored list.
#[component]
pub fn TeamNotes(cook_and_run_id: Uuid, team_id: Uuid, notes: Vec<NoteData>) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let post_task = use_busy();

    let mut headline = use_signal(String::new);
    let mut content = use_signal(String::new);
    let mut attempted = use_signal(|| false);

    let (headline_now, content_now) = (headline.read().trim().to_string(), content.read().trim().to_string());
    let (headline_error, content_error) = note_errors(&headline_now, &content_now);
    // Empty fields stay quiet until the first post attempt.
    let show = |value: &str, error: &str| {
        if value.is_empty() && !attempted() { String::new() } else { error.to_string() }
    };
    let (headline_shown, content_shown) =
        (show(&headline_now, &headline_error), show(&content_now, &content_error));

    let on_post = move |_| {
        attempted.set(true);
        let (headline_text, content_text) = (headline.peek().trim().to_string(), content.peek().trim().to_string());
        let (headline_error, content_error) = note_errors(&headline_text, &content_text);
        if !headline_error.is_empty() || !content_error.is_empty() {
            return;
        }
        let note = NoteCreate { headline: headline_text, content: content_text };
        post_task.run(async move {
            // One id for the request *and* for what is shown (the old code made up a second one).
            let note_id = Uuid::new_v4();
            let result = {
                storage.write().await.create_team_note_of_cook_and_run(cook_and_run_id, team_id, note_id, &note).await
            };
            match result {
                Ok(()) => {
                    headline.set(String::new());
                    content.set(String::new());
                    attempted.set(false);
                    storage.invalidate();
                }
                Err(e) => toasts.error("Posting the note failed", e),
            }
        });
    };

    let mut sorted = notes;
    sorted.sort_by(|a, b| b.created.cmp(&a.created));

    rsx! {
        div { class: "flex flex-col md:flex-row gap-6",
            div { class: "flex-1 md:border-r border-amber-100 md:pr-5 space-y-4",
                Field { label: "Headline", error: headline_shown.clone(),
                    TextInput {
                        value: headline(),
                        placeholder: "e.g. Participation fee",
                        error: !headline_shown.is_empty(),
                        oninput: move |e: Event<FormData>| headline.set(e.value()),
                    }
                }
                Field { label: "Content", error: content_shown.clone(),
                    TextArea {
                        value: content(),
                        placeholder: "e.g. Participation fee is partially paid!",
                        rows: 4,
                        error: !content_shown.is_empty(),
                        oninput: move |e: Event<FormData>| content.set(e.value()),
                    }
                }
                Button { label: "Post Note", loading: post_task.is_busy(), onclick: on_post }
            }

            div { class: "flex-1",
                Field { label: "Notes",
                    if sorted.is_empty() {
                        p { class: "text-sm text-zinc-400", "No notes yet." }
                    }
                    div { class: "space-y-2 overflow-y-auto max-h-80",
                        for note in sorted {
                            NoteCard { key: "{note.id}", note }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NoteCard(note: NoteData) -> Element {
    rsx! {
        Panel { flush: true, class: "px-4 py-3",
            div { class: "flex justify-between items-baseline gap-2 mb-1",
                span { class: "text-sm font-semibold text-zinc-800 truncate", "{note.headline}" }
                span { class: "text-[11px] text-zinc-400 shrink-0", "{format_local(note.created)}" }
            }
            p { class: "text-xs text-zinc-600 leading-relaxed whitespace-pre-wrap", "{note.content}" }
        }
    }
}
