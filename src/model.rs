#[derive(Debug, Clone, PartialEq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub physical_mm: Option<(u32, u32)>,
    pub available_modes: Vec<Mode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorConfig {
    pub name: String,
    pub mode: Mode,
    pub position: (i32, i32),
    pub scale: f64,
}

impl std::fmt::Display for MonitorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hz = format_hz(self.mode.refresh_hz);
        let scale = format_scale(self.scale);
        write!(
            f,
            "{},{}x{}@{},{}x{},{}",
            self.name,
            self.mode.width,
            self.mode.height,
            hz,
            self.position.0,
            self.position.1,
            scale
        )
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

fn format_scale(s: f64) -> String {
    if (s - s.round()).abs() < 1e-6 {
        format!("{}", s as u32)
    } else {
        let s = format!("{:.6}", s);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_config_formats_as_hyprland_keyword() {
        let cfg = MonitorConfig {
            name: "DP-1".to_string(),
            mode: Mode { width: 2560, height: 1440, refresh_hz: 165.0 },
            position: (0, 0),
            scale: 1.0,
        };
        assert_eq!(cfg.to_string(), "DP-1,2560x1440@165,0x0,1");
    }

    #[test]
    fn monitor_config_keeps_fractional_refresh() {
        let cfg = MonitorConfig {
            name: "eDP-1".to_string(),
            mode: Mode { width: 1920, height: 1080, refresh_hz: 59.951 },
            position: (2560, 0),
            scale: 1.25,
        };
        assert_eq!(cfg.to_string(), "eDP-1,1920x1080@59.951,2560x0,1.25");
    }
}
