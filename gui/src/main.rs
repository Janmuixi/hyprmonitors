mod app;
mod canvas;

use anyhow::Result;
use app::App;
use hyprmonitor::config;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("hyprmonitor_gui=info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let monitors = hyprmonitor_query().await?;
    let cfg = config::load_or_default(&config_path());

    let mut app = App::new();
    app.load(&monitors, &cfg);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "hyprmonitor",
        options,
        Box::new(|_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {}", e))?;
    Ok(())
}

async fn hyprmonitor_query() -> Result<Vec<hyprmonitor::model::Monitor>> {
    // The lib's hypr.rs lives in the bin crate, not the library, so we
    // shell out to `hyprctl` directly and reuse the same JSON shape.
    let output = tokio::process::Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!("hyprctl monitors -j: {}", String::from_utf8_lossy(&output.stderr));
    }
    parse_hyprctl_monitors(&output.stdout)
}

fn parse_hyprctl_monitors(json: &[u8]) -> Result<Vec<hyprmonitor::model::Monitor>> {
    use hyprmonitor::algo::scale;
    use hyprmonitor::model::{Mode, Monitor};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Raw {
        name: String,
        width: u32,
        height: u32,
        #[serde(rename = "availableModes")]
        available_modes: Vec<String>,
    }

    let raw: Vec<Raw> = serde_json::from_slice(json)?;
    let monitors = raw
        .into_iter()
        .map(|r| {
            let edid = read_edid_for_connector(&r.name);
            let physical_mm = edid.as_deref().and_then(scale::parse_edid_dimensions);
            let preferred_mode = edid.as_deref().and_then(scale::parse_edid_preferred_mode);
            let edid_id = edid.as_deref().and_then(scale::derive_edid_id);
            let available_modes: Vec<Mode> = r
                .available_modes
                .into_iter()
                .filter_map(|s| parse_mode_string(&s))
                .collect();
            Monitor {
                name: r.name,
                width_px: r.width,
                height_px: r.height,
                physical_mm,
                preferred_mode,
                edid_id,
                available_modes,
            }
        })
        .collect();
    Ok(monitors)
}

fn parse_mode_string(s: &str) -> Option<hyprmonitor::model::Mode> {
    let (res, hz) = s.split_once('@')?;
    let (w, h) = res.split_once('x')?;
    let hz = hz.trim_end_matches("Hz").trim();
    Some(hyprmonitor::model::Mode {
        width: w.parse().ok()?,
        height: h.parse().ok()?,
        refresh_hz: hz.parse().ok()?,
    })
}

fn read_edid_for_connector(connector: &str) -> Option<Vec<u8>> {
    let drm = std::path::PathBuf::from("/sys/class/drm");
    let entries = std::fs::read_dir(&drm).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_str()?;
        if let Some(rest) = name_str.split_once('-') {
            if rest.1 == connector {
                let edid_path = entry.path().join("edid");
                if let Ok(bytes) = std::fs::read(&edid_path) {
                    if !bytes.is_empty() {
                        return Some(bytes);
                    }
                }
            }
        }
    }
    None
}

fn config_path() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".config")
            .join("hyprmonitor")
            .join("monitors.json")
    } else {
        std::path::PathBuf::from(".config/hyprmonitor/monitors.json")
    }
}
