# hyprmonitor — Design

A Rust CLI for Hyprland that detects the best configuration for each connected monitor and applies it automatically when displays are plugged in or removed.

Target: Arch Linux + Hyprland (developed against 0.54.3).

## Goals

- Detect connected monitors and choose a sensible mode, scale, and position for each, without manual configuration.
- React automatically to hotplug events (monitor connected, disconnected, lid open/close).
- Provide a one-shot subcommand to apply the same logic on demand.
- Keep the decision logic pure and unit-testable.

## Non-goals (v1)

- Persisting configuration across Hyprland restarts via `hyprland.conf`. State is re-derived on each daemon startup by querying live monitors and re-applying.
- Per-monitor user overrides (configuration file). The algorithm is fully automatic.
- VRR / 10-bit color / HDR / rotation / mirroring controls. Users can layer those on themselves via their own `hyprland.conf`.
- EDID-based "preferred mode" detection. Approximated as the highest-pixel-count mode reported by Hyprland.
- Wayland protocols other than Hyprland's IPC. Not portable to sway, etc.

## CLI

```
hyprmonitor daemon          # subscribe to Hyprland events, reconfigure on change
hyprmonitor apply           # one-shot: query monitors, configure, exit
hyprmonitor list            # print detected monitors + what apply WOULD do

# global flags
  -v, --verbose             # debug-level logs to stderr
      --dry-run             # for `apply`: print hyprctl commands instead of running them
```

`daemon` is intended to be launched from the user's `hyprland.conf` via `exec-once = hyprmonitor daemon`. `apply` is a manual rescue command. `list` is for debugging — it shows the parsed monitor state and the plan without applying anything.

## Architecture

```
src/
├── main.rs               # entry point
├── cli.rs                # clap definitions, subcommand dispatch
├── daemon.rs             # async event loop + debouncing
├── hypr.rs               # adapter over the `hyprland` crate (query + apply)
├── model.rs              # Monitor, Mode, MonitorConfig, Layout
├── notify.rs             # notify-send wrapper
└── algo/
    ├── mod.rs            # plan(monitors) -> Vec<MonitorConfig>
    ├── mode.rs           # pick_best_mode
    ├── scale.rs          # pick_scale (EDID + DPI)
    ├── layout.rs         # arrange_positions
    └── primary.rs        # is_internal (eDP/LVDS/DSI)
```

**Key boundary:** `algo::*` is pure. It takes `Vec<Monitor>` and returns `Vec<MonitorConfig>`. No I/O, no `hyprctl`, no filesystem access. `hypr.rs` is the only module that talks to Hyprland. `daemon.rs` glues them together.

This means the whole decision pipeline is unit-testable from a fake monitor list — no Hyprland required in tests.

### Key dependencies

- [`hyprland`](https://crates.io/crates/hyprland) — IPC client, event listener, monitor queries.
- `tokio` — async runtime for the event loop.
- `clap` — argument parsing.
- `serde` + `serde_json` — for fixture-based tests parsing real `hyprctl monitors -j` output.
- `edid-rs` — to parse `/sys/class/drm/.../edid` for physical dimensions.
- `tracing` + `tracing-subscriber` — structured logs to stderr.
- `thiserror` — error types in `hypr.rs`.

## Algorithm: best mode

For each monitor:

1. Read `availableModes` from Hyprland — list of `WxH@Hz` strings.
2. **Native resolution** ≈ the mode with the highest pixel count (`W*H`). This is a heuristic; modern displays virtually always have their native res as the highest advertised mode.
3. Filter modes to that resolution.
4. Pick the one with the highest refresh rate.
5. Format the config string: `<name>,<W>x<H>@<Hz>,<x>x<y>,<scale>`.

**Refresh rate parsing.** Hyprland reports refresh as a float, e.g. `59.951Hz`. We round to 3 decimals when comparing, then pass the original (unrounded) string to `hyprctl`.

**Edge case — no modes reported.** Fall back to `preferred,auto,1.0` and log a warning. Don't notify; this is rare and recoverable.

**Future:** EDID-based preferred-mode lookup if the "max pixels = native" heuristic causes problems on some hardware.

## Algorithm: smart-primary layout

**Internal panel detection.** Connector name matches `^(eDP|LVDS|DSI)-\d+$` (case-insensitive).

**Ordering.** When multiple monitors are connected:

1. Internal panel first, if present and active.
2. Externals after, sorted by connector name (`DP-1`, `DP-2`, `HDMI-A-1`, …) for stable order across reconnects.

**Position assignment.** Place left-to-right at `y=0`. Each monitor's `x` equals the sum of effective widths of monitors to its left, where `effective_width = round(width_px / scale)`.

The first monitor in the list is at `x=0` — that's the de-facto primary in Hyprland (it gets the lowest-numbered workspaces and any "primary"-targeted output).

**Lid-close.** When the lid closes, Hyprland fires `monitorremoved>>eDP-1` and `hyprctl monitors -j` no longer lists eDP. The next `reconfigure()` pass naturally treats the remaining externals as the new primary set; no special lid-state polling is needed. Re-opening fires `monitoradded>>eDP-1` and we place it back at `x=0` per the ordering rule.

**Single monitor.** Placed at `(0, 0)`.

## Algorithm: DPI-based scale

**Get physical size.** Read `/sys/class/drm/card*-<connector>/edid` and parse bytes 0x15–0x16 (max horizontal/vertical image size in cm). Convert to mm.

**Compute DPI:**

```
diagonal_inches = sqrt(width_mm² + height_mm²) / 25.4
diagonal_px     = sqrt(width_px² + height_px²)
dpi             = diagonal_px / diagonal_inches
```

**Pick from this table:**

| DPI range   | Scale |
|-------------|-------|
| < 110       | 1.0   |
| 110 – 139   | 1.25  |
| 140 – 169   | 1.5   |
| 170 – 219   | 1.75  |
| ≥ 220       | 2.0   |

**EDID unreadable or reports zero dimensions.** Some virtual displays (KVMs, screen-sharing tools) and projectors return zeroed physical size. Treated identically to "unreadable": fall back to scale `1.0`, log a warning. No notification — common and would be spammy.

**Hyprland's non-integer-scale warning.** Hyprland warns when `width_px / scale` isn't an integer. We accept that warning for v1 rather than snap to nearest "valid" scale; it's a log line in Hyprland, not a failure.

## Event loop

```
on startup:
  reconfigure()                              # initial pass

on monitoradded / monitorremoved:
  schedule_reconfigure_in(200ms)             # debounce
    (if already scheduled, reset the timer)

reconfigure():
  monitors = hypr::query()
  plan     = algo::plan(monitors)
  for cfg in plan:
    hypr::apply(cfg)                         # hyprctl keyword monitor ...
  verify_or_notify(plan)                     # re-query, compare; notify on mismatch
```

**Debounce: 200ms.** Docking stations and KVM switches commonly fire several monitor events within milliseconds of each other. Reacting on each one causes mid-transition flicker. 200ms catches the burst while still feeling instant.

**Hyprland socket disconnects.** Likely cause is Hyprland restarting or crashing. Retry connecting with exponential backoff (1s, 2s, 4s, 8s, capped at 30s, forever). Log each attempt. Run `reconfigure()` immediately when the connection comes back.

**`apply` subcommand** reuses `algo::plan` + `hypr::apply` — same path, no event loop wrapper.

## Error handling & notifications

| Situation | Behavior |
|---|---|
| `hyprctl keyword` returns non-zero | Log error, `notify-send` with monitor name + chosen mode, continue (do not crash daemon) |
| `hyprctl keyword` returns 0 but chosen mode wasn't applied | Re-query monitors after apply; compare `(width, height)` exactly and refresh rate within ±0.01Hz; on mismatch → `notify-send` "DP-2: requested 2560x1440@165Hz but got 1920x1080@60Hz" |
| EDID unreadable | Fall back to scale 1.0, log warning, no notification |
| `availableModes` empty | Use `preferred,auto,1.0`, log warning, no notification |
| Hyprland event socket disconnects | Exponential-backoff retry, no notification |
| `notify-send` itself fails | Silently ignore — already in an error path |

Notifications use urgency `critical` so they persist until dismissed. Title `hyprmonitor`, body describes the specific problem.

## Testing

**Unit tests (pure functions, fast):**

- `algo::mode::pick_best_mode` — fake `Vec<Mode>` → expected mode.
- `algo::scale::pick_scale_from_dpi` — table of `(width_px, height_px, width_mm, height_mm) → expected_scale`.
- `algo::layout::arrange_positions` — fake monitor list → expected `(x, y)` coordinates.
- `algo::primary::is_internal` — connector name → bool.
- `algo::plan` — end-to-end fake monitors → expected `Vec<MonitorConfig>`.

**Fixture-based tests:**

- Capture real `hyprctl monitors -j` output in `tests/fixtures/*.json` (single monitor, laptop+external, dual external, 4K HiDPI).
- Parse + plan + assert generated config strings.

**Integration test (manual / CI-skipped):**

- Marked `#[ignore]`, only runs locally. Spawns `hyprmonitor apply --dry-run` and snapshots output against a known monitor setup.

**Not tested in this project:**

- Real `hyprctl keyword` calls (require live Hyprland).
- `notify-send` (smoke-tested manually).
- The `hyprland` crate's event-listener wiring (their job).

## Decisions log

- **Run mode:** daemon + one-shot subcommand. Daemon reacts to events; one-shot is a rescue command.
- **Best mode:** native resolution + max refresh rate at that resolution. "Native" approximated as highest-pixel-count mode.
- **Layout:** auto with smart primary — internal panel first, externals to the right.
- **Scale:** auto from DPI computed against EDID physical size.
- **Apply method:** live via `hyprctl keyword` only. No `hyprland.conf` writes. Daemon re-applies on Hyprland restart.
- **Feedback:** logs always; `notify-send` only on failures.
- **IPC:** the `hyprland` crate with `tokio` async runtime.
