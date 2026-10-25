//! Wire the daemon into Hyprland's autostart.
//!
//! Hyprland 0.56 picks `hyprland.lua` over `hyprland.conf` whenever both
//! exist and silently ignores the other, so an autostart line written in the
//! wrong language never runs and never errors.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

const LUA: &str = r#"hl.on("hyprland.start", function() hl.exec_cmd("hyprmonitor daemon") end)"#;
const CONF: &str = "exec-once = hyprmonitor daemon";

/// `$XDG_CONFIG_HOME/hypr`, else `~/.config/hypr`.
pub fn config_dir() -> Result<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(xdg).join("hypr"));
    }
    match std::env::var_os("HOME") {
        Some(home) => Ok(PathBuf::from(home).join(".config/hypr")),
        None => bail!("no HOME or XDG_CONFIG_HOME"),
    }
}

/// The file to append to and the line to append, for the config in `dir`.
fn target(dir: &Path) -> Result<(PathBuf, &'static str)> {
    for (main, split, line) in
        [("hyprland.lua", "autostart.lua", LUA), ("hyprland.conf", "autostart.conf", CONF)]
    {
        if dir.join(main).is_file() {
            // Setups that split autostart into its own file already source it.
            let split = dir.join(split);
            return Ok((if split.is_file() { split } else { dir.join(main) }, line));
        }
    }
    bail!("no hyprland.lua or hyprland.conf in {}", dir.display())
}

/// Append the autostart line. Returns the file, and `false` if hyprmonitor
/// was already launched from it.
pub fn setup(dir: &Path) -> Result<(PathBuf, bool)> {
    let (file, line) = target(dir)?;
    let existing = fs::read_to_string(&file).unwrap_or_default();
    let configured = existing
        .lines()
        .any(|l| l.contains("hyprmonitor") && !l.trim_start().starts_with(['#', '-']));
    if configured {
        return Ok((file, false));
    }
    let mut out = existing.trim_end().to_string();
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(line);
    out.push('\n');
    fs::write(&file, out)?;
    Ok((file, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_with(files: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for f in files {
            fs::write(d.path().join(f), "").unwrap();
        }
        d
    }

    #[test]
    fn lua_wins_over_leftover_conf() {
        let d = dir_with(&["hyprland.lua", "hyprland.conf"]);
        assert_eq!(target(d.path()).unwrap(), (d.path().join("hyprland.lua"), LUA));
    }

    #[test]
    fn split_autostart_file_is_preferred() {
        let d = dir_with(&["hyprland.conf", "autostart.conf"]);
        assert_eq!(target(d.path()).unwrap(), (d.path().join("autostart.conf"), CONF));
    }

    #[test]
    fn errors_without_a_hyprland_config() {
        assert!(target(dir_with(&[]).path()).is_err());
    }

    #[test]
    fn appends_once() {
        let d = dir_with(&["hyprland.lua"]);
        assert_eq!(setup(d.path()).unwrap().1, true);
        let after = fs::read_to_string(d.path().join("hyprland.lua")).unwrap();
        assert_eq!(after, format!("{}\n", LUA));

        assert_eq!(setup(d.path()).unwrap().1, false);
        assert_eq!(fs::read_to_string(d.path().join("hyprland.lua")).unwrap(), after);
    }

    #[test]
    fn a_commented_example_does_not_count_as_configured() {
        let d = dir_with(&[]);
        fs::write(d.path().join("hyprland.lua"), "-- hl.exec_cmd(\"hyprmonitor daemon\")\n").unwrap();
        assert_eq!(setup(d.path()).unwrap().1, true);
    }
}
