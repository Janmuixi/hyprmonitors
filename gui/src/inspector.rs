use crate::app::App;

pub fn render(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.selected else {
        ui.label("Click a monitor to edit it");
        return;
    };
    if idx >= app.monitors.len() {
        app.selected = None;
        return;
    }

    let m = &mut app.monitors[idx];
    ui.horizontal(|ui| {
        ui.label(format!("Selected: {}", m.connector_hint));
        if let Some(id) = &m.edid_id {
            ui.label(format!("[{}]", id));
        }
    });

    let mut resolutions: Vec<(u32, u32)> = m
        .available_modes
        .iter()
        .map(|mode| (mode.width, mode.height))
        .collect();
    resolutions.sort_by_key(|(w, h)| std::cmp::Reverse(*w as u64 * *h as u64));
    resolutions.dedup();

    let current_res = (m.chosen_mode.width, m.chosen_mode.height);

    let mut dirty = false;

    ui.horizontal(|ui| {
        ui.label("Resolution:");
        egui::ComboBox::from_id_salt("res")
            .selected_text(format!("{}×{}", current_res.0, current_res.1))
            .show_ui(ui, |ui| {
                for (w, h) in &resolutions {
                    if ui
                        .selectable_label(
                            current_res == (*w, *h),
                            format!("{}×{}", w, h),
                        )
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

        ui.label("Refresh:");
        let mut hz_options: Vec<f64> = m
            .available_modes
            .iter()
            .filter(|mm| mm.width == current_res.0 && mm.height == current_res.1)
            .map(|mm| mm.refresh_hz)
            .collect();
        hz_options.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        hz_options.dedup_by(|a, b| (*a - *b).abs() < 1e-3);

        egui::ComboBox::from_id_salt("hz")
            .selected_text(format!("{:.3}Hz", m.chosen_mode.refresh_hz))
            .show_ui(ui, |ui| {
                for hz in &hz_options {
                    if ui
                        .selectable_label(
                            (m.chosen_mode.refresh_hz - *hz).abs() < 1e-3,
                            format!("{:.3}Hz", hz),
                        )
                        .clicked()
                    {
                        m.chosen_mode.refresh_hz = *hz;
                        dirty = true;
                    }
                }
            });

        ui.label("Scale:");
        let scales = [1.0f64, 1.25, 1.5, 1.75, 2.0];
        egui::ComboBox::from_id_salt("scale")
            .selected_text(format!("{}", m.scale))
            .show_ui(ui, |ui| {
                for s in &scales {
                    if ui
                        .selectable_label((m.scale - *s).abs() < 1e-6, format!("{}", s))
                        .clicked()
                    {
                        m.scale = *s;
                        dirty = true;
                    }
                }
            });
    });

    ui.horizontal(|ui| {
        ui.label("Position:");
        let mut x = m.position.0;
        let mut y = m.position.1;
        if ui.add(egui::DragValue::new(&mut x).prefix("x=")).changed() {
            m.position.0 = x;
            dirty = true;
        }
        if ui.add(egui::DragValue::new(&mut y).prefix("y=")).changed() {
            m.position.1 = y;
            dirty = true;
        }

        ui.label("Rotation:");
        for r in [0u16, 90, 180, 270] {
            if ui.selectable_label(m.rotation == r, format!("{}°", r)).clicked() {
                m.rotation = r;
                dirty = true;
            }
        }

        if ui.checkbox(&mut m.disabled, "Disabled").changed() {
            dirty = true;
        }
    });

    if dirty {
        app.dirty = true;
    }
}
