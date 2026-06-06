use hyprmonitor::algo::plan;
use hyprmonitor::model::{Mode, Monitor};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct HyprMonitorFixture {
    name: String,
    width: u32,
    height: u32,
    #[serde(rename = "availableModes")]
    available_modes: Vec<String>,
}

fn load(path: &str) -> Vec<Monitor> {
    let bytes = fs::read(Path::new(path)).expect("fixture not found");
    let raw: Vec<HyprMonitorFixture> = serde_json::from_slice(&bytes).expect("parse json");
    raw.into_iter()
        .map(|f| Monitor {
            name: f.name,
            width_px: f.width,
            height_px: f.height,
            physical_mm: None, // fixtures don't carry EDID
            preferred_mode: None,
            edid_id: None,
            available_modes: f
                .available_modes
                .into_iter()
                .filter_map(|s| parse_mode(&s))
                .collect(),
        })
        .collect()
}

fn parse_mode(s: &str) -> Option<Mode> {
    let (res, hz) = s.split_once('@')?;
    let (w, h) = res.split_once('x')?;
    let hz = hz.trim_end_matches("Hz");
    Some(Mode {
        width: w.parse().ok()?,
        height: h.parse().ok()?,
        refresh_hz: hz.parse().ok()?,
    })
}

#[test]
fn single_4k_laptop_picks_native_at_max_hz() {
    let monitors = load("tests/fixtures/single_4k_laptop.json");
    let plan = plan(&monitors);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].name, "eDP-1");
    assert_eq!(plan[0].mode.width, 3840);
    assert_eq!(plan[0].mode.height, 2400);
    assert_eq!(plan[0].position, (0, 0));
    // No EDID in fixture → scale falls back to 1.0
    assert_eq!(plan[0].scale, 1.0);
}

#[test]
fn laptop_plus_external_orders_internal_first() {
    let monitors = load("tests/fixtures/laptop_plus_external.json");
    let plan = plan(&monitors);
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].name, "eDP-1");
    assert_eq!(plan[1].name, "DP-1");
    assert_eq!(plan[0].position, (0, 0));
    assert_eq!(plan[1].position, (1920, 0));
    assert_eq!(plan[1].mode.refresh_hz, 165.0);
}
