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
                Panel {
                    Eyebrow { text: "Teams Registered", class: "block mb-1" }
                    p { class: "text-2xl font-bold text-brand-hover", "{team_count}" }
                }
                Panel {
                    Eyebrow { text: "Teams Allowed", class: "block mb-1" }
                    p { class: "text-2xl font-bold text-brand-hover", "{allowed}" }
                }
            }

            if max_reached {
                Notice { tone: NoticeTone::Attention, roomy: true, title: "Maximum team limit reached",
                    "No more teams can join via the share link."
                }
            }

            div { class: "flex flex-col space-y-4 {dim}",
                Panel { class: "flex flex-col items-center gap-3",
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
                OptionBox { title: "Teams should confirm their email address (teams without confirmation may not be admitted)",
                    Checkbox {
                        label: "Email verification",
                        checked: current.require_email_verification,
                        onchange: move |checked| set_email_verification(&mut draft.write(), checked),
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

            FieldMatrix { draft }

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
                Notice { tone: NoticeTone::Attention, small: true, "The registration deadline has passed." }
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
        Panel { flush: true, class: "px-3 py-2.5 hover:bg-amber-50/60 transition-colors",
            div { title: "{title}", {children} }
        }
    }
}

/// The three questions the organiser answers per detail (name and address are always asked).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Setting {
    /// Guests may enter it at all (it is shown to them).
    Allowed,
    /// Guests have to enter it.
    Required,
    /// Changing it sends the team back to review.
    Review,
}

fn toggle(list: &mut Vec<RequiredField>, field: RequiredField, on: bool) {
    list.retain(|f| *f != field);
    if on {
        list.push(field);
    }
}

/// Changes one cell of the matrix and keeps the rows consistent, so the configuration is always
/// one the server accepts: what is required or reviewed must be allowed, and a required
/// confirmation of the address needs the address to be required.
fn apply_setting(config: &mut ShareTeamConfigCreate, field: RequiredField, setting: Setting, on: bool) {
    match setting {
        Setting::Allowed => {
            toggle(&mut config.allowed_fields, field, on);
            if !on {
                toggle(&mut config.required_fields, field, false);
                toggle(&mut config.review_trigger_fields, field, false);
                if field == RequiredField::Mail {
                    config.require_email_verification = false;
                }
            }
        }
        Setting::Required => {
            toggle(&mut config.required_fields, field, on);
            if on {
                toggle(&mut config.allowed_fields, field, true);
            }
            if !on && field == RequiredField::Mail {
                config.require_email_verification = false;
            }
        }
        Setting::Review => {
            toggle(&mut config.review_trigger_fields, field, on);
            if on {
                toggle(&mut config.allowed_fields, field, true);
            }
        }
    }
}

/// Turning the confirmation on makes the email address a required detail.
fn set_email_verification(config: &mut ShareTeamConfigCreate, on: bool) {
    config.require_email_verification = on;
    if on {
        apply_setting(config, RequiredField::Mail, Setting::Required, true);
    }
}

/// Which details guests may enter, which they must, and which changes trigger a review.
#[component]
fn FieldMatrix(draft: Signal<ShareTeamConfigCreate>) -> Element {
    let current = draft.read().clone();
    let columns = [
        (Setting::Allowed, "May enter"),
        (Setting::Required, "Required"),
        (Setting::Review, "Review on change"),
    ];
    rsx! {
        Panel { class: "space-y-3",
            Eyebrow { text: "Team details" }
            Hint {
                "Name and address are always asked. For every other detail decide whether guests may enter it, whether they have to, and whether changing it sends the team back to review. A detail guests may not enter is neither shown nor stored."
            }
            div { class: "overflow-x-auto",
                table { class: "w-full text-sm",
                    caption { class: "sr-only", "Settings for the details of a team" }
                    thead {
                        tr { class: "text-left text-xs text-zinc-500",
                            th { scope: "col", class: "py-1.5 pr-3 font-semibold", "Detail" }
                            for (_, title) in columns {
                                th { key: "{title}", scope: "col", class: "py-1.5 px-2 font-semibold text-center whitespace-nowrap", "{title}" }
                            }
                        }
                    }
                    tbody {
                        for (field, label) in FIELD_OPTIONS {
                            tr { key: "{label}", class: "border-t border-amber-100",
                                th { scope: "row", class: "py-2 pr-3 text-left font-medium text-zinc-800", "{label}" }
                                for (setting, title) in columns {
                                    {
                                        let allowed = current.allowed_fields.contains(&field);
                                        let checked = match setting {
                                            Setting::Allowed => allowed,
                                            Setting::Required => current.required_fields.contains(&field),
                                            Setting::Review => current.review_trigger_fields.contains(&field),
                                        };
                                        // The confirmation of the address needs the address as a required detail.
                                        let forced = field == RequiredField::Mail
                                            && current.require_email_verification
                                            && matches!(setting, Setting::Allowed | Setting::Required);
                                        rsx! {
                                            td { key: "{title}", class: "py-2 px-2 text-center",
                                                CheckboxCell {
                                                    label: format!("{label}: {title}"),
                                                    checked,
                                                    disabled: forced,
                                                    onchange: move |on| apply_setting(&mut draft.write(), field, setting, on),
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if current.require_email_verification {
                Hint { "The email address is required while the confirmation is switched on." }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config(allowed: &[RequiredField], required: &[RequiredField], review: &[RequiredField]) -> ShareTeamConfigCreate {
        ShareTeamConfigCreate {
            allowed_fields: allowed.to_vec(),
            required_fields: required.to_vec(),
            review_trigger_fields: review.to_vec(),
            invite_text: "x".into(),
            ..ShareTeamConfigCreate::default()
        }
    }

    #[test]
    fn required_or_review_switches_the_detail_on() {
        let mut c = config(&[], &[], &[]);
        apply_setting(&mut c, RequiredField::Phone, Setting::Required, true);
        apply_setting(&mut c, RequiredField::Diets, Setting::Review, true);
        assert!(c.allowed_fields.contains(&RequiredField::Phone) && c.allowed_fields.contains(&RequiredField::Diets));
        assert!(c.validate().is_ok());
    }

    #[test]
    fn switching_a_detail_off_clears_its_other_settings() {
        let all = RequiredField::ALL;
        let mut c = config(&all, &[RequiredField::Phone], &[RequiredField::Phone]);
        apply_setting(&mut c, RequiredField::Phone, Setting::Allowed, false);
        assert!(!c.allowed_fields.contains(&RequiredField::Phone));
        assert!(c.required_fields.is_empty() && c.review_trigger_fields.is_empty());
        assert!(c.validate().is_ok());
    }

    #[test]
    fn the_confirmation_needs_the_address_and_every_path_keeps_that() {
        let mut c = config(&[], &[], &[]);
        set_email_verification(&mut c, true);
        assert!(c.required_fields.contains(&RequiredField::Mail) && c.allowed_fields.contains(&RequiredField::Mail));
        assert!(c.validate().is_ok());
        // taking the address away takes the confirmation away
        apply_setting(&mut c, RequiredField::Mail, Setting::Required, false);
        assert!(!c.require_email_verification);
        assert!(c.validate().is_ok());
        set_email_verification(&mut c, true);
        apply_setting(&mut c, RequiredField::Mail, Setting::Allowed, false);
        assert!(!c.require_email_verification && c.required_fields.is_empty());
        assert!(c.validate().is_ok());
    }

    #[test]
    fn no_sequence_of_clicks_makes_an_invalid_configuration() {
        let all = RequiredField::ALL;
        let settings = [Setting::Allowed, Setting::Required, Setting::Review];
        let mut c = config(&all, &[], &[]);
        for step in 0..200usize {
            let field = all[step % 4];
            let setting = settings[(step / 4) % 3];
            let on = (step / 12) % 2 == 0;
            if step % 17 == 0 {
                set_email_verification(&mut c, on);
            } else {
                apply_setting(&mut c, field, setting, on);
            }
            assert!(c.validate().is_ok(), "step {step}: {c:?}");
        }
    }
}
