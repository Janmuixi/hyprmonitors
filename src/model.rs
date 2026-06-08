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
    pub preferred_mode: Option<(u32, u32)>,
    pub edid_id: Option<String>,
    pub available_modes: Vec<Mode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorConfig {
    pub name: String,
    pub mode: Mode,
    pub position: (i32, i32),
    pub scale: f64,
    pub rotation: u16,
    pub disabled: bool,
}

impl std::fmt::Display for MonitorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.disabled {
            return write!(f, "{},disable", self.name);
        }
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
        )?;
        if self.rotation != 0 {
            let transform = (self.rotation / 90) % 4;
            write!(f, ",transform,{}", transform)?;
        }
        Ok(())
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
            rotation: 0,
            disabled: false,
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
            rotation: 0,
            disabled: false,
        };
        assert_eq!(cfg.to_string(), "eDP-1,1920x1080@59.951,2560x0,1.25");
    }

    #[test]
    fn monitor_config_emits_transform_when_rotated() {
        let cfg = MonitorConfig {
            name: "DP-2".to_string(),
            mode: Mode { width: 1920, height: 1080, refresh_hz: 60.0 },
            position: (1920, 0),
            scale: 1.0,
            rotation: 90,
            disabled: false,
        };
        assert_eq!(cfg.to_string(), "DP-2,1920x1080@60,1920x0,1,transform,1");
    }

    #[test]
    fn monitor_config_disabled_emits_disable_keyword() {
        let cfg = MonitorConfig {
            name: "HDMI-A-1".to_string(),
            mode: Mode { width: 1920, height: 1080, refresh_hz: 60.0 },
            position: (1000, 200),
            scale: 1.5,
            rotation: 90,
            disabled: true,
        };
        assert_eq!(cfg.to_string(), "HDMI-A-1,disable");
    }
}
