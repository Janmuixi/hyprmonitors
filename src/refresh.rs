//! Best-effort refresh of bar / wallpaper layer-shell clients after a monitor
//! reconfigure.
//!
//! A resolution or scale change makes Hyprland destroy and recreate the
//! `wl_output`. The layer-surfaces that bars (waybar, …) and wallpaper daemons
//! (swaybg, …) draw onto are bound to that output, so they get torn down with
//! it. Most such clients don't re-attach cleanly on their own — the bar
//! vanishes or the wallpaper lands on the wrong output. That's the root cause
//! of the "have to click Save twice" symptom: the second apply changes no
//! geometry, so nothing is torn down and the clients recover incidentally.
//!
//! Rather than rely on that, we detect which known clients are running and
//! nudge each one deterministically after a successful apply: a reload signal
//! where the client documents one (waybar), otherwise kill-and-respawn from the
//! client's own argv (swaybg & friends have no reload mechanism and must be
//! restarted to rebind their surface).
//!
//! This is intentionally a fixed known-list, not a guess: clients that already
//! self-heal on output hotplug (swww) or whose windows are too stateful to
//! safely re-exec from argv (eww) are deliberately left alone.

use tracing::{debug, info, warn};

/// `SIGUSR2` on Linux. waybar reloads (tears down and recreates every bar) on
/// this signal, which rebinds bars dropped by an output teardown.
const SIGUSR2: i32 = 12;

/// A running process, reduced to what we need to recognise and refresh it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcInfo {
    pub pid: i32,
    /// `/proc/<pid>/comm` — the kernel's name for the process.
    pub comm: String,
    /// `/proc/<pid>/cmdline` split on NULs. Used to re-spawn a restartable
    /// client with the exact arguments it was launched with.
    pub argv: Vec<String>,
}

/// What to do to a recognised client to make it rebind after a reconfigure.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Handling {
    /// Send a signal; the client reloads in place (no flicker, no relaunch).
    Reload(i32),
    /// Kill and re-spawn from its own argv (no reload mechanism available).
    Restart,
}

/// The action `plan_refresh` decided on for one client. Kept separate from
/// execution so the decision logic is pure and unit-testable.
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshAction {
    Signal {
        pid: i32,
        signal: i32,
        client: &'static str,
    },
    Restart {
        pid: i32,
        argv: Vec<String>,
        client: &'static str,
    },
}

/// How each known client is refreshed. Anything not listed here is ignored —
/// including clients that self-heal (swww) or are unsafe to re-exec (eww).
fn handling_for(name: &str) -> Option<Handling> {
    Some(match name {
        "waybar" => Handling::Reload(SIGUSR2),
        "swaybg" | "hyprpaper" | "mpvpaper" | "wpaperd" | "hyprpanel" | "ironbar" => {
            Handling::Restart
        }
        _ => return None,
    })
}

const KNOWN_NAMES: &[&str] = &[
    "waybar",
    "swaybg",
    "hyprpaper",
    "mpvpaper",
    "wpaperd",
    "hyprpanel",
    "ironbar",
];

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Resolve a process to a known client name, matching on either `comm` or the
/// basename of `argv[0]`. Some clients run under an interpreter whose `comm`
/// isn't their own name (e.g. hyprpanel under gjs), so argv[0] is a useful
/// second chance.
fn canonical(p: &ProcInfo) -> Option<&'static str> {
    let argv0_base = p.argv.first().map(|s| basename(s));
    KNOWN_NAMES
        .iter()
        .copied()
        .find(|&k| p.comm == k || argv0_base == Some(k))
}

/// Decide what to do for each running process. Pure: no I/O, no side effects.
pub fn plan_refresh(procs: &[ProcInfo]) -> Vec<RefreshAction> {
    let mut actions = Vec::new();
    for p in procs {
        let Some(client) = canonical(p) else { continue };
        match handling_for(client).expect("KNOWN_NAMES and handling_for must agree") {
            Handling::Reload(signal) => actions.push(RefreshAction::Signal {
                pid: p.pid,
                signal,
                client,
            }),
            Handling::Restart => {
                // Can't re-spawn without an argv; skip rather than kill a
                // client we'd be unable to bring back.
                if p.argv.is_empty() {
                    warn!("refresh: {} (pid {}) has no argv; skipping", client, p.pid);
                    continue;
                }
                actions.push(RefreshAction::Restart {
                    pid: p.pid,
                    argv: p.argv.clone(),
                    client,
                });
            }
        }
    }
    actions
}

/// Read every `/proc/<pid>` into a `ProcInfo`. Entries that disappear or can't
/// be read mid-scan are skipped — this is best-effort.
fn scan_processes() -> Vec<ProcInfo> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return out;
    };
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let Some(pid) = fname.to_str().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        let path = entry.path();
        let comm = std::fs::read_to_string(path.join("comm"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let argv = std::fs::read(path.join("cmdline"))
            .map(|bytes| {
                bytes
                    .split(|b| *b == 0)
                    .filter(|s| !s.is_empty())
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if comm.is_empty() && argv.is_empty() {
            continue;
        }
        out.push(ProcInfo { pid, comm, argv });
    }
    out
}

fn execute(action: &RefreshAction) {
    match action {
        RefreshAction::Signal { pid, signal, client } => {
            info!("refresh: reloading {} (pid {}, signal {})", client, pid, signal);
            let status = std::process::Command::new("kill")
                .arg(format!("-{}", signal))
                .arg(pid.to_string())
                .status();
            if let Err(e) = status {
                warn!("refresh: failed to signal {}: {}", client, e);
            }
        }
        RefreshAction::Restart { pid, argv, client } => {
            info!("refresh: restarting {} (pid {})", client, pid);
            // Terminate the old instance...
            if let Err(e) = std::process::Command::new("kill")
                .arg(pid.to_string())
                .status()
            {
                warn!("refresh: failed to kill {}: {}", client, e);
            }
            // ...and re-spawn it detached, in a new session, so it outlives us
            // (the GUI may exit right after Save). `setsid -f` forks and exits
            // immediately, so this neither blocks nor leaves a zombie.
            let status = std::process::Command::new("setsid")
                .arg("-f")
                .arg("--")
                .args(argv)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if let Err(e) = status {
                warn!("refresh: failed to respawn {}: {}", client, e);
            }
        }
    }
}

/// Detect running bar/wallpaper clients and refresh them so they rebind to the
/// post-reconfigure output layout. Best-effort: every failure is logged and
/// swallowed, so a refresh problem never fails the apply that triggered it.
pub fn refresh_clients() {
    let actions = plan_refresh(&scan_processes());
    if actions.is_empty() {
        debug!("refresh: no known bar/wallpaper clients running");
        return;
    }
    for action in &actions {
        execute(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: i32, comm: &str, argv: &[&str]) -> ProcInfo {
        ProcInfo {
            pid,
            comm: comm.to_string(),
            argv: argv.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn waybar_reloads_via_sigusr2() {
        let procs = vec![proc(100, "waybar", &["/usr/bin/waybar"])];
        assert_eq!(
            plan_refresh(&procs),
            vec![RefreshAction::Signal {
                pid: 100,
                signal: SIGUSR2,
                client: "waybar",
            }]
        );
    }

    #[test]
    fn swaybg_restarts_from_its_argv() {
        let argv = ["/usr/bin/swaybg", "-i", "/home/u/bg.png", "-m", "fill"];
        let procs = vec![proc(200, "swaybg", &argv)];
        assert_eq!(
            plan_refresh(&procs),
            vec![RefreshAction::Restart {
                pid: 200,
                argv: argv.iter().map(|s| s.to_string()).collect(),
                client: "swaybg",
            }]
        );
    }

    #[test]
    fn unknown_clients_are_ignored() {
        let procs = vec![
            proc(1, "alacritty", &["alacritty"]),
            proc(2, "Hyprland", &["Hyprland"]),
        ];
        assert!(plan_refresh(&procs).is_empty());
    }

    #[test]
    fn self_healing_or_stateful_clients_are_left_alone() {
        // swww self-heals on hotplug; eww windows are too stateful to re-exec.
        let procs = vec![
            proc(3, "swww-daemon", &["/usr/bin/swww-daemon"]),
            proc(4, "swww", &["swww"]),
            proc(5, "eww", &["/usr/bin/eww", "daemon"]),
        ];
        assert!(plan_refresh(&procs).is_empty());
    }

    #[test]
    fn matches_on_argv0_when_comm_differs() {
        // hyprpanel runs under gjs, so comm isn't "hyprpanel" but argv[0] is.
        let procs = vec![proc(6, "gjs", &["/usr/bin/hyprpanel"])];
        assert_eq!(
            plan_refresh(&procs),
            vec![RefreshAction::Restart {
                pid: 6,
                argv: vec!["/usr/bin/hyprpanel".to_string()],
                client: "hyprpanel",
            }]
        );
    }

    #[test]
    fn restartable_client_without_argv_is_skipped() {
        // Recognised by comm, but no argv to relaunch from -> don't kill it.
        let procs = vec![proc(7, "swaybg", &[])];
        assert!(plan_refresh(&procs).is_empty());
    }

    #[test]
    fn preserves_order_across_multiple_clients() {
        let procs = vec![
            proc(10, "waybar", &["waybar"]),
            proc(11, "swaybg", &["swaybg", "-i", "/bg"]),
        ];
        let actions = plan_refresh(&procs);
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], RefreshAction::Signal { pid: 10, .. }));
        assert!(matches!(actions[1], RefreshAction::Restart { pid: 11, .. }));
    }
}
