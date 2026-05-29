use crate::model::{Mode, Monitor, MonitorConfig};
use anyhow::{Context, Result};
use hyprland::prelude::HyprData;
use std::fs;
use std::path::PathBuf;

/// Read all currently-connected monitors from Hyprland, enriched with
/// EDID-derived physical dimensions.
pub async fn query_monitors() -> Result<Vec<Monitor>> {
    let hypr_monitors = hyprland::data::Monitors::get_async()
        .await
        .context("hyprland::Monitors::get_async")?;

    let mut monitors = Vec::new();
    for hm in hypr_monitors.iter() {
        monitors.push(convert(hm));
    }
    Ok(monitors)
}

fn convert(hm: &hyprland::data::Monitor) -> Monitor {
    // The hyprland crate v0.4.0-beta.3 Monitor struct does not expose an
    // available_modes field; we synthesise a single-entry list from the
    // monitor's current active mode so the rest of the pipeline has something
    // to work with.
    let current_mode = Mode {
        width: hm.width as u32,
        height: hm.height as u32,
        refresh_hz: hm.refresh_rate as f64,
    };
    let physical_mm = read_edid_for_connector(&hm.name);

    Monitor {
        name: hm.name.clone(),
        width_px: hm.width as u32,
        height_px: hm.height as u32,
        physical_mm,
        available_modes: vec![current_mode],
    }
}

fn parse_available_modes(raw: &[String]) -> Vec<Mode> {
    raw.iter()
        .filter_map(|s| parse_mode_string(s))
        .collect()
}

fn parse_mode_string(s: &str) -> Option<Mode> {
    // Format: "1920x1080@60.000Hz" or "1920x1080@60Hz"
    let (res, hz) = s.split_once('@')?;
    let (w, h) = res.split_once('x')?;
    let width = w.trim().parse().ok()?;
    let height = h.trim().parse().ok()?;
    let hz_str = hz.trim_end_matches("Hz").trim();
    let refresh_hz: f64 = hz_str.parse().ok()?;
    Some(Mode { width, height, refresh_hz })
}

fn read_edid_for_connector(connector: &str) -> Option<(u32, u32)> {
    // /sys/class/drm/card?-<connector>/edid
    let drm = PathBuf::from("/sys/class/drm");
    let entries = fs::read_dir(&drm).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_str()?;
        if let Some(rest) = name.split_once('-') {
            if rest.1 == connector {
                let edid_path = entry.path().join("edid");
                if let Ok(bytes) = fs::read(&edid_path) {
                    return crate::algo::scale::parse_edid_dimensions(&bytes);
                }
            }
        }
    }
    None
}

/// Apply a single monitor configuration via `hyprctl keyword monitor`.
pub async fn apply(cfg: &MonitorConfig) -> Result<()> {
    let arg = cfg.to_string();
    hyprland::keyword::Keyword::set_async("monitor", arg.clone())
        .await
        .with_context(|| format!("hyprctl keyword monitor {}", arg))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mode_with_decimal_hz() {
        assert_eq!(
            parse_mode_string("1920x1080@59.951Hz"),
            Some(Mode { width: 1920, height: 1080, refresh_hz: 59.951 })
        );
    }

    #[test]
    fn parses_mode_with_integer_hz() {
        assert_eq!(
            parse_mode_string("2560x1440@165Hz"),
            Some(Mode { width: 2560, height: 1440, refresh_hz: 165.0 })
        );
    }

    #[test]
    fn parses_mode_without_hz_suffix() {
        assert_eq!(
            parse_mode_string("2560x1440@165"),
            Some(Mode { width: 2560, height: 1440, refresh_hz: 165.0 })
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_mode_string("garbage"), None);
        assert_eq!(parse_mode_string(""), None);
        assert_eq!(parse_mode_string("1920x@60Hz"), None);
    }
}
