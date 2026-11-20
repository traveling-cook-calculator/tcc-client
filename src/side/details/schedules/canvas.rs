//! The sheet in the designer, edited like a slide: click to select, drag to move, the squares to
//! resize, the circle to rotate. The arithmetic is in `design_geometry`.
//!
//! * Shift while resizing a corner: the opposite of the default (pictures keep their proportions,
//!   other elements do not). Shift while rotating: steps of 15 degrees.
//! * Arrow keys nudge (Shift: bigger steps), Delete removes, Escape deselects, Ctrl+D duplicates.
//! * Double click on a text: jump to the text field.
//!
//! The selection frame is not part of the sheet (`#section-to-print`), so it is never in the
//! downloaded image.

use std::rc::Rc;

use dioxus::html::Key;
use dioxus::prelude::*;
use uuid::Uuid;

use super::super::run_schedule::design::{DesignedSchedule, ElementPress};
use super::super::run_schedule::{Schedule, SheetInfo};
use super::Designer;
use crate::design_geometry::{self, Canvas, Geometry, Guides};
use crate::schedule_design::ScheduleDesign;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Move,
    Resize { hx: i8, hy: i8 },
    Rotate,
}

/// A drag that is going on: where it started, how big the sheet is on the screen, how the element
/// looked before.
#[derive(Clone, Debug, PartialEq)]
struct Drag {
    id: Uuid,
    mode: Mode,
    start: (f64, f64),
    left: f64,
    top: f64,
    canvas: Canvas,
    origin: Geometry,
    is_image: bool,
}

const HANDLES: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)];
const ACCENT: &str = "#d67229";

fn resize_cursor(hx: i8, hy: i8) -> &'static str {
    match (hx, hy) {
        (-1, -1) | (1, 1) => "nwse-resize",
        (1, -1) | (-1, 1) => "nesw-resize",
        (0, _) => "ns-resize",
        _ => "ew-resize",
    }
}

#[component]
pub(super) fn Stage(design: ScheduleDesign, config: SheetInfo, schedule: Schedule) -> Element {
    let ds = use_context::<Designer>();
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);
    let mut drag = use_signal(|| None::<Drag>);
    let mut guides = use_signal(Guides::default);
    // The button is still down. The size of the sheet is read asynchronously, and the button may be
    // released before that is done.
    let mut pressed = use_signal(|| false);

    let selected = *ds.selected.read();
    let frame = selected.and_then(|id| design.element(id)).map(|element| {
        (
            element.id,
            Geometry { x: element.x, y: element.y, w: element.w, h: element.h, rotation: element.rotation },
        )
    });
    let ratio = design.ratio;

    let mut begin = move |id: Uuid, mode: Mode, start: (f64, f64)| {
        let Some((origin, is_image)) = ds.geometry(id) else { return };
        let Some(node) = mounted.peek().clone() else { return };
        pressed.set(true);
        spawn(async move {
            let Ok(rect) = node.get_client_rect().await else { return };
            if !*pressed.peek() || rect.size.width < 1.0 || rect.size.height < 1.0 {
                return;
            }
            drag.set(Some(Drag {
                id,
                mode,
                start,
                left: rect.origin.x,
                top: rect.origin.y,
                canvas: Canvas { width: rect.size.width, height: rect.size.height },
                origin,
                is_image,
            }));
        });
    };

    let mut on_move = move |client: (f64, f64), shift: bool| {
        let Some(current) = drag.peek().clone() else { return };
        let (dx, dy) = (client.0 - current.start.0, client.1 - current.start.1);
        match current.mode {
            Mode::Move => {
                let (geometry, lines) = design_geometry::move_by(current.origin, current.canvas, dx, dy, true);
                ds.set_geometry(current.id, geometry);
                guides.set(lines);
            }
            Mode::Resize { hx, hy } => {
                let corner = hx != 0 && hy != 0;
                let keep_ratio = corner && (current.is_image != shift);
                let geometry = design_geometry::resize(current.origin, current.canvas, hx, hy, dx, dy, keep_ratio);
                ds.set_geometry(current.id, geometry);
            }
            Mode::Rotate => {
                let center = design_geometry::center_on_screen(current.origin, current.canvas, current.left, current.top);
                let rotation = design_geometry::rotate(
                    current.origin.rotation,
                    center,
                    current.start,
                    client,
                    shift.then_some(15.0),
                );
                ds.set_geometry(current.id, Geometry { rotation, ..current.origin });
            }
        }
    };

    let mut end = move || {
        pressed.set(false);
        drag.set(None);
        guides.set(Guides::default());
    };

    let dragging = drag.read().clone();
    let cursor = match dragging.as_ref().map(|d| d.mode) {
        Some(Mode::Move) => "move",
        Some(Mode::Resize { hx, hy }) => resize_cursor(hx, hy),
        Some(Mode::Rotate) => "grabbing",
        None => "default",
    };
    let lines = guides.read().clone();

    rsx! {
        div {
            class: "relative mx-auto my-8 w-full max-w-[600px] outline-none",
            tabindex: "0",
            onmounted: move |e| mounted.set(Some(e.data())),
            onpointermove: move |e| {
                if drag.peek().is_some() {
                    let point = e.client_coordinates();
                    on_move((point.x, point.y), e.modifiers().shift());
                }
            },
            onpointerup: move |_| end(),
            onpointercancel: move |_| end(),
            onkeydown: move |e| {
                let Some(id) = *ds.selected.peek() else { return };
                let big = e.modifiers().shift();
                let step = if big { 2.0 } else { 0.5 };
                let nudge = |dx: f32, dy: f32| {
                    if let Some((mut geometry, _)) = ds.geometry(id) {
                        geometry.x += dx;
                        geometry.y += dy / ratio;
                        ds.set_geometry(id, geometry);
                    }
                };
                match e.key() {
                    Key::ArrowLeft => { e.prevent_default(); nudge(-step, 0.0) }
                    Key::ArrowRight => { e.prevent_default(); nudge(step, 0.0) }
                    Key::ArrowUp => { e.prevent_default(); nudge(0.0, -step) }
                    Key::ArrowDown => { e.prevent_default(); nudge(0.0, step) }
                    Key::Delete | Key::Backspace => { e.prevent_default(); ds.delete_element(id) }
                    Key::Escape => {
                        let mut selected = ds.selected;
                        selected.set(None);
                    }
                    Key::Character(c) if c.eq_ignore_ascii_case("d") && (e.modifiers().ctrl() || e.modifiers().meta()) => {
                        e.prevent_default();
                        ds.duplicate_element(id);
                    }
                    _ => {}
                }
            },

            div { class: "overflow-hidden rounded-xl shadow-lg ring-1 ring-amber-200",
                DesignedSchedule {
                    design,
                    config,
                    schedule,
                    selected,
                    onselect: move |id: Option<Uuid>| {
                        let mut selected = ds.selected;
                        selected.set(id);
                    },
                    onelementdown: move |press: ElementPress| {
                        let mut selected = ds.selected;
                        selected.set(Some(press.id));
                        begin(press.id, Mode::Move, (press.x, press.y));
                    },
                    ondoubleclick: move |_| {
                        // The text field of the selected text: ready to type.
                        spawn(async move {
                            let _ = document::eval(
                                "for (let i = 0; i < 10; i++) { const el = document.getElementById('design-text-editor'); if (el) { el.focus(); el.select(); return; } await new Promise((r) => setTimeout(r, 60)); }",
                            )
                            .await;
                        });
                    },
                }
            }

            // Selection frame, handles and guides: on top of the sheet, but not part of it.
            div { class: "pointer-events-none absolute inset-0",
                for line in lines.vertical.iter().copied() {
                    div { key: "v{line}", style: "position:absolute; top:0; bottom:0; left:{line}%; width:1px; background:#FF2D87;" }
                }
                for line in lines.horizontal.iter().copied() {
                    div { key: "h{line}", style: "position:absolute; left:0; right:0; top:{line}%; height:1px; background:#FF2D87;" }
                }
                if let Some((id, g)) = frame {
                    div {
                        style: "position:absolute; left:{g.x}%; top:{g.y}%; width:{g.w}%; height:{g.h}%; transform:rotate({g.rotation}deg);",
                        div { style: "position:absolute; inset:-1px; border:1.5px solid {ACCENT}; pointer-events:none;" }
                        // Stem and handle for rotating.
                        div { style: "position:absolute; left:50%; top:-26px; width:1.5px; height:26px; background:{ACCENT}; transform:translateX(-50%);" }
                        div {
                            style: "position:absolute; left:50%; top:-36px; width:16px; height:16px; margin-left:-8px; border-radius:9999px; background:#fff; border:2px solid {ACCENT}; cursor:grab; pointer-events:auto; touch-action:none;",
                            onpointerdown: move |e| {
                                e.stop_propagation();
                                let point = e.client_coordinates();
                                begin(id, Mode::Rotate, (point.x, point.y));
                            },
                        }
                        for (hx, hy) in HANDLES {
                            div {
                                key: "{hx}{hy}",
                                style: "position:absolute; left:{50 + 50 * hx as i32}%; top:{50 + 50 * hy as i32}%; width:12px; height:12px; margin:-6px 0 0 -6px; background:#fff; border:2px solid {ACCENT}; border-radius:2px; cursor:{resize_cursor(hx, hy)}; pointer-events:auto; touch-action:none;",
                                onpointerdown: move |e| {
                                    e.stop_propagation();
                                    let point = e.client_coordinates();
                                    begin(id, Mode::Resize { hx, hy }, (point.x, point.y));
                                },
                            }
                        }
                    }
                }
            }

            // While dragging, this covers the whole window, so that the pointer can leave the sheet.
            if dragging.is_some() {
                div {
                    class: "fixed inset-0 z-50",
                    style: "cursor:{cursor}; touch-action:none;",
                    onpointermove: move |e| {
                        let point = e.client_coordinates();
                        on_move((point.x, point.y), e.modifiers().shift());
                    },
                    onpointerup: move |_| end(),
                    onpointercancel: move |_| end(),
                }
            }
        }
    }
}
