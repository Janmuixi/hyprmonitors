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
    painter.rect_filled(canvas_rect, 0.0, crate::theme::BG);
    draw_dot_grid(&painter, canvas_rect);

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

    // Third pass: draw the monitor cards + labels.
    for (i, rect, internal) in &snapshot {
        let m = &app.monitors[*i];
        let selected = app.selected == Some(*i);
        draw_monitor_card(&painter, *rect, m, *internal, selected);
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

const CARD_RADIUS: f32 = 8.0;

/// A faint dot lattice behind the monitors, so the canvas reads as a workspace
/// rather than an empty void.
fn draw_dot_grid(painter: &egui::Painter, rect: egui::Rect) {
    let step = 30.0;
    let color = egui::Color32::from_rgba_unmultiplied(150, 153, 170, 16);
    let mut y = rect.top() + step;
    while y < rect.bottom() {
        let mut x = rect.left() + step;
        while x < rect.right() {
            painter.circle_filled(egui::pos2(x, y), 1.0, color);
            x += step;
        }
        y += step;
    }
}

/// Draw one monitor as a rounded card: soft shadow, accent selection ring, a
/// laptop/external badge dot, and a two-tier label (bold name, muted specs).
fn draw_monitor_card(
    painter: &egui::Painter,
    rect: egui::Rect,
    m: &EditableMonitor,
    internal: bool,
    selected: bool,
) {
    use crate::theme;

    // Soft drop shadow — stacked, expanding, low-alpha rects fake a blur.
    if !m.disabled {
        for i in 1..=3 {
            let e = i as f32 * 2.0;
            painter.rect_filled(
                rect.translate(egui::vec2(0.0, 3.0)).expand(e),
                CARD_RADIUS + e,
                egui::Color32::from_black_alpha(22),
            );
        }
    }

    let fill = if m.disabled {
        theme::CARD_DISABLED
    } else if selected {
        theme::CARD_SELECTED
    } else {
        theme::CARD
    };
    painter.rect_filled(rect, CARD_RADIUS, fill);

    // Selection glow: a wider, translucent accent ring behind the crisp one.
    if selected {
        painter.rect_stroke(
            rect.expand(3.0),
            CARD_RADIUS + 3.0,
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgba_unmultiplied(129, 140, 248, 70)),
            egui::StrokeKind::Outside,
        );
    }
    let (stroke_w, stroke_color): (f32, egui::Color32) = if selected {
        (2.0, theme::ACCENT)
    } else {
        (1.0, theme::BORDER)
    };
    painter.rect_stroke(
        rect,
        CARD_RADIUS,
        egui::Stroke::new(stroke_w, stroke_color),
        egui::StrokeKind::Inside,
    );

    // Corner badge dot: teal = laptop/internal, indigo = external.
    let badge = if m.disabled {
        theme::TEXT_MUTED
    } else if internal {
        theme::OK
    } else {
        theme::ACCENT
    };
    painter.circle_filled(rect.left_top() + egui::vec2(13.0, 13.0), 4.0, badge);

    // Skip labels on cards too small to hold them cleanly.
    if rect.width() < 54.0 || rect.height() < 40.0 {
        return;
    }

    let name_color = if m.disabled { theme::TEXT_MUTED } else { theme::TEXT };
    let mut title = m.connector_hint.clone();
    if internal {
        title.push_str("  (laptop)");
    }
    if m.disabled {
        title.push_str("  — off");
    }
    painter.text(
        rect.center() - egui::vec2(0.0, 13.0),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(15.0),
        name_color,
    );
    let details = format!(
        "{}×{}  ·  {} Hz\nscale {}",
        m.chosen_mode.width,
        m.chosen_mode.height,
        m.chosen_mode.refresh_hz.round() as u32,
        m.scale,
    );
    painter.text(
        rect.center() + egui::vec2(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        details,
        egui::FontId::proportional(11.5),
        theme::TEXT_MUTED,
    );
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
