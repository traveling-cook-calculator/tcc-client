//! The arithmetic behind moving, resizing and rotating an element of a schedule sheet with the
//! mouse or a finger. No UI in here, so that it can be tested.
//!
//! An element lies on a sheet of `canvas.width` x `canvas.height` pixels. Its position and size are
//! percentages of the sheet (x and width of the sheet width, y and height of the sheet height) and it
//! is rotated around its center.

/// Position, size and rotation of an element. Percent of the sheet, degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rotation: f32,
}

/// The sheet in pixels, as it is on the screen right now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Canvas {
    pub width: f64,
    pub height: f64,
}

/// Lines that are shown while an element snaps to the middle or an edge of the sheet (percent).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Guides {
    pub vertical: Vec<f32>,
    pub horizontal: Vec<f32>,
}

/// An element closer than this (percent of the sheet) to the middle or an edge snaps to it.
const SNAP_DISTANCE: f64 = 1.2;
/// Smallest size of an element in pixels.
const MIN_SIZE_PX: f64 = 8.0;

/// Snaps one axis: the start, middle and end of the element against 0, 50 and 100.
fn snap_axis(start: f64, size: f64) -> (f64, Option<f32>) {
    let mut best: Option<(f64, f64)> = None; // (distance, shift)
    for offset in [0.0, size / 2.0, size] {
        for target in [0.0, 50.0, 100.0] {
            let shift = target - (start + offset);
            if shift.abs() <= SNAP_DISTANCE
                && best.is_none_or(|(distance, _)| shift.abs() < distance)
            {
                best = Some((shift.abs(), shift));
            }
        }
    }
    match best {
        Some((_, shift)) => {
            let snapped = start + shift;
            let line = [0.0, size / 2.0, size]
                .iter()
                .map(|offset| snapped + offset)
                .find(|edge| [0.0, 50.0, 100.0].iter().any(|t| (edge - t).abs() < 1e-6))
                .map(|edge| ((edge / 50.0).round() * 50.0) as f32);
            (snapped, line)
        }
        None => (start, None),
    }
}

/// The element is dragged by (`dx`, `dy`) pixels.
pub fn move_by(
    start: Geometry,
    canvas: Canvas,
    dx: f64,
    dy: f64,
    snap: bool,
) -> (Geometry, Guides) {
    let mut x = start.x as f64 + dx / canvas.width * 100.0;
    let mut y = start.y as f64 + dy / canvas.height * 100.0;
    let mut guides = Guides::default();
    if snap {
        let (snapped, line) = snap_axis(x, start.w as f64);
        x = snapped;
        guides.vertical.extend(line);
        let (snapped, line) = snap_axis(y, start.h as f64);
        y = snapped;
        guides.horizontal.extend(line);
    }
    (
        Geometry {
            x: x as f32,
            y: y as f32,
            ..start
        },
        guides,
    )
}

/// A resize handle is dragged by (`dx`, `dy`) pixels. `hx`/`hy` say which handle: -1 left/top,
/// 1 right/bottom, 0 the middle (that axis does not change). The opposite side stays where it is,
/// also when the element is rotated.
pub fn resize(
    start: Geometry,
    canvas: Canvas,
    hx: i8,
    hy: i8,
    dx: f64,
    dy: f64,
    keep_ratio: bool,
) -> Geometry {
    let (hx, hy) = (hx.signum() as f64, hy.signum() as f64);
    let theta = (start.rotation as f64).to_radians();
    let (sin, cos) = theta.sin_cos();
    // The mouse movement along the axes of the element.
    let local_x = dx * cos + dy * sin;
    let local_y = -dx * sin + dy * cos;

    let w0 = start.w as f64 / 100.0 * canvas.width;
    let h0 = start.h as f64 / 100.0 * canvas.height;
    let mut w = (w0 + hx * local_x).max(MIN_SIZE_PX);
    let mut h = (h0 + hy * local_y).max(MIN_SIZE_PX);
    if keep_ratio && hx != 0.0 && hy != 0.0 {
        let scale = (w / w0).max(h / h0);
        w = (w0 * scale).max(MIN_SIZE_PX);
        h = (h0 * scale).max(MIN_SIZE_PX);
    } else if keep_ratio && hx != 0.0 {
        h = (h0 * w / w0).max(MIN_SIZE_PX);
    } else if keep_ratio && hy != 0.0 {
        w = (w0 * h / h0).max(MIN_SIZE_PX);
    }

    // The center moves by half of what the dragged side moved, in the rotated frame.
    let shift_x = hx * (w - w0) / 2.0;
    let shift_y = hy * (h - h0) / 2.0;
    let center_x = (start.x as f64 + start.w as f64 / 2.0) / 100.0 * canvas.width + shift_x * cos
        - shift_y * sin;
    let center_y = (start.y as f64 + start.h as f64 / 2.0) / 100.0 * canvas.height
        + shift_x * sin
        + shift_y * cos;

    Geometry {
        x: ((center_x - w / 2.0) / canvas.width * 100.0) as f32,
        y: ((center_y - h / 2.0) / canvas.height * 100.0) as f32,
        w: (w / canvas.width * 100.0) as f32,
        h: (h / canvas.height * 100.0) as f32,
        rotation: start.rotation,
    }
}

/// The new rotation when the rotate handle is dragged from `from` to `to`. `center` is the middle
/// of the element on the screen. With `step` the angle snaps to multiples of it (for example 15).
/// Without it, it still snaps when it is within 3 degrees of a multiple of 45.
pub fn rotate(
    start_rotation: f32,
    center: (f64, f64),
    from: (f64, f64),
    to: (f64, f64),
    step: Option<f32>,
) -> f32 {
    let angle = |p: (f64, f64)| (p.1 - center.1).atan2(p.0 - center.0).to_degrees();
    let mut rotation = start_rotation as f64 + angle(to) - angle(from);
    if let Some(step) = step {
        let step = step as f64;
        rotation = (rotation / step).round() * step;
    } else {
        let nearest = (rotation / 45.0).round() * 45.0;
        if (rotation - nearest).abs() <= 3.0 {
            rotation = nearest;
        }
    }
    // Into (-180, 180].
    rotation = (rotation + 180.0).rem_euclid(360.0) - 180.0;
    if rotation == -180.0 {
        rotation = 180.0;
    }
    rotation as f32
}

/// The middle of an element on the screen, given the screen position of the sheet.
pub fn center_on_screen(
    start: Geometry,
    canvas: Canvas,
    canvas_left: f64,
    canvas_top: f64,
) -> (f64, f64) {
    (
        canvas_left + (start.x as f64 + start.w as f64 / 2.0) / 100.0 * canvas.width,
        canvas_top + (start.y as f64 + start.h as f64 / 2.0) / 100.0 * canvas.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANVAS: Canvas = Canvas {
        width: 1000.0,
        height: 1250.0,
    };

    fn element(x: f32, y: f32, w: f32, h: f32, rotation: f32) -> Geometry {
        Geometry {
            x,
            y,
            w,
            h,
            rotation,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn moving_converts_pixels_to_percent_of_each_axis() {
        let (moved, guides) = move_by(
            element(10.0, 10.0, 20.0, 10.0, 0.0),
            CANVAS,
            100.0,
            125.0,
            false,
        );
        assert!(close(moved.x, 20.0) && close(moved.y, 20.0));
        assert_eq!(guides, Guides::default());
    }

    #[test]
    fn moving_snaps_to_the_middle_and_the_edges() {
        // The middle of the element (x + w/2) is at 49.5: it snaps to 50.
        let (moved, guides) = move_by(element(39.5, 10.0, 20.0, 10.0, 0.0), CANVAS, 0.0, 0.0, true);
        assert!(close(moved.x, 40.0));
        assert_eq!(guides.vertical, vec![50.0]);
        // The left edge is close to 0.
        let (moved, guides) = move_by(element(0.8, 30.0, 20.0, 10.0, 0.0), CANVAS, 0.0, 0.0, true);
        assert!(close(moved.x, 0.0));
        assert_eq!(guides.vertical, vec![0.0]);
        // Far from everything: no snapping.
        let (moved, guides) = move_by(element(10.0, 30.0, 20.0, 10.0, 0.0), CANVAS, 0.0, 0.0, true);
        assert!(close(moved.x, 10.0) && guides == Guides::default());
        // Switched off.
        let (moved, _) = move_by(element(0.8, 30.0, 20.0, 10.0, 0.0), CANVAS, 0.0, 0.0, false);
        assert!(close(moved.x, 0.8));
    }

    #[test]
    fn resizing_at_the_bottom_right_keeps_the_top_left() {
        let start = element(10.0, 10.0, 20.0, 10.0, 0.0);
        let resized = resize(start, CANVAS, 1, 1, 100.0, 125.0, false);
        assert!(close(resized.x, 10.0) && close(resized.y, 10.0));
        assert!(close(resized.w, 30.0) && close(resized.h, 20.0));
    }

    #[test]
    fn resizing_at_the_top_left_keeps_the_bottom_right() {
        let start = element(10.0, 10.0, 20.0, 10.0, 0.0);
        let resized = resize(start, CANVAS, -1, -1, -100.0, -125.0, false);
        assert!(close(resized.x + resized.w, 30.0) && close(resized.y + resized.h, 20.0));
        assert!(close(resized.w, 30.0) && close(resized.h, 20.0));
    }

    #[test]
    fn an_edge_handle_changes_only_one_axis() {
        let start = element(10.0, 10.0, 20.0, 10.0, 0.0);
        let resized = resize(start, CANVAS, 1, 0, 50.0, 400.0, false);
        assert!(close(resized.w, 25.0) && close(resized.h, 10.0) && close(resized.y, 10.0));
    }

    #[test]
    fn the_ratio_can_be_kept() {
        let start = element(10.0, 10.0, 20.0, 16.0, 0.0); // 200 x 200 px
        let resized = resize(start, CANVAS, 1, 1, 100.0, 10.0, true);
        let (w_px, h_px) = (
            resized.w as f64 / 100.0 * CANVAS.width,
            resized.h as f64 / 100.0 * CANVAS.height,
        );
        assert!((w_px - h_px).abs() < 0.5, "{w_px} x {h_px}");
        assert!(resized.w >= 29.9, "the larger of the two changes wins");
    }

    #[test]
    fn resizing_a_rotated_element_keeps_the_opposite_corner() {
        let start = element(30.0, 30.0, 20.0, 10.0, 90.0);
        let corner = |g: Geometry, sx: f64, sy: f64| {
            // the screen position of a corner (sx, sy in -1/1 of the half sizes)
            let (cx, cy) = center_on_screen(g, CANVAS, 0.0, 0.0);
            let (hw, hh) = (
                g.w as f64 / 100.0 * CANVAS.width / 2.0,
                g.h as f64 / 100.0 * CANVAS.height / 2.0,
            );
            let t = (g.rotation as f64).to_radians();
            (
                cx + sx * hw * t.cos() - sy * hh * t.sin(),
                cy + sx * hw * t.sin() + sy * hh * t.cos(),
            )
        };
        let before = corner(start, -1.0, -1.0);
        // The element is turned by 90 degrees: its right side points down. Dragging the right middle
        // handle down by 50 px makes it 50 px wider.
        let resized = resize(start, CANVAS, 1, 0, 0.0, 50.0, false);
        let after = corner(resized, -1.0, -1.0);
        assert!(
            (before.0 - after.0).abs() < 0.5 && (before.1 - after.1).abs() < 0.5,
            "{before:?} {after:?}"
        );
        assert!(close(resized.w, 25.0), "{}", resized.w);
    }

    #[test]
    fn nothing_gets_smaller_than_a_few_pixels() {
        let start = element(10.0, 10.0, 20.0, 10.0, 0.0);
        let resized = resize(start, CANVAS, 1, 1, -5000.0, -5000.0, false);
        assert!(resized.w as f64 / 100.0 * CANVAS.width >= MIN_SIZE_PX - 0.01);
        assert!(resized.h as f64 / 100.0 * CANVAS.height >= MIN_SIZE_PX - 0.01);
    }

    #[test]
    fn rotating_follows_the_pointer_around_the_center() {
        let center = (100.0, 100.0);
        // From straight above to the right of the center: a quarter turn clockwise.
        let rotation = rotate(0.0, center, (100.0, 0.0), (200.0, 100.0), None);
        assert!(close(rotation, 90.0), "{rotation}");
        // Starting rotation is kept.
        let rotation = rotate(30.0, center, (100.0, 0.0), (100.0, 0.0), Some(15.0));
        assert!(close(rotation, 30.0));
    }

    #[test]
    fn rotation_snaps_and_stays_in_range() {
        let center = (0.0, 0.0);
        // 44 degrees snaps to 45 without a step.
        let to = (
            44f64.to_radians().sin() * 100.0,
            -44f64.to_radians().cos() * 100.0,
        );
        let rotation = rotate(0.0, center, (0.0, -100.0), to, None);
        assert!(close(rotation, 45.0), "{rotation}");
        // With a step of 15: 100 degrees becomes 105.
        let rotation = rotate(100.0, center, (0.0, -100.0), (0.0, -100.0), Some(15.0));
        assert!(close(rotation, 105.0));
        // Over 180 wraps around.
        let rotation = rotate(170.0, center, (0.0, -100.0), (100.0, -100.0), None);
        assert!(
            rotation > -180.0 && rotation <= 180.0 && rotation < 0.0,
            "{rotation}"
        );
    }
}
