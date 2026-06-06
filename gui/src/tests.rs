#![cfg(test)]

use crate::app::App;
use hyprmonitor::config::{Config, MonitorOverride, Position};
use hyprmonitor::model::{Mode, Monitor};

fn fake_monitor(name: &str) -> Monitor {
    Monitor {
        name: name.to_string(),
        width_px: 1920,
        height_px: 1080,
        physical_mm: Some((530, 300)),
        preferred_mode: Some((1920, 1080)),
        edid_id: Some(format!("FAKE-0001-{}", name)),
        available_modes: vec![
            Mode { width: 1920, height: 1080, refresh_hz: 60.0 },
            Mode { width: 1920, height: 1080, refresh_hz: 144.0 },
            Mode { width: 1280, height: 720, refresh_hz: 60.0 },
        ],
    }
}

#[test]
fn load_builds_editable_monitors_from_auto_plan() {
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1")], &Config::default());
    assert_eq!(app.monitors.len(), 1);
    assert_eq!(app.monitors[0].connector_hint, "DP-1");
    assert_eq!(app.monitors[0].chosen_mode.refresh_hz, 144.0);
    assert!(!app.dirty);
}

#[test]
fn load_applies_config_override() {
    let cfg = Config {
        version: 1,
        monitors: vec![MonitorOverride {
            edid_id: Some("FAKE-0001-DP-1".to_string()),
            connector_hint: "DP-1".to_string(),
            position: Position { x: 500, y: 0 },
            mode: "1280x720@60".to_string(),
            scale: 1.0,
            rotation: 90,
            disabled: false,
        }],
    };
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1")], &cfg);
    assert_eq!(app.monitors[0].chosen_mode.width, 1280);
    assert_eq!(app.monitors[0].position, (500, 0));
    assert_eq!(app.monitors[0].rotation, 90);
}

#[test]
fn to_config_round_trips_back_to_overrides() {
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1"), fake_monitor("HDMI-A-1")], &Config::default());
    app.monitors[0].position = (100, 200);
    app.monitors[1].disabled = true;
    let cfg = app.to_config();
    assert_eq!(cfg.version, 1);
    assert_eq!(cfg.monitors.len(), 2);
    let dp1 = cfg.monitors.iter().find(|m| m.connector_hint == "DP-1").unwrap();
    assert_eq!(dp1.position, Position { x: 100, y: 200 });
    let hdmi = cfg.monitors.iter().find(|m| m.connector_hint == "HDMI-A-1").unwrap();
    assert!(hdmi.disabled);
}

#[test]
fn save_validate_rejects_overlapping_enabled_monitors() {
    use crate::save::validate;
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1"), fake_monitor("DP-2")], &Config::default());
    app.monitors[0].position = (0, 0);
    app.monitors[1].position = (100, 0); // overlap with DP-1 (which spans 0..1920)
    let err = validate(&app.monitors).expect_err("should detect overlap");
    assert!(err.to_string().contains("overlaps"), "got: {}", err);
}

#[test]
fn save_validate_allows_disabled_overlapping_monitors() {
    use crate::save::validate;
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1"), fake_monitor("DP-2")], &Config::default());
    app.monitors[0].position = (0, 0);
    app.monitors[1].position = (100, 0);
    app.monitors[1].disabled = true; // hidden — overlap doesn't matter
    validate(&app.monitors).expect("disabled overlaps allowed");
}

#[test]
fn save_validate_rejects_mode_not_in_available() {
    use crate::save::validate;
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1")], &Config::default());
    app.monitors[0].chosen_mode = Mode { width: 9999, height: 9999, refresh_hz: 60.0 };
    let err = validate(&app.monitors).expect_err("should reject");
    assert!(err.to_string().contains("not in available modes"));
}
