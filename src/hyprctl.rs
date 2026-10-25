//! Building the `hyprctl` invocation that applies a set of monitor configs.
//!
//! Hyprland 0.56 added a Lua config parser. When the session was started from
//! a `.lua` config, the server rejects every `hyprctl keyword` with
//! "keyword can't work with non-legacy parsers. Use eval." — and does so on
//! stdout with exit status 0, so an apply looks like it succeeded while
//! nothing changes. Sessions started from a `.conf` still only understand
//! `keyword`, so we probe once and emit whichever form this session accepts.

use crate::model::MonitorConfig;
use std::sync::OnceLock;

/// Whether this Hyprland session runs the Lua parser (and therefore wants
/// `hyprctl eval` instead of `hyprctl keyword`). Probed once per process.
pub fn uses_lua_parser() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        let out = std::process::Command::new("hyprctl").args(["eval", "return 1"]).output();
        match out {
            // Legacy servers don't know the request and answer "unknown request".
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "ok",
            Err(_) => false,
        }
    })
}

/// `hyprctl` arguments that apply every config in one server round-trip, so
/// Hyprland reconfigures atomically (no transient half-applied layout that
/// drops topbars or splits the screen). `None` when there is nothing to apply.
pub fn batch_args(configs: &[MonitorConfig]) -> Option<Vec<String>> {
    if configs.is_empty() {
        return None;
    }
    if uses_lua_parser() {
        let chunk =
            configs.iter().map(|c| c.to_lua()).collect::<Vec<_>>().join("\n");
        Some(vec!["eval".to_string(), chunk])
    } else {
        let batch = configs
            .iter()
            .map(|c| format!("keyword monitor {}", c))
            .collect::<Vec<_>>()
            .join(" ; ");
        Some(vec!["--batch".to_string(), batch])
    }
}

/// Human-readable form of what `batch_args` will send, for `--dry-run`.
pub fn dry_run_lines(configs: &[MonitorConfig]) -> Vec<String> {
    if uses_lua_parser() {
        configs.iter().map(|c| c.to_lua()).collect()
    } else {
        configs.iter().map(|c| format!("hyprctl keyword monitor {}", c)).collect()
    }
}

/// An `hyprctl eval` that fails reports it on stdout with exit status 0.
pub fn check_output(args: &[String], stdout: &str) -> anyhow::Result<()> {
    let stdout = stdout.trim();
    if args.first().map(String::as_str) == Some("eval") {
        if stdout != "ok" {
            anyhow::bail!("hyprctl eval: {}", stdout);
        }
    } else if stdout.contains("non-legacy parsers") {
        anyhow::bail!("hyprctl keyword rejected by Hyprland's Lua parser: {}", stdout);
    }
    Ok(())
}
