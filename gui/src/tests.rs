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
fn to_config_skips_monitors_matching_auto() {
    // Pure-auto state — nothing edited. Saving should produce an empty
    // override list so "Reset to auto" actually round-trips and future auto
    // changes still apply.
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1"), fake_monitor("HDMI-A-1")], &Config::default());
    let cfg = app.to_config();
    assert_eq!(cfg.monitors.len(), 0, "pure-auto should not emit any overrides");
}

#[test]
fn to_config_emits_only_changed_monitors() {
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1"), fake_monitor("HDMI-A-1")], &Config::default());
    app.monitors[0].scale = 1.75; // only DP-1 deviates
    let cfg = app.to_config();
    assert_eq!(cfg.monitors.len(), 1);
    assert_eq!(cfg.monitors[0].connector_hint, "DP-1");
    assert_eq!(cfg.monitors[0].scale, 1.75);
}

#[test]
fn to_config_drops_override_after_user_reverts_to_auto_value() {
    // Start with an existing override that pins DP-1 to a non-auto scale.
    let cfg = Config {
        version: 1,
        monitors: vec![MonitorOverride {
            edid_id: Some("FAKE-0001-DP-1".to_string()),
            connector_hint: "DP-1".to_string(),
            position: Position { x: 0, y: 0 },
            mode: "1920x1080@144".to_string(),
            scale: 1.75,
            rotation: 0,
            disabled: false,
        }],
    };
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1")], &cfg);
    assert_eq!(app.monitors[0].scale, 1.75);

    // User manually changes the scale back to the auto value (1.0 for this
    // fake monitor, see fake_monitor() — 24" 1080p → ~92 DPI).
    app.monitors[0].scale = 1.0;
    let emitted = app.to_config();
    assert_eq!(
        emitted.monitors.len(),
        0,
        "matching auto means the override should be dropped, not preserved"
    );
}

#[test]
fn load_does_not_apply_edid_override_to_different_monitor_on_same_connector() {
    // An override was saved for monitor AAA on HDMI-A-1 with rotation=90 and
    // disabled=true. A *different* monitor (its own EDID) is now plugged into
    // HDMI-A-1. EDID is authoritative, so none of the override — including
    // rotation/disabled — must leak onto the new monitor. (merge_into_plan
    // already guards mode/scale/position; this covers rotation/disabled too.)
    let cfg = Config {
        version: 1,
        monitors: vec![MonitorOverride {
            edid_id: Some("AAA-0001-00000001".to_string()),
            connector_hint: "HDMI-A-1".to_string(),
            position: Position { x: 0, y: 0 },
            mode: "1920x1080@60".to_string(),
            scale: 1.0,
            rotation: 90,
            disabled: true,
        }],
    };
    let mut app = App::new();
    // fake_monitor("HDMI-A-1") has EDID "FAKE-0001-HDMI-A-1" — different from AAA.
    app.load(&[fake_monitor("HDMI-A-1")], &cfg);
    assert_eq!(app.monitors.len(), 1);
    assert_eq!(
        app.monitors[0].rotation, 0,
        "rotation from a different monitor's EDID override must not apply"
    );
    assert!(
        !app.monitors[0].disabled,
        "disabled from a different monitor's EDID override must not apply"
    );
}

#[test]
fn to_config_preserves_overrides_for_disconnected_monitors() {
    // Two setups were previously saved to disk: DP-1 (currently connected) and
    // an "office" 4K monitor on DP-9 (currently unplugged). Saving while only
    // DP-1 is connected must NOT wipe the office monitor's remembered config —
    // that EDID-keyed entry is the whole point of the file.
    let cfg = Config {
        version: 1,
        monitors: vec![
            MonitorOverride {
                edid_id: Some("FAKE-0001-DP-1".to_string()),
                connector_hint: "DP-1".to_string(),
                position: Position { x: 0, y: 0 },
                mode: "1280x720@60".to_string(),
                scale: 1.0,
                rotation: 0,
                disabled: false,
            },
            MonitorOverride {
                edid_id: Some("OFFICE-1234-00000001".to_string()),
                connector_hint: "DP-9".to_string(),
                position: Position { x: 1920, y: 0 },
                mode: "3840x2160@60".to_string(),
                scale: 2.0,
                rotation: 0,
                disabled: false,
            },
        ],
    };
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1")], &cfg);
    // Tweak DP-1 so it still emits an override of its own.
    app.monitors[0].position = (10, 20);
    let emitted = app.to_config();

    let office = emitted
        .monitors
        .iter()
        .find(|m| m.edid_id.as_deref() == Some("OFFICE-1234-00000001"))
        .expect("disconnected monitor's override must be preserved");
    assert_eq!(office.mode, "3840x2160@60");
    assert_eq!(office.position, Position { x: 1920, y: 0 });
    assert_eq!(office.scale, 2.0);

    let dp1 = emitted
        .monitors
        .iter()
        .find(|m| m.connector_hint == "DP-1")
        .expect("connected monitor's edited override must still be emitted");
    assert_eq!(dp1.position, Position { x: 10, y: 20 });
}

#[test]
fn to_config_does_not_preserve_stale_override_for_connected_monitor_reset_to_auto() {
    // A connected monitor that the user reset to auto must have its override
    // dropped — the preservation logic only applies to monitors that are NOT
    // currently connected, so this must not resurrect the stale entry.
    let cfg = Config {
        version: 1,
        monitors: vec![MonitorOverride {
            edid_id: Some("FAKE-0001-DP-1".to_string()),
            connector_hint: "DP-1".to_string(),
            position: Position { x: 0, y: 0 },
            mode: "1920x1080@144".to_string(),
            scale: 1.75,
            rotation: 0,
            disabled: false,
        }],
    };
    let mut app = App::new();
    app.load(&[fake_monitor("DP-1")], &cfg);
    app.monitors[0].scale = 1.0; // back to the auto value for this fake monitor
    let emitted = app.to_config();
    assert_eq!(
        emitted.monitors.len(),
        0,
        "a connected monitor reset to auto must not keep a stale override"
    );
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
