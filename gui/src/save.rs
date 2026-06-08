use crate::app::{App, EditableMonitor};
use anyhow::{anyhow, Result};
use hyprmonitor::config;
use hyprmonitor::model::MonitorConfig;

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

pub fn save_and_apply(app: &mut App) -> Result<()> {
    crate::canvas::align_all(&mut app.monitors);
    validate(&app.monitors)?;
    let cfg = app.to_config();
    let path = config::default_path();
    config::write_atomic(&path, &cfg)?;

    // Apply via `hyprctl --batch` so all monitor changes land in a single
    // Hyprland transaction. Disabled monitors are kept in the batch — their
    // Display impl renders `NAME,disable`, which is how you turn a monitor
    // off in Hyprland.
    let batch = app
        .monitors
        .iter()
        .map(|m| {
            let cfg_entry = MonitorConfig {
                name: m.connector_hint.clone(),
                mode: m.chosen_mode.clone(),
                position: m.position,
                scale: m.scale,
                rotation: m.rotation,
                disabled: m.disabled,
            };
            format!("keyword monitor {}", cfg_entry)
        })
        .collect::<Vec<_>>()
        .join(" ; ");

    if batch.is_empty() {
        return Ok(());
    }

    let output = std::process::Command::new("hyprctl")
        .args(["--batch", &batch])
        .output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "hyprctl --batch: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}
