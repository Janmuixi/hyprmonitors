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

/// Compute the AABB of all monitors in world coordinates.
pub fn world_bounds(monitors: &[EditableMonitor]) -> egui::Rect {
    if monitors.is_empty() {
        return egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1920.0, 1080.0));
    }
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for m in monitors {
        if m.disabled {
            continue;
        }
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

/// Render the canvas (rectangles + labels only — no interaction yet).
pub fn render(ui: &mut egui::Ui, app: &mut App) {
    let bounds = world_bounds(&app.monitors);
    let canvas_rect = ui.available_rect_before_wrap();

    // Auto-fit on first frame.
    if app.canvas_scale <= 0.0 {
        let sx = (canvas_rect.width() - 40.0) / bounds.width().max(1.0);
        let sy = (canvas_rect.height() - 40.0) / bounds.height().max(1.0);
        app.canvas_scale = sx.min(sy).max(0.01);
    }

    let painter = ui.painter_at(canvas_rect);
    let bcenter = bounds.center();
    let scale = app.canvas_scale;
    let to_screen = |wx: f32, wy: f32| -> egui::Pos2 {
        egui::pos2(
            canvas_rect.center().x + (wx - bcenter.x) * scale,
            canvas_rect.center().y + (wy - bcenter.y) * scale,
        )
    };

    painter.rect_filled(canvas_rect, 0.0, egui::Color32::from_gray(28));

    for (i, m) in app.monitors.iter().enumerate() {
        if m.disabled {
            continue;
        }
        let (w, h) = footprint(m);
        let top_left = to_screen(m.position.0 as f32, m.position.1 as f32);
        let bottom_right = to_screen(m.position.0 as f32 + w, m.position.1 as f32 + h);
        let rect = egui::Rect::from_two_pos(top_left, bottom_right);

        let selected = app.selected == Some(i);
        let fill = if selected {
            egui::Color32::from_rgb(60, 120, 200)
        } else {
            egui::Color32::from_rgb(70, 70, 80)
        };
        painter.rect_filled(rect, 4.0, fill);
        // egui 0.34: rect_stroke requires StrokeKind as 4th argument
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(2.0, egui::Color32::WHITE),
            egui::StrokeKind::Outside,
        );

        let internal = hyprmonitor::algo::primary::is_internal(&m.connector_hint);
        let label = format!(
            "{}{}\n{}×{} @{}\nscale {}",
            m.connector_hint,
            if internal { " (laptop)" } else { "" },
            m.chosen_mode.width,
            m.chosen_mode.height,
            m.chosen_mode.refresh_hz.round() as u32,
            m.scale,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::TextStyle::Body.resolve(ui.style()),
            egui::Color32::WHITE,
        );
    }
}
