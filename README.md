# hyprmonitor

Auto-configures Hyprland monitors when displays are plugged in or out. No config file needed — it picks a sensible mode, scale, and position for each connected monitor.

## What it does

- Picks each monitor's native resolution at its highest refresh rate.
- Computes scale from physical DPI (EDID): 1.0 / 1.25 / 1.5 / 1.75 / 2.0.
- Places the laptop panel (`eDP-*` / `LVDS-*` / `DSI-*`) at `(0, 0)`, externals to the right sorted by connector name.
- Reacts to hotplug events with a 200 ms debounce, reconnects if Hyprland restarts.
- Notifies via `notify-send` on failures only.

## Build

```sh
cargo build --release
# binary at ./target/release/hyprmonitor
```

Requires Rust 2021 edition and a live Hyprland session (developed against 0.54.x).

## Usage

```sh
hyprmonitor list              # show detected monitors and the plan
hyprmonitor apply             # apply the plan once
hyprmonitor apply --dry-run   # print the hyprctl commands instead of running them
hyprmonitor daemon            # run as a daemon, reacting to hotplug events
hyprmonitor -v <cmd>          # debug logs
```

## Auto-start with Hyprland

Add to `~/.config/hypr/hyprland.conf`:

```
exec-once = /path/to/hyprmonitor daemon
```

## How it picks the config

For each monitor:

1. **Mode:** the highest pixel count in `availableModes`, then the highest refresh rate at that resolution.
2. **Scale:** computed from the EDID-derived diagonal DPI. Falls back to `1.0` if EDID is unavailable.
3. **Position:** monitors are laid out left-to-right at `y = 0`; effective width = `mode.width / scale`. Internal panel goes first.

When you close your laptop lid, the eDP disappears from Hyprland's monitor list — the daemon's next reconfigure naturally promotes the external to the primary slot. Reopening puts it back.

## Limitations

- Verify-after-apply only checks resolution, not refresh rate (Hyprland's IPC doesn't surface the active refresh on each monitor through the crate we use).
- No per-monitor config overrides — everything is automatic.
- No VRR, HDR, mirroring, or rotation. Layer those on yourself via your own `hyprland.conf` after the daemon's config line.
