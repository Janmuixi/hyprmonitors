use crate::model::{Mode, MonitorConfig};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::warn;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub monitors: Vec<MonitorOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorOverride {
    #[serde(default)]
    pub edid_id: Option<String>,
    pub connector_hint: String,
    pub position: Position,
    pub mode: String,
    pub scale: f64,
    #[serde(default)]
    pub rotation: u16,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

pub const CURRENT_VERSION: u32 = 1;

/// Standard on-disk location: `$HOME/.config/hyprmonitor/monitors.json`,
/// or the same path under the current directory if `$HOME` is unset.
pub fn default_path() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".config")
            .join("hyprmonitor")
            .join("monitors.json")
    } else {
        std::path::PathBuf::from(".config/hyprmonitor/monitors.json")
    }
}

/// Load the config from a path. Returns an empty Config on missing file,
/// malformed JSON (with a warn log), or an unrecognized version.
pub fn load_or_default(path: &Path) -> Config {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("config: failed to read {:?}: {}", path, e);
            }
            return Config::default();
        }
    };
    match serde_json::from_slice::<Config>(&bytes) {
        Ok(cfg) if cfg.version == CURRENT_VERSION => cfg,
        Ok(cfg) => {
            warn!(
                "config: unknown version {}; ignoring file at {:?}",
                cfg.version, path
            );
            Config::default()
        }
        Err(e) => {
            warn!("config: malformed JSON in {:?}: {}", path, e);
            Config::default()
        }
    }
}

/// Atomically write `cfg` to `path` by writing to `<path>.tmp`, fsyncing,
/// and renaming. Returns an io::Result; on failure, the original file at
/// `path` (if any) is untouched.
pub fn write_atomic(path: &Path, cfg: &Config) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let tmp = path.with_extension(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| format!("{}.tmp", e))
            .unwrap_or_else(|| "tmp".to_string()),
    );
    let json = serde_json::to_vec_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Merge user overrides into the auto-generated plan. For each entry in `plan`:
/// - Look up a matching `MonitorOverride` by `edid_id` first, then by
///   `connector_hint`.
/// - If matched: replace mode/scale/position/rotation with the override's
///   values and copy `disabled` through. A disabled entry stays in the plan
///   so `apply_batch` can send `NAME,disable` to Hyprland.
/// - If not matched: leave the entry as-is.
pub fn merge_into_plan(
    plan: &mut Vec<MonitorConfig>,
    monitors: &[crate::model::Monitor],
    cfg: &Config,
) {
    for entry in plan.iter_mut() {
        let monitor = monitors.iter().find(|m| m.name == entry.name);

        let override_entry = cfg.monitors.iter().find(|o| {
            match (monitor.and_then(|m| m.edid_id.as_deref()), o.edid_id.as_deref()) {
                (Some(mid), Some(oid)) if mid == oid => return true,
                _ => {}
            }
            o.connector_hint == entry.name
        });

        if let Some(o) = override_entry {
            entry.disabled = o.disabled;
            if let Some(mode) = parse_mode_string(&o.mode) {
                entry.mode = mode;
            }
            entry.position = (o.position.x, o.position.y);
            entry.scale = o.scale;
            entry.rotation = o.rotation;
        }
    }
}

fn parse_mode_string(s: &str) -> Option<Mode> {
    let (res, hz) = s.split_once('@')?;
    let (w, h) = res.split_once('x')?;
    let hz = hz.trim_end_matches("Hz").trim();
    Some(Mode {
        width: w.parse().ok()?,
        height: h.parse().ok()?,
        refresh_hz: hz.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(contents: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("tempfile");
        f.write_all(contents.as_bytes()).expect("write");
        f
    }

    #[test]
    fn parses_valid_config() {
        let f = write_tmp(
            r#"{
                "version": 1,
                "monitors": [
                    {
                        "edid_id": "LEN-4032-00012345",
                        "connector_hint": "eDP-1",
                        "position": { "x": 0, "y": 0 },
                        "mode": "2880x1800@120",
                        "scale": 2.0,
                        "rotation": 0,
                        "disabled": false
                    }
                ]
            }"#,
        );
        let cfg = load_or_default(f.path());
        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.monitors.len(), 1);
        assert_eq!(cfg.monitors[0].connector_hint, "eDP-1");
        assert_eq!(cfg.monitors[0].mode, "2880x1800@120");
        assert_eq!(cfg.monitors[0].scale, 2.0);
    }

    #[test]
    fn missing_file_returns_default() {
        let cfg = load_or_default(Path::new("/tmp/does-not-exist-xyzzy.json"));
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn malformed_json_returns_default() {
        let f = write_tmp("{ not valid json");
        let cfg = load_or_default(f.path());
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn unknown_version_returns_default() {
        let f = write_tmp(r#"{ "version": 999, "monitors": [] }"#);
        let cfg = load_or_default(f.path());
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn write_atomic_creates_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("monitors.json");
        let cfg = Config {
            version: 1,
            monitors: vec![MonitorOverride {
                edid_id: None,
                connector_hint: "DP-1".to_string(),
                position: Position { x: 0, y: 0 },
                mode: "1920x1080@60".to_string(),
                scale: 1.0,
                rotation: 0,
                disabled: false,
            }],
        };
        write_atomic(&path, &cfg).expect("write");
        let reloaded = load_or_default(&path);
        assert_eq!(reloaded, cfg);
    }

    #[test]
    fn write_atomic_overwrites_existing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("monitors.json");
        std::fs::write(&path, b"OLD CONTENTS").expect("seed");
        let cfg = Config { version: 1, monitors: vec![] };
        write_atomic(&path, &cfg).expect("write");
        let reloaded = load_or_default(&path);
        assert_eq!(reloaded, cfg);
    }

    #[test]
    fn write_atomic_leaves_no_tmp_on_success() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("monitors.json");
        let cfg = Config { version: 1, monitors: vec![] };
        write_atomic(&path, &cfg).expect("write");
        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists(), "{} should not exist after success", tmp.display());
    }

    use crate::model::{Mode, Monitor, MonitorConfig};

    fn fake_mon(name: &str, edid_id: Option<&str>) -> Monitor {
        Monitor {
            name: name.to_string(),
            width_px: 1920,
            height_px: 1080,
            physical_mm: None,
            preferred_mode: None,
            edid_id: edid_id.map(String::from),
            available_modes: vec![Mode { width: 1920, height: 1080, refresh_hz: 60.0 }],
        }
    }

    fn fake_cfg(name: &str, edid_id: Option<&str>, mode: &str) -> MonitorOverride {
        MonitorOverride {
            edid_id: edid_id.map(String::from),
            connector_hint: name.to_string(),
            position: Position { x: 100, y: 200 },
            mode: mode.to_string(),
            scale: 1.5,
            rotation: 0,
            disabled: false,
        }
    }

    fn fake_plan_entry(name: &str) -> MonitorConfig {
        MonitorConfig {
            name: name.to_string(),
            mode: Mode { width: 1920, height: 1080, refresh_hz: 60.0 },
            position: (0, 0),
            scale: 1.0,
            rotation: 0,
            disabled: false,
        }
    }

    #[test]
    fn merge_matches_by_edid_id() {
        let monitors = vec![fake_mon("HDMI-A-1", Some("GSM-1234-00000001"))];
        let mut plan = vec![fake_plan_entry("HDMI-A-1")];
        let cfg = Config {
            version: 1,
            monitors: vec![fake_cfg("DOES-NOT-MATTER", Some("GSM-1234-00000001"), "2560x1440@60")],
        };
        merge_into_plan(&mut plan, &monitors, &cfg);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].mode.width, 2560);
        assert_eq!(plan[0].position, (100, 200));
        assert_eq!(plan[0].scale, 1.5);
    }

    #[test]
    fn merge_falls_back_to_connector_hint() {
        let monitors = vec![fake_mon("DP-1", None)];
        let mut plan = vec![fake_plan_entry("DP-1")];
        let cfg = Config {
            version: 1,
            monitors: vec![fake_cfg("DP-1", None, "2560x1440@60")],
        };
        merge_into_plan(&mut plan, &monitors, &cfg);
        assert_eq!(plan[0].mode.width, 2560);
    }

    #[test]
    fn merge_disabled_marks_entry_keeps_it() {
        let monitors = vec![
            fake_mon("eDP-1", Some("LEN-1234-00000001")),
            fake_mon("HDMI-A-1", Some("GSM-1234-00000001")),
        ];
        let mut plan = vec![fake_plan_entry("eDP-1"), fake_plan_entry("HDMI-A-1")];
        let mut disabled = fake_cfg("HDMI-A-1", Some("GSM-1234-00000001"), "1920x1080@60");
        disabled.disabled = true;
        let cfg = Config { version: 1, monitors: vec![disabled] };
        merge_into_plan(&mut plan, &monitors, &cfg);
        assert_eq!(plan.len(), 2, "disabled entries stay in plan for apply_batch");
        assert!(!plan[0].disabled);
        assert!(plan[1].disabled);
        assert_eq!(plan[1].name, "HDMI-A-1");
    }

    #[test]
    fn merge_no_match_leaves_plan_untouched() {
        let monitors = vec![fake_mon("DP-1", Some("ABC-1234-00000001"))];
        let mut plan = vec![fake_plan_entry("DP-1")];
        let original = plan.clone();
        let cfg = Config {
            version: 1,
            monitors: vec![fake_cfg("OTHER", Some("XYZ-9999-00000099"), "9999x9999@60")],
        };
        merge_into_plan(&mut plan, &monitors, &cfg);
        assert_eq!(plan, original);
    }

    #[test]
    fn merge_malformed_mode_string_falls_back() {
        let monitors = vec![fake_mon("DP-1", None)];
        let mut plan = vec![fake_plan_entry("DP-1")];
        let cfg = Config {
            version: 1,
            monitors: vec![fake_cfg("DP-1", None, "garbage")],
        };
        merge_into_plan(&mut plan, &monitors, &cfg);
        // Mode unchanged because the override's mode string couldn't parse
        assert_eq!(plan[0].mode.width, 1920);
        // But position and scale still applied
        assert_eq!(plan[0].position, (100, 200));
        assert_eq!(plan[0].scale, 1.5);
    }

    #[test]
    fn merge_applies_rotation_override() {
        let monitors = vec![fake_mon("DP-1", None)];
        let mut plan = vec![fake_plan_entry("DP-1")];
        let mut o = fake_cfg("DP-1", None, "1920x1080@60");
        o.rotation = 90;
        let cfg = Config { version: 1, monitors: vec![o] };
        merge_into_plan(&mut plan, &monitors, &cfg);
        assert_eq!(plan[0].rotation, 90);
    }
}
