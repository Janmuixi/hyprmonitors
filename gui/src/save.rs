use crate::app::{App, EditableMonitor};
use anyhow::{anyhow, Result};
use hyprmonitor::config;
use hyprmonitor::model::MonitorConfig;
use std::path::PathBuf;

pub fn validate(monitors: &[EditableMonitor]) -> Result<()> {
    for m in monitors {
        let found = m.available_modes.iter().any(|am| {
            am.width == m.chosen_mode.width
                && am.height == m.chosen_mode.height
                && (am.refresh_hz - m.chosen_mode.refresh_hz).abs() < 1e-3
        });
        if !found {
            return Err(anyhow!(
                "{}: mode {}x{}@{} not in available modes",
                m.connector_hint,
                m.chosen_mode.width,
                m.chosen_mode.height,
                m.chosen_mode.refresh_hz,
            ));
        }
        if ![0, 90, 180, 270].contains(&m.rotation) {
            return Err(anyhow!(
                "{}: rotation must be 0/90/180/270 (got {})",
                m.connector_hint,
                m.rotation
            ));
        }
    }
    let active: Vec<(&str, i32, i32, i32, i32)> = monitors
        .iter()
        .filter(|m| !m.disabled)
        .map(|m| {
            let w = (m.chosen_mode.width as f64 / m.scale) as i32;
            let h = (m.chosen_mode.height as f64 / m.scale) as i32;
            let (ew, eh) = if m.rotation == 90 || m.rotation == 270 {
                (h, w)
            } else {
                (w, h)
            };
            (
                m.connector_hint.as_str(),
                m.position.0,
                m.position.1,
                m.position.0 + ew,
                m.position.1 + eh,
            )
        })
        .collect();
    for i in 0..active.len() {
        for j in (i + 1)..active.len() {
            let (na, ax0, ay0, ax1, ay1) = active[i];
            let (nb, bx0, by0, bx1, by1) = active[j];
            let overlaps = ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1;
            if overlaps {
                return Err(anyhow!("{} overlaps {}", na, nb));
            }
        }
    }
    Ok(())
}

pub fn config_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("hyprmonitor").join("monitors.json")
    } else {
        PathBuf::from(".config/hyprmonitor/monitors.json")
    }
}

pub async fn save_and_apply(app: &App) -> Result<()> {
    validate(&app.monitors)?;
    let cfg = app.to_config();
    let path = config_path();
    config::write_atomic(&path, &cfg)?;
    for m in app.monitors.iter().filter(|m| !m.disabled) {
        let cfg_entry = MonitorConfig {
            name: m.connector_hint.clone(),
            mode: m.chosen_mode.clone(),
            position: m.position,
            scale: m.scale,
        };
        let arg = cfg_entry.to_string();
        let output = tokio::process::Command::new("hyprctl")
            .args(["keyword", "monitor", &arg])
            .output()
            .await?;
        if !output.status.success() {
            return Err(anyhow!(
                "hyprctl keyword monitor {}: {}",
                arg,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    Ok(())
}
