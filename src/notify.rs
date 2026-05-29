use std::process::Command;
use tracing::warn;

/// Send a critical-urgency desktop notification. Failures are logged
/// at warn level and otherwise swallowed — we're already in an error
/// path when this is called.
pub fn notify_failure(body: &str) {
    let result = Command::new("notify-send")
        .args([
            "--urgency=critical",
            "--app-name=hyprmonitor",
            "hyprmonitor",
            body,
        ])
        .status();
    match result {
        Ok(s) if s.success() => {}
        Ok(s) => warn!("notify-send exited with {}", s),
        Err(e) => warn!("notify-send failed to spawn: {}", e),
    }
}
