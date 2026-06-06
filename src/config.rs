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
}
