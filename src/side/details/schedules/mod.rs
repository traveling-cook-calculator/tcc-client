//! Schedules: design the schedule sheet ("Laufzettel") that every team gets.
//!
//! A design is a sheet with a background, uploaded pictures, shapes and texts. The texts contain
//! placeholders (`{team_name}`, `{course1_host}`, `{guests}`, ...) that are replaced for each
//! team. The designs of a local project are kept in the browser (`storage::local_designs`), those
//! of a cloud project in the backend (see `cloud`): they are saved on request, and teams see the
//! one that is switched on. A design can be exported to and imported from a file.

mod canvas;
mod cloud;
mod export_all;
mod panels;
pub(crate) mod store;

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;

use self::canvas::Stage;
use self::cloud::CloudState;
use self::export_all::{download_all, ExportSheet, Source};
use self::panels::{InsertPanel, Inspector, SheetPanel};
use super::design_notice::NoDesignNotice;
use super::plan::load_team_schedule;
use super::plan_bundle::{load_bundle, PlanBundle};
use super::run_schedule::design::{download_design, DesignedSchedule};
use super::run_schedule::export::{file_stem, EXPORT_HOST_ID, EXPORT_SHEET_ID};
use super::run_schedule::sheet_design::LanguageTabs;
use super::run_schedule::{browser_language, Schedule, SheetInfo};
use crate::data::{use_storage, Storage};
use crate::schedule_design::{
    DesignImage, DesignLibrary, Element as DesignElement, ElementKind, Preset, ScheduleDesign,
    EXPORT_WIDTH, MAX_DESIGNS, MAX_ELEMENTS, MAX_IMAGES,
};
use crate::storage::design::CloudLibrary;
use crate::storage::Language;
use crate::ui::toast::Toasts;
use crate::ui::*;
use crate::web::download_text;

/// Handle of the designer state; every component gets it from the context.
#[derive(Clone, Copy)]
pub(super) struct Designer {
    library: Signal<DesignLibrary>,
    /// Counts the changes; the library is stored shortly after the last one.
    version: Signal<u32>,
    pub selected: Signal<Option<Uuid>>,
    pub toasts: Toasts,
    /// The project is in the cloud: its designs are saved on request instead of automatically.
    pub is_cloud: bool,
    project: Uuid,
    storage: Storage,
    /// Parallel to the library (only used for a cloud project).
    cloud: Signal<CloudState>,
    /// The design whose save found a newer version: the conflict dialog is open for it.
    conflict: Signal<Option<Uuid>>,
}

impl Designer {
    fn bump(&self) {
        let mut version = self.version;
        *version.write() += 1;
    }

    fn active(&self) -> Option<ScheduleDesign> {
        self.library.read().active_design().cloned()
    }

    /// Changes the design that is open.
    pub fn edit(&self, change: impl FnOnce(&mut ScheduleDesign)) {
        let mut library = self.library;
        {
            let mut library = library.write();
            let active = library.active;
            if let Some(design) = library.designs.get_mut(active) {
                change(design);
            }
        }
        self.bump();
    }

    pub fn edit_text(&self, id: Uuid, change: impl FnOnce(&mut crate::schedule_design::TextStyle)) {
        self.edit_element(id, |element| {
            if let ElementKind::Text(style) = &mut element.kind {
                change(style);
            }
        });
    }

    /// The text of a text box.
    pub fn text_of(&self, id: Uuid) -> Option<String> {
        let library = self.library.peek();
        match &library.active_design()?.element(id)?.kind {
            ElementKind::Text(style) => Some(style.text.clone()),
            _ => None,
        }
    }

    /// The uploaded and built-in pictures of the open design: `(id, name)`.
    pub fn picture_names(&self) -> Vec<(Uuid, String)> {
        self.library
            .read()
            .active_design()
            .map(|design| design.images.iter().map(|image| (image.id, image.name.clone())).collect())
            .unwrap_or_default()
    }

    /// Position, size and rotation of an element, and whether it is a picture.
    pub fn geometry(&self, id: Uuid) -> Option<(crate::design_geometry::Geometry, bool)> {
        let library = self.library.peek();
        let element = library.active_design()?.element(id)?;
        Some((
            crate::design_geometry::Geometry {
                x: element.x,
                y: element.y,
                w: element.w,
                h: element.h,
                rotation: element.rotation,
            },
            matches!(element.kind, ElementKind::Image(_)),
        ))
    }

    pub fn set_geometry(&self, id: Uuid, geometry: crate::design_geometry::Geometry) {
        let round = |value: f32| (value * 100.0).round() / 100.0;
        self.edit_element(id, |element| {
            element.x = round(geometry.x);
            element.y = round(geometry.y);
            element.w = round(geometry.w);
            element.h = round(geometry.h);
            element.rotation = round(geometry.rotation);
        });
    }

    pub fn edit_element(&self, id: Uuid, change: impl FnOnce(&mut DesignElement)) {
        self.edit(|design| {
            if let Some(element) = design.element_mut(id) {
                change(element);
            }
        });
    }

    fn used_bytes(&self) -> usize {
        self.library.read().designs.iter().map(ScheduleDesign::approx_bytes).sum()
    }

    fn add_design(&self, design: ScheduleDesign) {
        if self.library.read().designs.len() >= MAX_DESIGNS {
            return self.toasts.error("Too many designs", format!("You can keep {MAX_DESIGNS} designs. Delete one first."));
        }
        if !self.is_cloud && self.used_bytes() + design.approx_bytes() > store::STORAGE_BUDGET {
            return self.toasts.error("Not enough space", "The browser storage is nearly full. Remove pictures or designs first.");
        }
        let mut library = self.library;
        {
            let mut library = library.write();
            library.designs.push(design);
            library.active = library.designs.len() - 1;
        }
        if self.is_cloud {
            let mut cloud = self.cloud;
            cloud.write().entries.push(CloudState::unsaved());
        }
        let mut selected = self.selected;
        selected.set(None);
        self.bump();
    }

    fn switch_to(&self, index: usize) {
        let mut library = self.library;
        library.write().active = index;
        let mut selected = self.selected;
        selected.set(None);
        self.bump();
    }

    fn delete_active(&self) {
        let mut library = self.library;
        {
            let mut library = library.write();
            let active = library.active;
            if active < library.designs.len() {
                library.designs.remove(active);
            }
            library.active = library.active.saturating_sub(1).min(library.designs.len().saturating_sub(1));
        }
        let mut selected = self.selected;
        selected.set(None);
        self.bump();
    }

    pub fn add_element(&self, element: DesignElement) {
        let id = element.id;
        let count = self.active().map_or(0, |design| design.elements.len());
        if count >= MAX_ELEMENTS {
            return self.toasts.error("Too many elements", format!("A sheet can have {MAX_ELEMENTS} elements."));
        }
        self.edit(|design| design.elements.push(element));
        let mut selected = self.selected;
        selected.set(Some(id));
    }

    pub fn duplicate_element(&self, id: Uuid) {
        let Some(mut copy) = self.active().and_then(|design| design.element(id).cloned()) else { return };
        copy.id = Uuid::new_v4();
        copy.name = format!("{} copy", copy.name);
        copy.x = (copy.x + 3.0).min(95.0);
        copy.y = (copy.y + 3.0).min(95.0);
        self.add_element(copy);
    }

    pub fn delete_element(&self, id: Uuid) {
        self.edit(|design| design.elements.retain(|element| element.id != id));
        let mut selected = self.selected;
        selected.set(None);
    }

    /// A built-in graphic (picture, icon, decoration) as a new element.
    pub fn add_graphic(&self, key: &str) {
        let mut new_id = None;
        self.edit(|design| {
            if design.elements.len() < MAX_ELEMENTS {
                let width = if key.starts_with("icon_") { 8.0 } else { 25.0 };
                let element = design.graphic(key, 10.0, 10.0, width, 1.0);
                new_id = Some(element.id);
                design.elements.push(element);
            }
        });
        if let Some(id) = new_id {
            let mut selected = self.selected;
            selected.set(Some(id));
        }
    }

    /// A new text box in the middle of the sheet, selected and ready to type.
    pub fn add_text_box(&self) {
        let element = DesignElement::text("Text", "Text", 15.0, 20.0, 70.0, 10.0, 4.0);
        let id = element.id;
        self.add_element(element);
        // The script runs on its own, also after the panel that started it is gone (the text box
        // is selected now, so the panel with the text field replaces this one). It waits for the
        // text field to appear.
        let _ = document::eval(
            "for (let i = 0; i < 20; i++) { const el = document.getElementById('design-text-editor'); if (el) { el.focus(); el.select(); return; } await new Promise((r) => setTimeout(r, 60)); }",
        );
        let _ = id;
    }

    pub fn add_shape(&self) {
        self.add_element(DesignElement::shape("Shape", "#ffffff", 15.0, 20.0, 40.0, 20.0, 0.0));
    }

    /// A picture from the computer as a new element in the middle of the sheet.
    pub fn add_picture(&self, picked: store::PickedImage) {
        let (width, height) = (picked.width as f32, picked.height as f32);
        let name = picked.name.clone();
        let Some(image) = self.add_image(picked) else { return };
        let mut new_id = None;
        self.edit(|design| {
            if design.elements.len() < MAX_ELEMENTS {
                let w = 50.0;
                let h = w * (height / width) / design.ratio;
                let mut element = DesignElement::image(&name, Some(image), 25.0, (50.0 - h / 2.0).max(2.0), w, h);
                if let ElementKind::Image(style) = &mut element.kind {
                    style.fit = crate::schedule_design::Fit::Contain;
                }
                new_id = Some(element.id);
                design.elements.push(element);
            }
        });
        if let Some(id) = new_id {
            let mut selected = self.selected;
            selected.set(Some(id));
        }
    }

    /// Stores an uploaded picture in the design. Returns its id.
    pub fn add_image(&self, picked: store::PickedImage) -> Option<Uuid> {
        let design = self.active()?;
        if design.uploaded_images().count() >= MAX_IMAGES {
            self.toasts.error("Too many pictures", format!("A design can hold {MAX_IMAGES} pictures."));
            return None;
        }
        if self.is_cloud {
            // The backend limits the pictures of the whole project, and takes 2 MiB per picture.
            if let Some(reason) = self.picture_quota_error(picked.data_url.len() / 4 * 3) {
                self.toasts.error("Cannot add the picture", reason);
                return None;
            }
        } else if self.used_bytes() + picked.data_url.len() > store::STORAGE_BUDGET {
            self.toasts.error(
                "Not enough space",
                "The browser storage is nearly full. Remove pictures you do not need and try again.",
            );
            return None;
        }
        let id = Uuid::new_v4();
        self.edit(|design| {
            design.images.push(DesignImage::new(id, picked.name, picked.data_url));
        });
        Some(id)
    }
}

// ─────────────────────────────────────────────
//  Page
// ─────────────────────────────────────────────

#[component]
pub fn Schedules(cook_and_run_id: ReadSignal<Uuid>) -> Element {
    let storage = use_storage();
    let data = use_resource(move || {
        let id = cook_and_run_id();
        async move {
            let _ = storage.revision();
            let bundle = load_bundle(storage, id).await?;
            // The designs of a cloud project are in the backend; a local project's are read by the page.
            let designs = if bundle.is_cloud {
                let backend = storage.read().await;
                Some(backend.load_design_library(id).await.map_err(String::from)?)
            } else {
                None
            };
            Ok::<_, String>(PageData { bundle, designs })
        }
    });
    let id = cook_and_run_id();
    render_resource(
        data,
        "Could not load the plan data. You may need to log in or the servers may be offline.",
        move |PageData { bundle, designs }| {
            rsx! { SchedulesPage { key: "{id}", cook_and_run_id: id, bundle, designs } }
        },
    )
}

/// What the page needs from the backend before it can start.
#[derive(Clone)]
struct PageData {
    bundle: PlanBundle,
    /// The designs of the project if it is in the cloud.
    designs: Option<CloudLibrary>,
}

fn preset_by_key(key: &str) -> ScheduleDesign {
    match Preset::from_key(key) {
        Some(preset) => preset.build(),
        None => ScheduleDesign::blank("New design"),
    }
}

#[component]
fn SchedulesPage(cook_and_run_id: Uuid, bundle: PlanBundle, designs: Option<CloudLibrary>) -> Element {
    let toasts = use_toasts();
    let storage = use_storage();
    let is_cloud = bundle.is_cloud;
    let missing_images = designs.as_ref().map_or(0, |loaded| loaded.missing_images);
    let (first_library, first_cloud) = match designs {
        Some(loaded) => cloud::editor_state(loaded),
        None => (store::load(cook_and_run_id), CloudState::default()),
    };
    let library = use_signal(move || first_library);
    let cloud = use_signal(move || first_cloud);
    let version = use_signal(|| 0u32);
    let selected = use_signal(|| None::<Uuid>);
    let conflict = use_signal(|| None::<Uuid>);
    let ds = Designer {
        library,
        version,
        selected,
        toasts,
        is_cloud,
        project: cook_and_run_id,
        storage,
        cloud,
        conflict,
    };
    use_context_provider(|| ds);

    // The language of the sheets: the one of the browser until the organizer picks another.
    let mut language = use_signal(browser_language);
    let info = SheetInfo::new(bundle.project_name.clone(), Some(bundle.date), language());

    // Local project: store the designs shortly after the last change (a slider produces many
    // changes). A cloud project saves on request.
    use_effect(move || {
        let current = version();
        if current == 0 || is_cloud {
            return;
        }
        spawn(async move {
            TimeoutFuture::new(500).await;
            if *version.peek() == current {
                let snapshot = library.peek().clone();
                if let Err(e) = store::save(cook_and_run_id, &snapshot) {
                    toasts.error("The design could not be stored", e);
                }
            }
        });
    });

    let design = ds.active();
    let export_design = design.clone();
    // Width of the off-screen sheet (the designs scale with it).
    let export_width = 640;

    // ── Download all plans ───────────────────────────────────────────────────
    let export_task = use_busy();
    let export_sheet = use_signal(|| None::<ExportSheet>);
    let progress = use_signal(|| None::<(usize, usize)>);
    let has_plan = bundle.plan.is_some();
    let has_design = design.is_some();
    let teams: Vec<(Uuid, String)> = {
        let mut teams: Vec<_> = bundle.taking_part().map(|team| (team.id, team.name.clone())).collect();
        teams.sort_by_key(|(_, name)| name.to_lowercase());
        teams
    };
    let source_bundle = bundle.clone();
    let on_download_all = move |_| {
        let Some(plan) = source_bundle.plan.clone() else { return };
        if !has_design {
            return;
        }
        let source = Source {
            cook_and_run_id,
            project_name: source_bundle.project_name.clone(),
            german: language.peek().clone() == Language::German,
            is_cloud: source_bundle.is_cloud,
            plan,
            start: source_bundle.start.clone(),
            end: source_bundle.end.clone(),
            teams: teams.clone(),
            width: EXPORT_WIDTH,
        };
        export_task.run(async move {
            let result = download_all(storage, source, export_sheet, progress).await;
            // The off-screen sheet and the progress are only needed while the download runs.
            let (mut export_sheet, mut progress) = (export_sheet, progress);
            export_sheet.set(None);
            progress.set(None);
            match result {
                Ok((done, 0)) => toasts.success("Plans downloaded", format!("{done} schedules are in the ZIP file.")),
                Ok((done, skipped)) => toasts.info(
                    "Plans downloaded",
                    format!("{done} schedules are in the ZIP file. {skipped} team(s) are not part of the plan and were left out."),
                ),
                Err(e) => toasts.error("The plans could not be downloaded", e),
            }
        });
    };
    let download_label = match progress() {
        Some((done, total)) => format!("Downloading {}/{total}…", (done + 1).min(total)),
        None => "Download all plans".to_string(),
    };
    let download_all_button = rsx! {
        span { title: if !has_plan { "Calculate a plan first" } else if !has_design { "Create a design first" } else { "One image per team, packed into a ZIP file" },
            Button {
                label: download_label,
                variant: ButtonVariant::Secondary,
                loading: export_task.is_busy(),
                disabled: !has_plan || !has_design,
                onclick: on_download_all,
            }
        }
    };

    rsx! {
        PageShell {
            PageHeader {
                title: "Schedules",
                subtitle: "Design the schedule sheet each team receives. Everything is edited on the sheet itself.".to_string(),
                actions: download_all_button,
            }
            if bundle.is_cloud {
                // What the Designer knows is current: it changes without a reload.
                NoDesignNotice {
                    cook_and_run_id,
                    in_use: Some(ds.cloud.read().active.is_some()),
                    on_schedules_page: true,
                }
            }
            DesignBar { has_design: design.is_some(), missing_images }
            match design {
                None => rsx! {
                    EmptyState {
                        title: "No design yet",
                        text: if bundle.is_cloud {
                            "Create a design from a template above, or import a design file. Until a design is in use, teams do not see their plan."
                        } else {
                            "Create a design from a template above, or import a design file. Until then there is no sheet."
                        },
                    }
                },
                Some(design) => rsx! {
                    Editor {
                        cook_and_run_id,
                        bundle: bundle.clone(),
                        design,
                        info: info.clone(),
                        onlanguage: move |value: Language| language.set(value),
                    }
                },
            }
        }

        // The sheet of the team that is being exported: outside of the screen, but drawn like a
        // normal sheet (the picture is made from it).
        if let Some(current) = export_sheet() {
            div {
                key: "{current.key}",
                id: EXPORT_HOST_ID,
                "data-key": "{current.key}",
                "aria-hidden": "true",
                style: "position:fixed; left:-20000px; top:0; width:{export_width}px; pointer-events:none;",
                if let Some(design) = export_design {
                    DesignedSchedule {
                        design,
                        config: info.clone(),
                        schedule: current.schedule.clone(),
                        sheet_id: Some(EXPORT_SHEET_ID.to_string()),
                    }
                }
            }
        }
    }
}

#[component]
fn Editor(
    cook_and_run_id: Uuid,
    bundle: PlanBundle,
    design: ScheduleDesign,
    info: SheetInfo,
    onlanguage: EventHandler<Language>,
) -> Element {
    let ds = use_context::<Designer>();
    let storage = use_storage();
    let mut preview_team = use_signal(|| None::<Uuid>);
    let download_task = use_busy();
    let toasts = ds.toasts;

    let preview = use_resource(move || {
        let team = preview_team();
        async move {
            match team {
                Some(team) => load_team_schedule(storage, cook_and_run_id, team).await,
                None => Ok(None),
            }
        }
    });
    let german = info.german();
    let (schedule, preview_error) = match &*preview.read_unchecked() {
        Some(Ok(Some(schedule))) => (schedule.clone(), None),
        Some(Err(e)) => (Schedule::sample(german), Some(e.clone())),
        _ => (Schedule::sample(german), None),
    };
    let teams: Vec<(Uuid, String)> = {
        let mut teams: Vec<_> = bundle.taking_part().map(|team| (team.id, team.name.clone())).collect();
        teams.sort_by_key(|(_, name)| name.to_lowercase());
        teams
    };
    let has_plan = bundle.plan.is_some();
    let selected_id = *ds.selected.read();
    let selected_element = selected_id.and_then(|id| design.element(id).cloned());
    // <project>_<team>; with the example data there is no team, the design takes its place.
    let preview_name = preview_team().and_then(|id| teams.iter().find(|(team, _)| *team == id).map(|(_, name)| name.clone()));
    let file_name = file_stem(&bundle.project_name, preview_name.as_deref().unwrap_or(&design.name));
    let courses = bundle.courses.len();

    rsx! {
        div { class: "grid grid-cols-1 xl:grid-cols-[minmax(0,1fr)_22rem] gap-6 items-start",
            div { class: "space-y-4 min-w-0",
                // Always visible, also while an element is selected.
                InsertPanel {}
                Card { title: "Sheet",
                    div { class: "flex flex-wrap items-center gap-2",
                        LanguageTabs { value: info.language.clone(), onchange: move |value: Language| onlanguage.call(value) }
                        ChoiceSelect {
                            class: "w-auto",
                            disabled: !has_plan,
                            value: preview_team().map(|id| id.to_string()).unwrap_or_default(),
                            options: std::iter::once((String::new(), "Example data".to_string()))
                                .chain(teams.into_iter().map(|(id, name)| (id.to_string(), name)))
                                .collect::<Vec<_>>(),
                            onchange: move |value: String| preview_team.set(value.parse::<Uuid>().ok()),
                        }
                        Button {
                            label: "Download as image",
                            loading: download_task.is_busy(),
                            onclick: move |_| {
                                let name = file_name.clone();
                                download_task.run(async move {
                                    if let Err(e) = download_export(&name).await {
                                        toasts.error("The image could not be created", e);
                                    }
                                });
                            },
                        }
                    }
                    if !has_plan {
                        p { class: "text-xs text-zinc-500", "There is no plan yet, so the preview shows example data." }
                    }
                    if let Some(error) = preview_error {
                        p { class: "text-xs text-red-600", "The team could not be loaded, showing example data: {error}" }
                    }
                    Stage { design: design.clone(), config: info.clone(), schedule }
                    p { class: "text-center text-xs text-zinc-500",
                        "Click to select, drag to move, squares to resize, circle to rotate. Arrow keys nudge, Delete removes."
                    }
                }
            }

            div { class: "space-y-4 min-w-0",
                match selected_element {
                    Some(element) => rsx! {
                        Inspector { key: "{element.id}", element, pictures: ds.picture_names(), courses, german }
                    },
                    None => rsx! {
                        SheetPanel { design: design.clone() }
                    },
                }
            }
        }
    }
}

async fn download_export(name: &str) -> Result<(), String> {
    download_design(name, EXPORT_WIDTH).await
}

// ─────────────────────────────────────────────
//  Designs: choose, create, rename, export, import
// ─────────────────────────────────────────────

#[component]
fn DesignBar(has_design: bool, #[props(default)] missing_images: usize) -> Element {
    let ds = use_context::<Designer>();
    let toasts = ds.toasts;
    let import_task = use_busy();
    let save_task = use_busy();
    let mut preset = use_signal(|| Preset::ALL[0].key().to_string());
    let mut confirm_delete = use_signal(|| false);

    let (names, active) = {
        let library = ds.library.read();
        (library.designs.iter().map(|design| design.name.clone()).collect::<Vec<_>>(), library.active)
    };
    let active_name = names.get(active).cloned().unwrap_or_default();
    let used = ds.used_bytes();
    let percent = used * 100 / store::STORAGE_BUDGET;
    let is_cloud = ds.is_cloud;
    let dirty = is_cloud && ds.is_dirty(active);
    let stored = is_cloud && ds.is_saved(active);
    let used_by_teams = is_cloud && ds.is_used_by_teams(active);
    let open_id = ds.open_id();
    let (picture_count, picture_bytes) = if is_cloud { ds.project_pictures() } else { (0, 0) };

    rsx! {
        Card { title: "Designs",
            if !names.is_empty() {
                div { class: "flex flex-wrap gap-2",
                    for (index, name) in names.iter().cloned().enumerate() {
                        ToggleButton {
                            key: "{index}",
                            large: true,
                            on: index == active,
                            title: if is_cloud && ds.is_used_by_teams(index) { "Teams see this design" } else { "" },
                            onclick: move |_| ds.switch_to(index),
                            if is_cloud && ds.is_used_by_teams(index) { "★ " }
                            "{name}"
                            if is_cloud && ds.is_dirty(index) { span { class: "text-amber-600", title: "Not saved", " ●" } }
                        }
                    }
                }
                if is_cloud {
                    Hint { "Designs of this project are stored in the cloud when you save them. Teams see the design marked ★ (its saved version)." }
                } else {
                    Hint {
                        "The open design is used for the schedule sheets (Teams → team plan) on this device. Designs are stored in this browser; "
                        "export a design to keep or share it."
                    }
                }
            }
            if is_cloud && has_design {
                div { class: "flex flex-wrap items-center gap-2",
                    Button {
                        label: if dirty || !stored { "Save to cloud" } else { "Saved" },
                        loading: save_task.is_busy(),
                        disabled: stored && !dirty,
                        onclick: move |_| save_task.run(async move { ds.save_open_with_toast().await }),
                    }
                    if used_by_teams {
                        Button {
                            label: "Stop using for teams",
                            variant: ButtonVariant::Secondary,
                            loading: save_task.is_busy(),
                            onclick: move |_| save_task.run(async move { ds.set_used_by_teams(None).await }),
                        }
                    } else {
                        span { title: if stored { "Teams will see the saved version of this design" } else { "Save the design first" },
                            Button {
                                label: "Use for teams",
                                variant: ButtonVariant::Secondary,
                                loading: save_task.is_busy(),
                                disabled: !stored,
                                onclick: move |_| {
                                    if let Some(id) = open_id {
                                        save_task.run(async move { ds.set_used_by_teams(Some(id)).await });
                                    }
                                },
                            }
                        }
                    }
                    if dirty && stored {
                        span { class: "text-xs text-amber-700", "Unsaved changes. Teams only see what is saved." }
                    } else if !stored {
                        span { class: "text-xs text-amber-700", "This design is not in the cloud yet." }
                    }
                }
            }
            div { class: "flex flex-wrap items-end gap-2",
                div { class: "w-56",
                    Select {
                        value: preset(),
                        options: vec![
                            (Preset::PizzaParty.key(), Preset::PizzaParty.label()),
                            (Preset::Sunny.key(), Preset::Sunny.label()),
                            (Preset::LineArt.key(), Preset::LineArt.label()),
                            ("blank", "Blank sheet"),
                        ],
                        onchange: move |value: String| preset.set(value),
                    }
                }
                Button {
                    label: "New design",
                    onclick: move |_| ds.add_design(preset_by_key(&preset.read())),
                }
                if has_design {
                    Button {
                        label: "Duplicate",
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| {
                            if let Some(mut copy) = ds.active() {
                                copy.name = format!("{} copy", copy.name).chars().take(60).collect();
                                ds.add_design(copy);
                            }
                        },
                    }
                    Button {
                        label: "Export",
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| {
                            let Some(design) = ds.active() else { return };
                            spawn(async move {
                                // Pictures of the cloud are loaded into the file.
                                let result = match cloud::with_inline_pictures(design).await {
                                    Ok(design) => design.to_json().map(|json| (design.name, json)),
                                    Err(e) => Err(e),
                                };
                                match result {
                                    Ok((name, json)) => {
                                        let name = name.replace(['/', '\\', ' '], "_");
                                        if let Err(e) = download_text(&format!("{name}.json"), &json) {
                                            toasts.error("The design could not be exported", e);
                                        }
                                    }
                                    Err(e) => toasts.error("The design could not be exported", e),
                                }
                            });
                        },
                    }
                    Button {
                        label: "Delete",
                        variant: ButtonVariant::Danger,
                        onclick: move |_| confirm_delete.set(true),
                    }
                }
                label {
                    r#for: "design-import-input",
                    class: button_class(ButtonVariant::Secondary),
                    if import_task.is_busy() { "Importing…" } else { "Import" }
                }
                FileInput {
                    id: "design-import-input",
                    accept: ".json,application/json",
                    onpick: move |_| {
                        import_task.run(async move {
                            match store::read_text("design-import-input").await {
                                Ok(Some(json)) => match ScheduleDesign::from_json(&json) {
                                    Ok(design) => ds.add_design(design),
                                    Err(e) => toasts.error("The design could not be imported", e),
                                },
                                Ok(None) => {}
                                Err(e) => toasts.error("The design could not be imported", e),
                            }
                        });
                    },
                }
            }
            if has_design {
                Field { label: "Name of this design",
                    TextInput {
                        value: active_name.clone(),
                        oninput: move |e: Event<FormData>| {
                            let name: String = e.value().chars().take(60).collect();
                            ds.edit(|d| d.name = name);
                        },
                    }
                }
            }
            if is_cloud && picture_count > 0 {
                p { class: "text-xs text-zinc-500",
                    "Pictures of the project: {picture_count} of {MAX_IMAGES}, {picture_bytes / 1024} KiB of 10 MiB (all designs together)."
                }
            }
            if missing_images > 0 {
                p { class: "text-xs text-red-600",
                    "{missing_images} picture(s) could not be loaded from the cloud and are shown empty. Reload the page to try again."
                }
            }
            if !is_cloud && percent >= 70 {
                p { class: "text-xs text-amber-700",
                    "The browser storage for designs is {percent}% full. Remove pictures you do not need, or use smaller ones."
                }
            }
        }
        if confirm_delete() {
            Dialog { title: "Delete this design?", onclose: move |_| confirm_delete.set(false),
                if is_cloud {
                    p { class: "text-sm text-zinc-700",
                        "\"{active_name}\" is deleted from the cloud (also for the teams, if they see it). Pictures that no other design uses are deleted with it. Export it first if you want to keep it."
                    }
                } else {
                    p { class: "text-sm text-zinc-700", "\"{active_name}\" and its pictures are removed from this browser. Export it first if you want to keep it." }
                }
                div { class: "flex flex-wrap justify-end gap-2",
                    Button { label: "Keep it", variant: ButtonVariant::Secondary, onclick: move |_| confirm_delete.set(false) }
                    Button {
                        label: "Delete",
                        variant: ButtonVariant::Danger,
                        loading: save_task.is_busy(),
                        onclick: move |_| {
                            confirm_delete.set(false);
                            if is_cloud {
                                save_task.run(async move { ds.delete_open_design().await });
                            } else {
                                ds.delete_active();
                            }
                        },
                    }
                }
            }
        }
        if let Some(conflicting) = (ds.conflict)() {
            Dialog { title: "Newer version in the cloud", onclose: move |_| { let mut conflict = ds.conflict; conflict.set(None); },
                p { class: "text-sm text-zinc-700",
                    "This design was saved on another device or in another tab since you opened it. "
                    "Choose which version to keep; nothing is overwritten without your choice."
                }
                div { class: "flex flex-wrap justify-end gap-2",
                    Button {
                        label: "Cancel",
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| { let mut conflict = ds.conflict; conflict.set(None); },
                    }
                    Button {
                        label: "Load the saved version",
                        variant: ButtonVariant::Secondary,
                        loading: save_task.is_busy(),
                        onclick: move |_| save_task.run(async move { ds.reload_conflicting().await }),
                    }
                    Button {
                        label: "Overwrite with my version",
                        variant: ButtonVariant::Danger,
                        loading: save_task.is_busy(),
                        onclick: move |_| save_task.run(async move { ds.overwrite_conflicting().await }),
                    }
                }
                span { class: "hidden", "{conflicting}" }
            }
        }
    }
}
