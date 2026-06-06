# hyprmonitor GUI — Design

A second binary, `hyprmonitor-gui`, that lets the user drag-and-drop monitors into a custom layout and override mode / scale / rotation / disabled state. The arrangement is persisted to `~/.config/hyprmonitor/monitors.json`, keyed on a stable EDID-derived identifier so a different physical monitor on the same connector never reuses a saved layout.

## Goals

- Visual editor: drag monitor rectangles to set positions, snap to neighbours.
- Full per-monitor controls in v1: position, resolution, refresh, scale, rotation, disabled.
- Persistent layout: JSON file keyed on a stable physical-panel identifier.
- The daemon merges the saved config over the existing auto-plan on each reconfigure, so any monitor the user has pinned uses their settings and anything else falls back to the auto rules.

## Non-goals (v1)

- Live preview while dragging. Save & Apply is the only path that touches hyprctl. ("Live preview" is a later option.)
- Daemon ↔ GUI coordination over a socket. The daemon re-reads the config on its next reconfigure (next hotplug event). File-watching is a small follow-up; out of scope here.
- A built-in monitor identification overlay (e.g., flashing the name on each screen). Hyprland already shows this via its own tooling.
- Pixel-perfect screenshot tests of the rendered UI.

## Architecture

```
hyprmonitor (existing bin)             hyprmonitor-gui (new bin)
   │                                       │
   └─→ uses ───┐                ┌─── uses ─┘
               ↓                ↓
       hyprmonitor (lib)
        ├── algo  (existing — plan, mode, scale, layout, primary)
        ├── model (existing — Monitor, Mode, MonitorConfig)
        ├── config (new — JSON read/write, merge_into_plan)
        └── algo::scale (extended — derive_edid_id helper)
                          │
                          ↓
            ~/.config/hyprmonitor/monitors.json
```

- **`hyprmonitor-gui`** — new bin crate in the workspace. Uses `eframe` (egui + winit). Pulls `Monitor` / `Mode` / `plan()` from the existing library.
- **`config` module in lib** — pure read/write of the JSON file and the `merge_into_plan(&mut plan, &Config)` overlay function.
- **`algo::scale::derive_edid_id`** — pure function on raw EDID bytes, returns a stable `Option<String>` identifier.
- **Daemon change** — at `reconfigure()` time, after `algo::plan(&monitors)`, call `config::merge_into_plan(&mut plan, &config)` before applying.

## `edid_id` derivation

The identifier is built from EDID bytes 8-17:

```
edid_id = "<MFG>-<PRODUCT>-<SERIAL>"        when serial != 0
        = "<MFG>-<PRODUCT>-W<WEEK>Y<YEAR>"  when serial == 0
        = None                              if EDID is unreadable / header invalid
```

- **MFG** (bytes 8-9): 3-letter manufacturer code, packed 5 bits per letter, big-endian. `'A' = 1, ..., 'Z' = 26`. Example: bytes `0x4D 0xD9` → `"LEN"`.
- **PRODUCT** (bytes 10-11, little-endian u16): printed as 4-digit hex.
- **SERIAL** (bytes 12-15, little-endian u32): printed as 8-digit hex.
- **WEEK** (byte 16): manufacturing week. Values 1-54 valid; 0xFF means model year (handled by reading byte 17 as the year directly).
- **YEAR** (byte 17): `year + 1990`. The W/Y suffix is used only when the serial is exactly zero, which is common on cheaper panels.

`connector_hint` ("HDMI-A-1") is recorded alongside in the JSON for human readability and is used as a last-resort match key for monitors that have no readable EDID at all (virtual displays, KVMs, screen-sharing tools).

## JSON config

Path: `~/.config/hyprmonitor/monitors.json`.

```json
{
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
    },
    {
      "edid_id": "GSM-5BBF-00000000",
      "connector_hint": "HDMI-A-1",
      "position": { "x": 3360, "y": 0 },
      "mode": "1920x1080@143.98",
      "scale": 1.0,
      "rotation": 0,
      "disabled": false
    }
  ]
}
```

- `edid_id` may be `null` for a monitor without readable EDID; matching then uses `connector_hint`.
- `mode` is the same string format Hyprland reports: `WxH@Hz` (Hz may be fractional).
- `rotation` is one of 0, 90, 180, 270 (degrees, clockwise).
- `disabled: true` removes the monitor from the applied plan; nothing is sent to `hyprctl` for it.

### Merge rules — `config::merge_into_plan`

1. For each entry currently in `plan` (built by `algo::plan`), look up a config entry:
   - First by `edid_id` (when both sides have one).
   - Else by `connector_hint`.
2. If matched: replace `mode`, `scale`, `position`, `rotation` with the config values. If `disabled: true`, remove the entry from the plan.
3. If not matched: leave the auto-planned entry untouched — fresh monitors keep auto behavior.
4. Config entries that don't match any live monitor are ignored silently (they're history; the monitor isn't currently plugged in).

### File handling

- **Missing file** → empty config; daemon and GUI behave exactly as today.
- **Malformed JSON** → log error + `notify-send`; daemon falls through to auto. GUI shows the parse error in a toast and starts with all current monitors as fresh.
- **Unknown / future `version`** → log warning, ignore the file (forward-compatible).
- **Atomic writes**: GUI writes to `monitors.json.tmp`, `fsync`s, `rename`s to `monitors.json`. A crash mid-save can't leave a half-written file.

## GUI structure

Single binary `hyprmonitor-gui`. One window, repainted every frame.

```
gui/
├── Cargo.toml
└── src/
    ├── main.rs       # eframe::run_native entry
    ├── app.rs        # App state + update() loop
    ├── canvas.rs     # draggable monitor rectangles
    ├── inspector.rs  # bottom details panel
    └── render.rs     # world↔screen coord helpers + snapping
```

### State

```rust
struct App {
    monitors: Vec<EditableMonitor>,
    selected: Option<usize>,
    canvas_scale: f32,           // world px → screen px (auto-fit on load)
    canvas_offset: Vec2,         // pan offset
    dirty: bool,
    last_error: Option<String>,  // shown in a toast
}

struct EditableMonitor {
    edid_id: Option<String>,
    connector_hint: String,
    available_modes: Vec<Mode>,  // from query, never edited
    physical_mm: Option<(u32, u32)>,
    // edited fields:
    chosen_mode: Mode,
    scale: f64,
    position: (i32, i32),
    rotation: u16,               // 0 / 90 / 180 / 270
    disabled: bool,
}
```

### Frame loop

1. **Top toolbar:** `[↻ Reload]  [⟲ Reset to auto]  [💾 Save & apply]  status: "N monitors, total WxH"`.
2. **Canvas (`CentralPanel`):** draw each `EditableMonitor` as a `Rect` at `(position.x * canvas_scale, position.y * canvas_scale)` sized `(mode.width / scale, mode.height / scale) * canvas_scale`. Internal panels (`primary::is_internal(connector_hint)`) tagged "(laptop)".
3. **Dragging:** `ui.interact(rect, id, Sense::click_and_drag())`. While dragging, update `position` in world coords; on release, snap to any other monitor's edge within `20 / canvas_scale` world px.
4. **Right-click → context menu:** Disable / Set as primary / Rotate (90 / 180 / 270 / reset to 0).
5. **Inspector (`TopBottomPanel::bottom`):** when `selected.is_some()`, render dropdowns for resolution / refresh / scale, numeric inputs for x / y, checkbox for disabled, radio for rotation.
6. **Keyboard:** arrow keys nudge `position` by 1 world px when something is selected. `Ctrl+S` triggers Save.

### Save flow

1. **Validate:** no overlapping monitors; all chosen modes are present in the monitor's `available_modes`. On failure, populate `last_error` and abort.
2. **Serialize** `Vec<EditableMonitor>` → `config::Config` → `serde_json::to_string_pretty`.
3. **Atomic write** to `~/.config/hyprmonitor/monitors.json`.
4. **Apply** each entry via the existing `hypr::apply` (same path the daemon uses; reused via the library).
5. Clear `dirty`, flash a "Saved ✓" toast.

No live preview, no daemon coordination, no IPC. If the daemon is running, it sees the new config on its next reconfigure pass (next hotplug event).

## Error handling

| Situation | Behavior |
|---|---|
| Config file missing | Empty config — daemon and GUI behave as if no overrides exist |
| Malformed JSON | Daemon logs error + `notify-send`, falls through to auto. GUI shows the parse error in a toast and starts fresh |
| Unknown / future `version` | Log warning, ignore the file |
| `edid_id` collision (rare) | First match wins; log a warning |
| Atomic write fails | GUI shows error toast, keeps `dirty = true` |
| `hyprctl keyword` fails during GUI save | Per-monitor error toast; the JSON is already written, daemon will retry on next hotplug |
| GUI started with no monitors | Empty canvas + "no monitors detected" message; Reload still works |
| User-edited monitors overlap on canvas | Save button disabled + hint shown at the offending edge |

## Testing

**Pure-logic tests (no GUI runtime):**

- `algo::scale::derive_edid_id` — table-driven EDID byte fixtures → expected `edid_id` (mfg-letter decode, serial-zero W/Y fallback, unreadable EDID returns None).
- `config::merge_into_plan` — fake plan + fake config → expected merged plan. Covers match by edid_id, fall-through to connector_hint, `disabled: true` removes the entry, no-match leaves auto plan unchanged.
- `config::parse` — valid JSON → `Config`; malformed JSON → error; unknown `version` → empty config + warning.
- `config::write_atomic` — writes to tempdir, verifies the `.tmp` + rename dance; injects a write failure to confirm the real file is never corrupted.

**GUI tests (headless):**

- `egui::Context::run` with synthetic input events (drag, arrow-key nudge, keyboard `Ctrl+S`) and asserts on `App` state changes — specifically: drag-snap behavior, arrow-nudge by 1 world px, Save button disabled when monitors overlap.
- No pixel-level screenshot tests; egui's renderer is upstream's responsibility.

**Manual smoke (documented, not automated):**

- Plug a new monitor → it appears in canvas after Reload, uses auto rules (no override).
- Save a config, restart Hyprland → daemon re-applies on startup from `monitors.json`.
- Open GUI with deliberately malformed JSON → toast appears, app still usable; correcting the file via the GUI's save fixes it.

## Decisions log

- **GUI tech:** egui + eframe. Rust-native, immediate-mode, drag-drop is straightforward, Wayland-native via winit.
- **Coordination model:** GUI is a pure editor. JSON write + `hyprctl keyword monitor` on Save. No live preview in v1.
- **Persistence:** JSON (not TOML) at `~/.config/hyprmonitor/monitors.json`, atomic writes, `version: 1` for forward compatibility.
- **Identifier:** EDID-derived `edid_id` (mfg-product-serial, with week/year fallback when serial is zero), with `connector_hint` as last-resort match for monitors without EDID.
- **Daemon integration:** existing `reconfigure()` calls `config::merge_into_plan(&mut plan, &config)` before applying.
- **Scope:** position + mode + refresh + scale + rotation + disabled in v1.
