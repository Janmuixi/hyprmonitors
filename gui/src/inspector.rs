use crate::app::App;
use crate::theme;

pub fn render(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.selected else {
        empty_hint(ui);
        return;
    };
    if idx >= app.monitors.len() {
        app.selected = None;
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| render_selected(ui, app, idx));
}

/// Shown when no monitor is selected — a calm, centered prompt instead of a
/// bare left-aligned line of text.
fn empty_hint(ui: &mut egui::Ui) {
    ui.add_space(28.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("No monitor selected")
                .strong()
                .color(theme::TEXT),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Click a card on the canvas\nto edit its settings.")
                .small()
                .color(theme::TEXT_MUTED),
        );
    });
}

/// A small, muted, uppercase-feel section label with breathing room above it.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(title)
            .small()
            .strong()
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(3.0);
}

fn render_selected(ui: &mut egui::Ui, app: &mut App, idx: usize) {
    let internal = hyprmonitor::algo::primary::is_internal(&app.monitors[idx].connector_hint);
    let m = &mut app.monitors[idx];
    let mut dirty = false;

    // ---- Header: badge dot + name + EDID id -------------------------------
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        let dot = if internal { theme::OK } else { theme::ACCENT };
        ui.painter().circle_filled(rect.center(), 4.5, dot);
        ui.label(egui::RichText::new(&m.connector_hint).heading().strong());
    });
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        ui.label(
            egui::RichText::new(if internal { "Built-in display" } else { "External display" })
                .small()
                .color(theme::TEXT_MUTED),
        );
    });
    if let Some(id) = &m.edid_id {
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.label(egui::RichText::new(id).small().monospace().color(theme::TEXT_MUTED));
        });
    }
    ui.add_space(8.0);
    ui.separator();

    // ---- Resolution -------------------------------------------------------
    let mut resolutions: Vec<(u32, u32)> = m
        .available_modes
        .iter()
        .map(|mode| (mode.width, mode.height))
        .collect();
    resolutions.sort_by_key(|(w, h)| std::cmp::Reverse(*w as u64 * *h as u64));
    resolutions.dedup();
    let current_res = (m.chosen_mode.width, m.chosen_mode.height);

    section(ui, "RESOLUTION");
    let full = ui.available_width();
    egui::ComboBox::from_id_salt("res")
        .width(full)
        .selected_text(format!("{}×{}", current_res.0, current_res.1))
        .show_ui(ui, |ui| {
            for (w, h) in &resolutions {
                if ui
                    .selectable_label(current_res == (*w, *h), format!("{}×{}", w, h))
                    .clicked()
                {
                    if let Some(best) = m
                        .available_modes
                        .iter()
                        .filter(|mm| mm.width == *w && mm.height == *h)
                        .max_by(|a, b| {
                            a.refresh_hz
                                .partial_cmp(&b.refresh_hz)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .cloned()
                    {
                        m.chosen_mode = best;
                        dirty = true;
                    }
                }
            }
        });

    // ---- Refresh rate -----------------------------------------------------
    let mut hz_options: Vec<f64> = m
        .available_modes
        .iter()
        .filter(|mm| mm.width == current_res.0 && mm.height == current_res.1)
        .map(|mm| mm.refresh_hz)
        .collect();
    hz_options.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    hz_options.dedup_by(|a, b| (*a - *b).abs() < 1e-3);

    section(ui, "REFRESH RATE");
    let full = ui.available_width();
    egui::ComboBox::from_id_salt("hz")
        .width(full)
        .selected_text(format!("{:.3} Hz", m.chosen_mode.refresh_hz))
        .show_ui(ui, |ui| {
            for hz in &hz_options {
                if ui
                    .selectable_label(
                        (m.chosen_mode.refresh_hz - *hz).abs() < 1e-3,
                        format!("{:.3} Hz", hz),
                    )
                    .clicked()
                {
                    m.chosen_mode.refresh_hz = *hz;
                    dirty = true;
                }
            }
        });

    // ---- Scale (segmented) ------------------------------------------------
    section(ui, "SCALE");
    ui.horizontal(|ui| {
        for s in [1.0f64, 1.25, 1.5, 1.75, 2.0] {
            if ui
                .selectable_label((m.scale - s).abs() < 1e-6, format!("{}×", s))
                .clicked()
            {
                m.scale = s;
                dirty = true;
            }
        }
    });

    // ---- Position ---------------------------------------------------------
    section(ui, "POSITION");
    ui.horizontal(|ui| {
        let mut x = m.position.0;
        let mut y = m.position.1;
        ui.label(egui::RichText::new("X").color(theme::TEXT_MUTED));
        if ui.add(egui::DragValue::new(&mut x).speed(1.0)).changed() {
            m.position.0 = x;
            dirty = true;
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Y").color(theme::TEXT_MUTED));
        if ui.add(egui::DragValue::new(&mut y).speed(1.0)).changed() {
            m.position.1 = y;
            dirty = true;
        }
    });

    // ---- Rotation (segmented) --------------------------------------------
    section(ui, "ROTATION");
    ui.horizontal(|ui| {
        for r in [0u16, 90, 180, 270] {
            if ui.selectable_label(m.rotation == r, format!("{}°", r)).clicked() {
                m.rotation = r;
                dirty = true;
            }
        }
    });

    // ---- Options ----------------------------------------------------------
    section(ui, "OPTIONS");
    if ui.checkbox(&mut m.disabled, "Disable this monitor").changed() {
        dirty = true;
    }

    if dirty {
        app.dirty = true;
    }
}
