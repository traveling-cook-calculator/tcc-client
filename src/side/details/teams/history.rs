//! History tab of a team: the change log of the backend (who changed what, when), newest first.
//!
//! The entries contain personal data (email, phone). They are only kept in memory while the tab is
//! open: not cached, not logged.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::data::use_storage;
use crate::storage::audit_text::{action_text, actor_text, describe_changes};
use crate::storage::{AuditAction, AuditActorType, AuditLogEntry};
use crate::time::format_local;
use crate::ui::*;

const PAGE_SIZE: u32 = 50;
/// The backend keeps at most this many entries per team; when it is full the team cannot be
/// changed any more.
const LOG_CAPACITY: u32 = 255;

#[component]
pub fn TeamHistory(cook_and_run_id: Uuid, team_id: Uuid) -> Element {
    let storage = use_storage();
    let task = use_busy();
    let mut entries = use_signal(Vec::<AuditLogEntry>::new);
    let mut next_page = use_signal(|| 1u32);
    let mut has_next = use_signal(|| false);
    let mut total = use_signal(|| 0u32);
    let mut loaded = use_signal(|| false);
    let mut error = use_signal(String::new);

    let load = move || {
        task.run(async move {
            let page = *next_page.peek();
            let result = {
                storage
                    .read()
                    .await
                    .select_team_audit_log_of_cook_and_run(cook_and_run_id, team_id, page, PAGE_SIZE)
                    .await
            };
            match result {
                Ok(fetched) => {
                    {
                        // A page can overlap with the one before when entries were added meanwhile.
                        let mut entries = entries.write();
                        for entry in fetched.items {
                            if entries.iter().all(|known| known.id != entry.id) {
                                entries.push(entry);
                            }
                        }
                    }
                    has_next.set(fetched.pagination.has_next);
                    total.set(fetched.pagination.total);
                    next_page.set(page + 1);
                    error.set(String::new());
                    loaded.set(true);
                }
                Err(e) => error.set(e),
            }
        });
    };
    use_effect(move || load());

    let full = total() >= LOG_CAPACITY;

    rsx! {
        div { class: "space-y-3",
            Hint {
                "Changes of this team, newest first. Times are in your local time. Only you can see this."
            }
            if full {
                Notice { tone: NoticeTone::Attention, small: true,
                    "The log of this team is full ({LOG_CAPACITY} entries). The team can no longer be changed, canceled or sent a new verification email."
                }
            }
            if !error().is_empty() {
                Notice { tone: NoticeTone::Danger, small: true, class: "break-words",
                    "The history could not be loaded: {error}"
                }
            }
            if !loaded() && error().is_empty() {
                div { class: "space-y-2", "aria-busy": "true",
                    Skeleton { class: "h-10 w-full" }
                    Skeleton { class: "h-10 w-full" }
                    span { class: "sr-only", "Loading history…" }
                }
            }
            if loaded() && entries.read().is_empty() {
                p { class: "text-sm text-zinc-500", "No changes recorded yet." }
            }
            ul { class: "space-y-2",
                for entry in entries.read().iter().cloned() {
                    HistoryRow { key: "{entry.id}", entry }
                }
            }
            if has_next() || (!error().is_empty() && !loaded()) {
                div { class: "flex justify-center",
                    Button {
                        label: if error().is_empty() { "Load more" } else { "Try again" },
                        variant: ButtonVariant::Secondary,
                        loading: task.is_busy(),
                        onclick: move |_| load(),
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryRow(entry: AuditLogEntry) -> Element {
    let changes = if entry.action == AuditAction::Updated {
        describe_changes(&entry.changed_fields)
    } else {
        Vec::new()
    };
    let by_team = entry.actor_type == AuditActorType::Participant;
    rsx! {
        li { class: "rounded-xl border border-amber-100 bg-white px-3 py-2",
            div { class: "flex flex-wrap items-center gap-x-2 gap-y-1",
                span { class: "text-sm font-semibold text-ink-brown", "{action_text(&entry.action)}" }
                Pill { tone: if by_team { PillTone::Neutral } else { PillTone::Amber }, "{actor_text(&entry)}" }
                span { class: "ml-auto text-xs text-zinc-500", "{format_local(entry.created_at)}" }
            }
            if !changes.is_empty() {
                ul { class: "mt-1 space-y-0.5",
                    for change in changes {
                        li { class: "text-xs text-zinc-700 break-words", "{change.text()}" }
                    }
                }
            }
        }
    }
}
