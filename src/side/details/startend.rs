//! Start and end point of a project.

use chrono::NaiveTime;
use dioxus::prelude::*;
use uuid::Uuid;

use super::address::{Address, AddressParam};
use crate::data::use_storage;
use crate::storage::{AddressData, MeetingPointData};
use crate::ui::*;

const TIME_FORMAT: &str = "%H:%M";

// ─────────────────────────────────────────────
//  Draft model
// ─────────────────────────────────────────────

/// What the user edits for one point. `time` is the text of the time input.
#[derive(Clone, PartialEq)]
struct PointDraft {
    enabled: bool,
    name: String,
    time: String,
}

impl PointDraft {
    fn from_point(point: &Option<MeetingPointData>) -> Self {
        match point {
            Some(p) => Self {
                enabled: true,
                name: p.name.clone(),
                time: p.time.format(TIME_FORMAT).to_string(),
            },
            None => Self { enabled: false, name: String::new(), time: "00:00".to_string() },
        }
    }

    fn parsed_time(&self) -> Result<NaiveTime, String> {
        NaiveTime::parse_from_str(&self.time, TIME_FORMAT)
            .or_else(|_| NaiveTime::parse_from_str(&self.time, "%H:%M:%S"))
            .map_err(|_| "Please enter a valid time.".to_string())
    }

    fn name_error(&self) -> String {
        if self.enabled && self.name.trim().is_empty() {
            "Name cannot be empty!".to_string()
        } else {
            String::new()
        }
    }

    fn time_error(&self) -> String {
        if self.enabled { self.parsed_time().err().unwrap_or_default() } else { String::new() }
    }

    fn is_valid(&self) -> bool {
        !self.enabled || (self.name_error().is_empty() && self.time_error().is_empty())
    }
}

type AddressSnapshot = (String, String, String);

/// `Ok(None)` = point switched off, `Ok(Some(..))` = point to store.
fn to_meeting_point(
    draft: &PointDraft,
    address: AddressParam,
) -> Result<Option<MeetingPointData>, String> {
    if !draft.enabled {
        return Ok(None);
    }
    // Also lets the address form highlight what is wrong.
    let address: AddressData = address.get_address_data()?;
    Ok(Some(MeetingPointData {
        name: draft.name.trim().to_string(),
        time: draft.parsed_time()?,
        address,
    }))
}

// ─────────────────────────────────────────────
//  Root
// ─────────────────────────────────────────────

#[component]
pub fn StartEnd(cook_and_run_id: ReadSignal<Uuid>) -> Element {
    let storage = use_storage();
    let points = use_resource(move || {
        let id = cook_and_run_id();
        async move {
            let _ = storage.revision();
            let storage = storage.read().await;
            let start = storage.select_cook_and_run_start_point(id).await?;
            let end = storage.select_cook_and_run_end_point(id).await?;
            Ok((start, end))
        }
    });

    let id = cook_and_run_id();
    render_resource(
        points,
        "Could not load project. You may need to log in or the servers may be offline.",
        move |(start_point, end_point)| {
            rsx! { StartEndContent { key: "{id}", cook_and_run_id: id, start_point, end_point } }
        },
    )
}

// ─────────────────────────────────────────────
//  Content
// ─────────────────────────────────────────────

#[component]
fn StartEndContent(
    cook_and_run_id: Uuid,
    start_point: Option<MeetingPointData>,
    end_point: Option<MeetingPointData>,
) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let flash = use_flash();
    let save_task = use_busy();

    let start = use_signal(|| PointDraft::from_point(&start_point));
    let end = use_signal(|| PointDraft::from_point(&end_point));
    let mut saved_start = use_signal(|| PointDraft::from_point(&start_point));
    let mut saved_end = use_signal(|| PointDraft::from_point(&end_point));

    // Hooks must run unconditionally, so a missing point gets an empty address form.
    let start_address =
        AddressParam::new(&start_point.as_ref().map(|p| p.address.clone()).unwrap_or_default());
    let end_address =
        AddressParam::new(&end_point.as_ref().map(|p| p.address.clone()).unwrap_or_default());

    let initial_addresses: (AddressSnapshot, AddressSnapshot) =
        use_hook(|| (start_address.snapshot(), end_address.snapshot()));
    let mut saved_addresses = use_signal(move || initial_addresses);

    let (start_now, end_now) = (start.read().clone(), end.read().clone());
    let addresses_now = (start_address.snapshot(), end_address.snapshot());
    let dirty = start_now != *saved_start.read()
        || end_now != *saved_end.read()
        || addresses_now != *saved_addresses.read();
    let can_save = dirty && start_now.is_valid() && end_now.is_valid();
    let id = cook_and_run_id;

    let on_save = move |_| {
        let (start_draft, end_draft) = (start.peek().clone(), end.peek().clone());
        save_task.run(async move {
            let (start_point, end_point) = match (
                to_meeting_point(&start_draft, start_address),
                to_meeting_point(&end_draft, end_address),
            ) {
                (Ok(start), Ok(end)) => (start, end),
                (Err(e), _) | (_, Err(e)) => return toasts.error("Please check your input", e),
            };

            let result = {
                let mut storage = storage.write().await;
                match storage.update_start_point_in_cook_and_run(id, &start_point).await {
                    Ok(()) => storage.update_end_point_in_cook_and_run(id, &end_point).await,
                    Err(e) => Err(e),
                }
            };
            match result {
                Ok(()) => {
                    saved_start.set(start_draft);
                    saved_end.set(end_draft);
                    saved_addresses.set((start_address.snapshot(), end_address.snapshot()));
                    flash.trigger();
                }
                Err(e) => toasts.error("Saving the start and end point failed", e),
            }
        });
    };

    rsx! {
        PageShell {
            PageHeader { title: "Start & End Point" }

            div { class: "grid grid-cols-1 md:grid-cols-2 gap-6",
                PointCard {
                    title: "Start Point",
                    toggle_label: "Use start point",
                    icon: rsx! { StartSVG {} },
                    draft: start,
                    address: start_address,
                    flash: flash.is_active(),
                }
                PointCard {
                    title: "End Point",
                    toggle_label: "Use end point",
                    icon: rsx! { EndSVG {} },
                    draft: end,
                    address: end_address,
                    flash: flash.is_active(),
                }
            }

            div { class: "flex justify-end pt-1",
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

// ─────────────────────────────────────────────
//  Point card (used for both start and end)
// ─────────────────────────────────────────────

#[component]
fn PointCard(
    title: &'static str,
    toggle_label: &'static str,
    icon: Element,
    draft: Signal<PointDraft>,
    address: AddressParam,
    flash: bool,
) -> Element {
    let current = draft.read().clone();
    let name_error = current.name_error();
    let time_error = current.time_error();
    let dimmed = if current.enabled { "" } else { "opacity-40 pointer-events-none" };

    rsx! {
        Card { title, icon, flash,
            Checkbox {
                label: toggle_label,
                checked: current.enabled,
                onchange: move |checked| draft.write().enabled = checked,
            }
            div { class: "space-y-4 transition-opacity duration-150 {dimmed}",
                Field { label: "Name", error: name_error.clone(),
                    TextInput {
                        value: current.name.clone(),
                        placeholder: title,
                        error: !name_error.is_empty(),
                        oninput: move |e: Event<FormData>| draft.write().name = e.value(),
                    }
                }
                Field { label: "Time", error: time_error.clone(),
                    TextInput {
                        kind: InputKind::Time,
                        value: current.time.clone(),
                        error: !time_error.is_empty(),
                        oninput: move |e: Event<FormData>| draft.write().time = e.value(),
                    }
                }
                Address { param: address }
            }
        }
    }
}
