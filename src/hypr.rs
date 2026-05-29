use hyprmonitor::model::{Mode, Monitor, MonitorConfig};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

/// Private struct matching the JSON shape emitted by `hyprctl monitors -j`.
/// Only the fields we need are declared; serde ignores the rest.
#[derive(Deserialize)]
struct HyprctlMonitor {
    name: String,
    width: u32,
    height: u32,
    #[serde(rename = "availableModes")]
    available_modes: Vec<String>,
}

/// Read all currently-connected monitors from Hyprland, enriched with
/// EDID-derived physical dimensions.
///
/// Uses `hyprctl monitors -j` so we get the full `availableModes` list that
/// the `hyprland` crate v0.4.0-beta.3 does not expose.
pub async fn query_monitors() -> Result<Vec<Monitor>> {
    let output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .await
        .context("hyprctl monitors -j")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "hyprctl monitors -j exited with {}: {}",
            output.status,
            stderr.trim()
        );
    }

    let raw: Vec<HyprctlMonitor> = serde_json::from_slice(&output.stdout)
        .context("hyprctl monitors -j")?;

    let mut monitors = Vec::new();
    for hm in raw {
        let available_modes = parse_available_modes(&hm.available_modes);
        let edid = read_edid_for_connector(&hm.name);
        let physical_mm = edid
            .as_deref()
            .and_then(hyprmonitor::algo::scale::parse_edid_dimensions);
        let preferred_mode = edid
            .as_deref()
            .and_then(hyprmonitor::algo::scale::parse_edid_preferred_mode);
        monitors.push(Monitor {
            name: hm.name,
            width_px: hm.width,
            height_px: hm.height,
            physical_mm,
            preferred_mode,
            available_modes,
        });
    }
    Ok(monitors)
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

fn read_edid_for_connector(connector: &str) -> Option<Vec<u8>> {
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
                    if !bytes.is_empty() {
                        return Some(bytes);
                    }
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
