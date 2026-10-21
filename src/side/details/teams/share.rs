//! Share dialog: invitation link / QR code (Info) and registration settings (Config).

use base64::{engine::general_purpose, Engine};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Utc};
use dioxus::prelude::*;
use qrcode::{render::svg, QrCode};
use uuid::Uuid;

use crate::data::use_storage;
use crate::storage::{RequiredField, ShareTeamConfig, ShareTeamConfigCreate};
use crate::time::{local_to_utc, local_wall_clock};
use crate::ui::*;
use crate::web::{download_text, origin};
use crate::Route;

const FIELD_OPTIONS: [(RequiredField, &str); 4] = [
    (RequiredField::Mail, "Email"),
    (RequiredField::Phone, "Phone"),
    (RequiredField::Members, "Number of Members"),
    (RequiredField::Diets, "Dietary Requirements"),
];

// ─────────────────────────────────────────────
//  Dialog
// ─────────────────────────────────────────────

#[component]
pub fn ShareDialog(
    cook_and_run_id: Uuid,
    config: Option<ShareTeamConfig>,
    team_count: usize,
    onclose: EventHandler<()>,
) -> Element {
    // Captured once; reading the clock in the render body would change the output on every render.
    let now = use_hook(|| Utc::now().naive_utc());
    let deadline_passed = config
        .as_ref()
        .and_then(|config| config.registration_deadline)
        .is_some_and(|deadline| deadline < now);

    let mut tab = use_signal(|| if config.is_some() && !deadline_passed { 0 } else { 1 });
    // Without a saved configuration there is nothing to show on the info tab.
    let shown_tab = if config.is_none() { 1 } else { tab() };
    let info_disabled = config.is_none() || deadline_passed;

    let initial = config
        .as_ref()
        .map(ShareTeamConfig::to_create)
        .unwrap_or_default();
    let is_new = config.is_none();

    rsx! {
        Dialog { title: "Share Configuration", wide: true, onclose,
            Tabs {
                tabs: vec!["Info", "Config"],
                selected: shown_tab,
                onselect: move |index| tab.set(index),
                disabled: if info_disabled { vec![0] } else { vec![] },
            }

            if shown_tab == 0 {
                if let Some(config) = config {
                    InfoTab { cook_and_run_id, config, team_count }
                }
            } else {
                ConfigTab {
                    key: "{is_new}",
                    cook_and_run_id,
                    initial,
                    is_new,
                    deadline_passed,
                    onsaved: move |_| tab.set(0),
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Info tab
// ─────────────────────────────────────────────

fn qr_svg(url: &str) -> Option<String> {
    let code = QrCode::new(url.as_bytes()).ok()?;
    Some(
        code.render::<svg::Color>()
            .min_dimensions(200, 200)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build(),
    )
}

#[component]
fn InfoTab(cook_and_run_id: Uuid, config: ShareTeamConfig, team_count: usize) -> Element {
    let toasts = use_toasts();

    // The link is built from the route, not from a hard-coded path.
    let share_url = use_hook(|| {
        format!("{}{}", origin().unwrap_or_default(), Route::ShareRegisterPage { cook_and_run_id })
    });
    let qr = use_hook({
        let share_url = share_url.clone();
        move || qr_svg(&share_url)
    });
    let qr_uri = qr
        .as_ref()
        .map(|svg| format!("data:image/svg+xml;base64,{}", general_purpose::STANDARD.encode(svg)))
        .unwrap_or_default();

    let max_reached = config.max_teams.is_some_and(|max| team_count >= max as usize);
    let allowed = config.max_teams.map(|max| max.to_string()).unwrap_or_else(|| "∞".to_string());
    let tone = if max_reached {
        "p-4 rounded-xl border border-red-200 bg-red-50"
    } else {
        "p-4 rounded-xl border border-amber-100 bg-amber-50/50"
    };
    let dim = if max_reached { "opacity-50 pointer-events-none" } else { "" };

    let qr_download = qr.clone();
    let on_download_qr = move |_| {
        if let Some(svg) = &qr_download {
            if let Err(e) = download_text("qr-code.svg", svg) {
                toasts.error("Download failed", e);
            }
        }
    };

    rsx! {
        div { class: "space-y-4",
            div { class: "grid grid-cols-2 gap-4",
                div { class: "{tone}",
                    Eyebrow { text: "Teams Registered" }
                    p { class: "text-2xl font-bold text-brand-hover", "{team_count}" }
                }
                div { class: "{tone}",
                    Eyebrow { text: "Teams Allowed" }
                    p { class: "text-2xl font-bold text-brand-hover", "{allowed}" }
                }
            }

            if max_reached {
                div { class: "rounded-xl border border-amber-300 bg-amber-50 px-4 py-3",
                    p { class: "text-sm font-semibold text-amber-700",
                        "Maximum team limit reached – no more teams can join via the share link."
                    }
                }
            }

            div { class: "flex flex-col space-y-4 {dim}",
                div { class: "flex flex-col items-center gap-3 p-4 rounded-xl border border-amber-100 bg-amber-50/30",
                    Eyebrow { text: "QR Code" }
                    if qr_uri.is_empty() {
                        p { class: "text-sm text-zinc-500", "The QR code could not be created." }
                    } else {
                        div { class: "bg-white p-3 rounded-xl border border-amber-100",
                            img { src: "{qr_uri}", alt: "QR code of the share link", class: "w-44 h-44" }
                        }
                        Button { label: "Download QR Code", onclick: on_download_qr }
                    }
                }

                CopyField { label: "Share Link", value: share_url.clone() }
            }
        }
    }
}

#[component]
fn Eyebrow(#[props(into)] text: String) -> Element {
    rsx! {
        p { class: "text-[11px] font-semibold uppercase tracking-[0.12em] text-zinc-400 mb-1", "{text}" }
    }
}

// ─────────────────────────────────────────────
//  Config tab
// ─────────────────────────────────────────────

/// Only the invitation text / only everything else, so each message lands next to its field.
fn invite_error(config: &ShareTeamConfigCreate) -> String {
    let mut probe = config.clone();
    probe.max_teams = None;
    probe.validate().err().unwrap_or_default()
}

fn other_error(config: &ShareTeamConfigCreate) -> String {
    let mut probe = config.clone();
    probe.invite_text = "x".to_string();
    probe.validate().err().unwrap_or_default()
}

#[component]
fn ConfigTab(
    cook_and_run_id: Uuid,
    initial: ShareTeamConfigCreate,
    is_new: bool,
    deadline_passed: bool,
    onsaved: EventHandler<()>,
) -> Element {
    let storage = use_storage();
    let toasts = use_toasts();
    let save_task = use_busy();

    let mut draft = use_signal(|| initial);
    let mut attempted = use_signal(|| false);

    let current = draft.read().clone();
    let (invite_msg, other_msg) = (invite_error(&current), other_error(&current));
    // An untouched, empty invitation text stays quiet until the user tries to save.
    let invite_shown = if current.invite_text.is_empty() && !attempted() { String::new() } else { invite_msg };

    let on_save = move |_| {
        attempted.set(true);
        let config = draft.peek().clone();
        if config.validate().is_err() {
            return;
        }
        save_task.run(async move {
            let result = {
                // These calls take `&self`, a shared guard is enough.
                let storage = storage.read().await;
                if is_new {
                    storage.create_cook_and_run_share_config(cook_and_run_id, &config).await
                } else {
                    storage.update_cook_and_run_share_config(cook_and_run_id, &config).await
                }
            };
            match result {
                Ok(()) => {
                    storage.invalidate();
                    toasts.success("Share configuration saved", "");
                    onsaved.call(());
                }
                Err(e) => toasts.error("Saving the share configuration failed", e),
            }
        });
    };

    rsx! {
        div { class: "flex flex-col space-y-5",
            Field { label: "Invitation Text", error: invite_shown.clone(),
                TextArea {
                    value: current.invite_text.clone(),
                    placeholder: "Enter an invitation text...",
                    rows: 4,
                    error: !invite_shown.is_empty(),
                    oninput: move |e: Event<FormData>| draft.write().invite_text = e.value(),
                }
            }

            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3",
                OptionBox { title: "Teams must confirm their email address",
                    Checkbox {
                        label: "Email verification",
                        checked: current.require_email_verification,
                        onchange: move |checked| draft.write().require_email_verification = checked,
                    }
                }
                OptionBox { title: "New teams are reviewed before they take part",
                    Checkbox {
                        label: "Review new teams",
                        checked: current.default_needs_check,
                        onchange: move |checked| draft.write().default_needs_check = checked,
                    }
                }
            }

            FieldGroup {
                title: "Required Fields",
                selected: current.required_fields.clone(),
                onchange: move |fields| draft.write().required_fields = fields,
            }
            FieldGroup {
                title: "Send team back to review when it changes",
                selected: current.review_trigger_fields.clone(),
                onchange: move |fields| draft.write().review_trigger_fields = fields,
            }

            Field {
                label: "Max Teams",
                error: other_msg.clone(),
                hint: "Leave empty for unlimited.".to_string(),
                TextInput {
                    kind: InputKind::Number,
                    value: current.max_teams.map(|max| max.to_string()).unwrap_or_default(),
                    placeholder: "Unlimited",
                    error: !other_msg.is_empty(),
                    oninput: move |e: Event<FormData>| {
                        let text = e.value();
                        let text = text.trim();
                        draft.write().max_teams = match text.parse::<u32>() {
                            Ok(0) | Err(_) => None,
                            Ok(max) => Some(max),
                        };
                    },
                }
            }

            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                DeadlineField {
                    label: "Registration deadline",
                    value: current.registration_deadline,
                    onchange: move |value| draft.write().registration_deadline = value,
                }
                DeadlineField {
                    label: "Edit deadline",
                    value: current.edit_deadline,
                    onchange: move |value| draft.write().edit_deadline = value,
                }
            }
            if deadline_passed {
                div { class: "rounded-xl border border-amber-300 bg-amber-50 px-3 py-2",
                    p { class: "text-xs font-semibold text-amber-700", "The registration deadline has passed." }
                }
            }

            OptionBox { title: "Receive an email when something happens",
                div { class: "space-y-2",
                    Eyebrow { text: "Notify the admin" }
                    div { class: "grid grid-cols-1 sm:grid-cols-3 gap-2",
                        Checkbox {
                            label: "New team",
                            checked: current.notify_admin_on_create,
                            onchange: move |checked| draft.write().notify_admin_on_create = checked,
                        }
                        Checkbox {
                            label: "Team needs review",
                            checked: current.notify_admin_on_review,
                            onchange: move |checked| draft.write().notify_admin_on_review = checked,
                        }
                        Checkbox {
                            label: "Team canceled",
                            checked: current.notify_admin_on_cancel,
                            onchange: move |checked| draft.write().notify_admin_on_cancel = checked,
                        }
                    }
                }
            }

            div { class: "flex justify-center",
                Button {
                    label: if is_new { "Create & Activate" } else { "Update" },
                    loading: save_task.is_busy(),
                    onclick: on_save,
                }
            }
        }
    }
}

#[component]
fn OptionBox(#[props(into)] title: String, children: Element) -> Element {
    rsx! {
        div {
            title: "{title}",
            class: "px-3 py-2.5 rounded-xl border border-amber-100 bg-amber-50/30 hover:bg-amber-50/60 transition-colors",
            {children}
        }
    }
}

/// A set of the four team fields (required fields / review triggers).
#[component]
fn FieldGroup(
    #[props(into)] title: String,
    selected: Vec<RequiredField>,
    onchange: EventHandler<Vec<RequiredField>>,
) -> Element {
    rsx! {
        div { class: "rounded-xl border border-amber-100 bg-amber-50/30 p-4",
            Eyebrow { text: title }
            div { class: "grid grid-cols-2 gap-2 mt-2",
                for (field, label) in FIELD_OPTIONS {
                    Checkbox {
                        key: "{label}",
                        label,
                        checked: selected.contains(&field),
                        onchange: {
                            let selected = selected.clone();
                            move |checked: bool| {
                                let mut next: Vec<RequiredField> =
                                    selected.iter().copied().filter(|f| *f != field).collect();
                                if checked {
                                    next.push(field);
                                }
                                onchange.call(next);
                            }
                        },
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Deadline input (UTC in the data, local time on screen)
// ─────────────────────────────────────────────

#[component]
fn DeadlineField(
    #[props(into)] label: String,
    value: Option<NaiveDateTime>,
    onchange: EventHandler<Option<NaiveDateTime>>,
) -> Element {
    // The old dialog showed the UTC fields as if they were local time and converted them
    // back as local time on save, so the deadline moved by the UTC offset with every save.
    let local = value.map(local_wall_clock);
    let date_text = local.map(|l| l.date().format("%Y-%m-%d").to_string()).unwrap_or_default();
    let time_text = local.map(|l| l.time().format("%H:%M").to_string()).unwrap_or_default();

    rsx! {
        Field { label,
            div { class: "flex gap-2",
                TextInput {
                    kind: InputKind::Date,
                    value: date_text,
                    class: "flex-1",
                    oninput: move |e: Event<FormData>| {
                        match NaiveDate::parse_from_str(&e.value(), "%Y-%m-%d") {
                            Ok(date) => {
                                let time = local
                                    .map(|l| l.time())
                                    .or_else(|| NaiveTime::from_hms_opt(18, 0, 0))
                                    .unwrap_or(NaiveTime::MIN);
                                onchange.call(local_to_utc(date.and_time(time)));
                            }
                            // Cleared: the deadline is removed.
                            Err(_) => onchange.call(None),
                        }
                    },
                }
                TextInput {
                    kind: InputKind::Time,
                    value: time_text,
                    class: "flex-1",
                    disabled: local.is_none(),
                    oninput: move |e: Event<FormData>| {
                        if let (Some(local), Ok(time)) = (local, NaiveTime::parse_from_str(&e.value(), "%H:%M")) {
                            onchange.call(local_to_utc(local.date().and_time(time)));
                        }
                    },
                }
            }
        }
    }
}
