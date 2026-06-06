use hyprmonitor::algo;
use hyprmonitor::config::{self, Config, MonitorOverride, Position};
use hyprmonitor::model::{Mode, Monitor};

#[derive(Debug, Clone)]
pub struct EditableMonitor {
    pub edid_id: Option<String>,
    pub connector_hint: String,
    pub available_modes: Vec<Mode>,
    pub physical_mm: Option<(u32, u32)>,
    pub chosen_mode: Mode,
    pub scale: f64,
    pub position: (i32, i32),
    pub rotation: u16,
    pub disabled: bool,
}

pub struct App {
    pub monitors: Vec<EditableMonitor>,
    pub selected: Option<usize>,
    pub canvas_scale: f32,
    pub canvas_offset: egui::Vec2,
    pub dirty: bool,
    pub last_error: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            monitors: Vec::new(),
            selected: None,
            canvas_scale: 0.05,
            canvas_offset: egui::Vec2::ZERO,
            dirty: false,
            last_error: None,
        }
    }

    /// Build EditableMonitor list from a live query: take what algo::plan
    /// produces, layered with overrides from the config file.
    pub fn load(&mut self, monitors: &[Monitor], cfg: &Config) {
        let mut plan = algo::plan(monitors);
        config::merge_into_plan(&mut plan, monitors, cfg);

        self.monitors = monitors
            .iter()
            .filter_map(|m| {
                let entry = plan.iter().find(|p| p.name == m.name)?;
                let cfg_entry = cfg.monitors.iter().find(|o| {
                    match (m.edid_id.as_deref(), o.edid_id.as_deref()) {
                        (Some(mid), Some(oid)) if mid == oid => true,
                        _ => o.connector_hint == m.name,
                    }
                });
                Some(EditableMonitor {
                    edid_id: m.edid_id.clone(),
                    connector_hint: m.name.clone(),
                    available_modes: m.available_modes.clone(),
                    physical_mm: m.physical_mm,
                    chosen_mode: entry.mode.clone(),
                    scale: entry.scale,
                    position: entry.position,
                    rotation: cfg_entry.map(|c| c.rotation).unwrap_or(0),
                    disabled: cfg_entry.map(|c| c.disabled).unwrap_or(false),
                })
            })
            .collect();
        self.selected = None;
        self.dirty = false;
        self.last_error = None;
    }

    pub fn to_config(&self) -> Config {
        Config {
            version: config::CURRENT_VERSION,
            monitors: self
                .monitors
                .iter()
                .map(|e| MonitorOverride {
                    edid_id: e.edid_id.clone(),
                    connector_hint: e.connector_hint.clone(),
                    position: Position { x: e.position.0, y: e.position.1 },
                    mode: format!(
                        "{}x{}@{}",
                        e.chosen_mode.width,
                        e.chosen_mode.height,
                        format_hz(e.chosen_mode.refresh_hz),
                    ),
                    scale: e.scale,
                    rotation: e.rotation,
                    disabled: e.disabled,
                })
                .collect(),
        }
    }
}

async fn reload_monitors() -> anyhow::Result<(Vec<hyprmonitor::model::Monitor>, hyprmonitor::config::Config)> {
    let monitors = crate::query_hyprctl_monitors().await?;
    let cfg = hyprmonitor::config::load_or_default(&crate::save::config_path());
    Ok((monitors, cfg))
}

fn format_hz(hz: f64) -> String {
    if (hz - hz.round()).abs() < 1e-6 {
        format!("{}", hz as u32)
    } else {
        let s = format!("{:.3}", hz);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// In eframe 0.34 the App trait changed from update(ctx, frame) to
// ui(ui, frame).  Panels must use show_inside(ui, …) instead of show(ctx, …).
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let (escape, ctrl_s, dx, dy) = ui.input(|i| {
            let escape = i.key_pressed(egui::Key::Escape);
            let ctrl_s = i.modifiers.ctrl && i.key_pressed(egui::Key::S);
            let dx = if i.key_pressed(egui::Key::ArrowRight) {
                1
            } else if i.key_pressed(egui::Key::ArrowLeft) {
                -1
            } else {
                0
            };
            let dy = if i.key_pressed(egui::Key::ArrowDown) {
                1
            } else if i.key_pressed(egui::Key::ArrowUp) {
                -1
            } else {
                0
            };
            (escape, ctrl_s, dx, dy)
        });

        let mut save_requested = false;
        let mut reset_requested = false;
        let mut reload_requested = false;

        let validation_error: Option<String> = crate::save::validate(&self.monitors)
            .err()
            .map(|e| e.to_string());
        if escape {
            self.selected = None;
        }
        if ctrl_s {
            save_requested = true;
        }
        if let Some(idx) = self.selected {
            if (dx != 0 || dy != 0) && idx < self.monitors.len() {
                self.monitors[idx].position.0 += dx;
                self.monitors[idx].position.1 += dy;
                self.dirty = true;
            }
        }
        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("hyprmonitor");
                ui.separator();
                ui.label(format!(
                    "{} monitors{}",
                    self.monitors.len(),
                    if self.dirty { " (unsaved)" } else { "" }
                ));
                if let Some(reason) = &validation_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("⚠ {}", reason));
                }
                if let Some(msg) = &self.last_error {
                    let color = if msg.contains('✓') {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::LIGHT_RED
                    };
                    ui.colored_label(color, msg);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save_button = egui::Button::new("\u{1f4be} Save & Apply");
                    let save_resp = ui.add_enabled(validation_error.is_none(), save_button);
                    let save_resp = if let Some(reason) = &validation_error {
                        save_resp.on_hover_text(reason)
                    } else {
                        save_resp
                    };
                    if save_resp.clicked() {
                        save_requested = true;
                    }
                    if ui.button("\u{27f2} Reset to auto").clicked() {
                        reset_requested = true;
                    }
                    if ui.button("\u{21bb} Reload").clicked() {
                        reload_requested = true;
                    }
                });
            });
        });
        egui::Panel::bottom("inspector")
            .resizable(false)
            .min_size(80.0)
            .show_inside(ui, |ui| {
                crate::inspector::render(ui, self);
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            crate::canvas::render(ui, self);
        });

        // Process flags AFTER the toolbar renders, so button clicks set the flags
        // before we read them. (Ctrl+S sets save_requested above the toolbar and
        // would otherwise work via input state, but the buttons themselves only
        // emit a click during the panel render.)
        if save_requested {
            match tokio::runtime::Handle::current().block_on(crate::save::save_and_apply(self)) {
                Ok(()) => {
                    self.dirty = false;
                    self.last_error = Some("Saved \u{2713}".to_string());
                }
                Err(e) => {
                    self.last_error = Some(e.to_string());
                }
            }
        }
        if reload_requested {
            match tokio::runtime::Handle::current().block_on(reload_monitors()) {
                Ok((monitors, cfg)) => {
                    self.load(&monitors, &cfg);
                    self.last_error = Some("Reloaded \u{2713}".to_string());
                }
                Err(e) => self.last_error = Some(format!("Reload failed: {}", e)),
            }
        }
        if reset_requested {
            match tokio::runtime::Handle::current().block_on(reload_monitors()) {
                Ok((monitors, _cfg)) => {
                    let empty = hyprmonitor::config::Config::default();
                    self.load(&monitors, &empty);
                    self.dirty = true;
                    self.last_error = Some("Reset to auto (unsaved)".to_string());
                }
                Err(e) => self.last_error = Some(format!("Reset failed: {}", e)),
            }
        }
    }
}
