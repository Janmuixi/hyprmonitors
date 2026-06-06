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
        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("hyprmonitor");
                ui.separator();
                ui.label(format!(
                    "{} monitors{}",
                    self.monitors.len(),
                    if self.dirty { " (unsaved)" } else { "" }
                ));
            });
        });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            crate::canvas::render(ui, self);
        });
    }
}
