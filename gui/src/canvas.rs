use crate::app::{App, EditableMonitor};

/// Effective on-screen size of a monitor, accounting for rotation.
pub fn footprint(m: &EditableMonitor) -> (f32, f32) {
    let w = (m.chosen_mode.width as f64 / m.scale) as f32;
    let h = (m.chosen_mode.height as f64 / m.scale) as f32;
    if m.rotation == 90 || m.rotation == 270 {
        (h, w)
    } else {
        (w, h)
    }
}

/// Compute the AABB of all monitors in world coordinates. Includes disabled
/// monitors so they remain visible on the canvas and the user can click to
/// re-enable them.
pub fn world_bounds(monitors: &[EditableMonitor]) -> egui::Rect {
    if monitors.is_empty() {
        return egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1920.0, 1080.0));
    }
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for m in monitors {
        let (w, h) = footprint(m);
        let x1 = m.position.0 as f32 + w;
        let y1 = m.position.1 as f32 + h;
        min_x = min_x.min(m.position.0);
        min_y = min_y.min(m.position.1);
        max_x = max_x.max(x1 as i32);
        max_y = max_y.max(y1 as i32);
    }
    if min_x == i32::MAX {
        return egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1920.0, 1080.0));
    }
    egui::Rect::from_min_max(
        egui::pos2(min_x as f32, min_y as f32),
        egui::pos2(max_x as f32, max_y as f32),
    )
}

/// Render the canvas with click-to-select and drag-to-reposition interaction.
pub fn render(ui: &mut egui::Ui, app: &mut App) {
    let bounds = world_bounds(&app.monitors);
    let canvas_rect = ui.available_rect_before_wrap();

    // Always recompute the fit so the layout stays optimally sized as the
    // window resizes or monitors move. The 60px margin leaves room around
    // the rectangles so they don't kiss the panel edges.
    let sx = (canvas_rect.width() - 60.0) / bounds.width().max(1.0);
    let sy = (canvas_rect.height() - 60.0) / bounds.height().max(1.0);
    app.canvas_scale = sx.min(sy).max(0.01);

    let to_screen = |wx: f32, wy: f32, scale: f32, center: egui::Pos2| -> egui::Pos2 {
        egui::pos2(
            canvas_rect.center().x + (wx - center.x) * scale,
            canvas_rect.center().y + (wy - center.y) * scale,
        )
    };

    let painter = ui.painter_at(canvas_rect);
    painter.rect_filled(canvas_rect, 0.0, egui::Color32::from_gray(28));

    let bg_id = ui.id().with("bg");
    let bg_response = ui.interact(canvas_rect, bg_id, egui::Sense::click());
    if bg_response.clicked() {
        app.selected = None;
    }

    let bcenter = bounds.center();
    let scale = app.canvas_scale;
    let mut click_target: Option<usize> = None;
    let mut drag_target: Option<(usize, egui::Vec2)> = None;
    let mut drag_stopped = false;

    // First pass: snapshot the rectangles for interaction + drawing. Disabled
    // monitors are kept in the snapshot — dimmed and not draggable, but still
    // clickable so the inspector can re-enable them.
    let snapshot: Vec<(usize, egui::Rect, bool)> = app
        .monitors
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (w, h) = footprint(m);
            let top_left = to_screen(m.position.0 as f32, m.position.1 as f32, scale, bcenter);
            let bottom_right = to_screen(
                m.position.0 as f32 + w,
                m.position.1 as f32 + h,
                scale,
                bcenter,
            );
            let rect = egui::Rect::from_two_pos(top_left, bottom_right);
            let internal = hyprmonitor::algo::primary::is_internal(&m.connector_hint);
            (i, rect, internal)
        })
        .collect();

    // Second pass: register interactions. Enabled monitors are click-and-drag;
    // disabled monitors are click-only — dragging a disabled rectangle would
    // be confusing since its position has no effect until it's re-enabled.
    for (i, rect, _internal) in &snapshot {
        let m = &app.monitors[*i];
        let sense = if m.disabled {
            egui::Sense::click()
        } else {
            egui::Sense::click_and_drag()
        };
        let response = ui.interact(*rect, ui.id().with(("monitor", *i)), sense);
        if response.clicked() {
            click_target = Some(*i);
        }
        if response.dragged() {
            click_target = Some(*i);
            drag_target = Some((*i, response.drag_delta()));
        }
        if response.drag_stopped() {
            drag_stopped = true;
        }
    }

    // Third pass: draw rectangles + labels.
    for (i, rect, internal) in &snapshot {
        let m = &app.monitors[*i];
        let selected = app.selected == Some(*i);
        let fill = if m.disabled {
            egui::Color32::from_rgb(35, 35, 40)
        } else if selected {
            egui::Color32::from_rgb(60, 120, 200)
        } else {
            egui::Color32::from_rgb(70, 70, 80)
        };
        let stroke_color = if m.disabled {
            egui::Color32::from_gray(100)
        } else {
            egui::Color32::WHITE
        };
        painter.rect_filled(*rect, 4.0, fill);
        painter.rect_stroke(
            *rect,
            4.0,
            egui::Stroke::new(2.0, stroke_color),
            egui::StrokeKind::Outside,
        );
        let label = format!(
            "{}{}{}\n{}×{} @{}\nscale {}",
            m.connector_hint,
            if *internal { " (laptop)" } else { "" },
            if m.disabled { " — disabled" } else { "" },
            m.chosen_mode.width,
            m.chosen_mode.height,
            m.chosen_mode.refresh_hz.round() as u32,
            m.scale,
        );
        let text_color = if m.disabled {
            egui::Color32::from_gray(160)
        } else {
            egui::Color32::WHITE
        };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::TextStyle::Body.resolve(ui.style()),
            text_color,
        );
    }

    if let Some(i) = click_target {
        app.selected = Some(i);
    }
    if let Some((i, delta)) = drag_target {
        let world_dx = (delta.x / scale) as i32;
        let world_dy = (delta.y / scale) as i32;
        if world_dx != 0 || world_dy != 0 {
            app.monitors[i].position.0 += world_dx;
            app.monitors[i].position.1 += world_dy;
            apply_snap(&mut app.monitors, i);
            app.dirty = true;
        }
    }
    if drag_stopped {
        align_all(&mut app.monitors);
        app.dirty = true;
    }
}

const SNAP_PX_DRAG: i32 = 20;
const SNAP_PX_ALIGN: i32 = 200;

/// Snap every monitor's edges to its neighbours within a generous threshold.
/// Processes left-to-right so later monitors see snapped earlier ones, which
/// lets a chain of small gaps collapse in a single pass.
pub(crate) fn align_all(monitors: &mut Vec<crate::app::EditableMonitor>) {
    let mut order: Vec<usize> = (0..monitors.len()).collect();
    order.sort_by_key(|&i| monitors[i].position.0);
    for i in order {
        apply_snap_with(monitors, i, SNAP_PX_ALIGN);
    }
}

fn apply_snap(monitors: &mut Vec<crate::app::EditableMonitor>, idx: usize) {
    apply_snap_with(monitors, idx, SNAP_PX_DRAG);
}

fn apply_snap_with(monitors: &mut Vec<crate::app::EditableMonitor>, idx: usize, snap_px: i32) {
    let me = monitors[idx].clone();
    let (me_w, me_h) = footprint(&me);
    let me_left = me.position.0 as f32;
    let me_top = me.position.1 as f32;
    let me_right = me_left + me_w;
    let me_bottom = me_top + me_h;

    let mut best_dx: Option<i32> = None;
    let mut best_dy: Option<i32> = None;

    for (j, other) in monitors.iter().enumerate() {
        if j == idx || other.disabled {
            continue;
        }
        let (ow, oh) = footprint(other);
        let o_left = other.position.0 as f32;
        let o_top = other.position.1 as f32;
        let o_right = o_left + ow;
        let o_bottom = o_top + oh;

        for (a, b) in [
            (me_right, o_left),
            (me_left, o_right),
            (me_left, o_left),
            (me_right, o_right),
        ] {
            let dx = (b - a).round() as i32;
            if dx.abs() <= snap_px && best_dx.map_or(true, |bd| dx.abs() < bd.abs()) {
                best_dx = Some(dx);
            }
        }
        for (a, b) in [
            (me_bottom, o_top),
            (me_top, o_bottom),
            (me_top, o_top),
            (me_bottom, o_bottom),
        ] {
            let dy = (b - a).round() as i32;
            if dy.abs() <= snap_px && best_dy.map_or(true, |bd| dy.abs() < bd.abs()) {
                best_dy = Some(dy);
            }
        }
    }

    if let Some(dx) = best_dx {
        monitors[idx].position.0 += dx;
    }
    if let Some(dy) = best_dy {
        monitors[idx].position.1 += dy;
    }
}
