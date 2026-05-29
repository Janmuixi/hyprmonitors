# hyprmonitor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI/daemon for Hyprland that auto-detects and applies the best mode, scale, and position for each connected monitor when displays are hotplugged.

**Architecture:** Pure `algo` layer (mode/scale/layout/primary) decides what configuration each monitor should have. Thin `hypr` adapter talks to Hyprland via the `hyprland` crate. `daemon` glues them with a tokio event loop and 200ms debouncing.

**Tech Stack:** Rust 2021 edition, `hyprland` crate, `tokio`, `clap` (derive), `tracing`, `regex`. No external EDID crate (we parse 2 bytes ourselves).

**Spec:** `docs/superpowers/specs/2026-05-29-hyprmonitor-design.md`

---

## File Structure

```
Cargo.toml
src/
├── main.rs               # entry point — calls cli::run()
├── cli.rs                # clap definitions + subcommand dispatch
├── daemon.rs             # async event loop + debounce + retry
├── hypr.rs               # adapter over hyprland crate + EDID reads
├── model.rs              # Monitor, Mode, MonitorConfig
├── notify.rs             # notify-send wrapper
└── algo/
    ├── mod.rs            # plan(monitors) -> Vec<MonitorConfig>
    ├── mode.rs           # pick_best_mode
    ├── scale.rs          # parse_edid_dimensions, compute_dpi, pick_scale
    ├── layout.rs         # arrange (positions)
    └── primary.rs        # is_internal
tests/
└── fixtures/
    └── *.json            # real hyprctl monitors -j captures
```

`algo/` is pure (no I/O). `hypr.rs` is the only module that touches Hyprland or the filesystem.

---

## Task 1: Project init + dependencies

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

- [ ] **Step 1: Initialize the cargo project**

Run from `/home/jmg/projects/best-effor-hyprmonitor`:

```bash
cargo init --name hyprmonitor
```

Expected: creates `Cargo.toml` and `src/main.rs` with a hello-world stub. Note: `cargo init` will warn that the repo already has a `.git` — that's fine.

- [ ] **Step 2: Replace Cargo.toml with the full dependency set**

Overwrite `Cargo.toml`:

```toml
[package]
name = "hyprmonitor"
version = "0.1.0"
edition = "2021"

[dependencies]
hyprland = "0.4"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "signal", "sync"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
thiserror = "1"
regex = "1"
anyhow = "1"
```

- [ ] **Step 3: Verify it builds**

```bash
cargo check
```

Expected: PASS. If `hyprland = "0.4"` doesn't resolve, run `cargo search hyprland` and use the latest published version; note the chosen version in this task.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs .gitignore
git commit -m "feat: project init with hyprland/tokio/clap deps"
```

---

## Task 2: Core model types

**Files:**
- Create: `src/model.rs`
- Modify: `src/main.rs` (add `mod model;`)

- [ ] **Step 1: Write the failing test for MonitorConfig Display formatting**

Create `src/model.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub physical_mm: Option<(u32, u32)>,
    pub available_modes: Vec<Mode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorConfig {
    pub name: String,
    pub mode: Mode,
    pub position: (i32, i32),
    pub scale: f64,
}

impl std::fmt::Display for MonitorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_config_formats_as_hyprland_keyword() {
        let cfg = MonitorConfig {
            name: "DP-1".to_string(),
            mode: Mode { width: 2560, height: 1440, refresh_hz: 165.0 },
            position: (0, 0),
            scale: 1.0,
        };
        assert_eq!(cfg.to_string(), "DP-1,2560x1440@165,0x0,1");
    }

    #[test]
    fn monitor_config_keeps_fractional_refresh() {
        let cfg = MonitorConfig {
            name: "eDP-1".to_string(),
            mode: Mode { width: 1920, height: 1080, refresh_hz: 59.951 },
            position: (2560, 0),
            scale: 1.25,
        };
        assert_eq!(cfg.to_string(), "eDP-1,1920x1080@59.951,2560x0,1.25");
    }
}
```

Add `mod model;` to `src/main.rs` so the file gets compiled:

```rust
mod model;

fn main() {
    println!("Hello, world!");
}
```

- [ ] **Step 2: Run tests — they should fail**

```bash
cargo test model::
```

Expected: FAIL — panics with "not implemented" inside the Display impl.

- [ ] **Step 3: Implement Display**

Replace the `unimplemented!()` body in `src/model.rs`:

```rust
impl std::fmt::Display for MonitorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hz = format_hz(self.mode.refresh_hz);
        let scale = format_scale(self.scale);
        write!(
            f,
            "{},{}x{}@{},{}x{},{}",
            self.name,
            self.mode.width,
            self.mode.height,
            hz,
            self.position.0,
            self.position.1,
            scale
        )
    }
}

fn format_hz(hz: f64) -> String {
    if (hz - hz.round()).abs() < 1e-6 {
        format!("{}", hz as u32)
    } else {
        let s = format!("{:.3}", hz);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn format_scale(s: f64) -> String {
    if (s - s.round()).abs() < 1e-6 {
        format!("{}", s as u32)
    } else {
        let s = format!("{:.6}", s);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}
```

- [ ] **Step 4: Run tests — should pass**

```bash
cargo test model::
```

Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/model.rs src/main.rs
git commit -m "feat(model): Monitor, Mode, MonitorConfig with hyprland keyword Display"
```

---

## Task 3: Internal panel detection

**Files:**
- Create: `src/algo/mod.rs`
- Create: `src/algo/primary.rs`
- Modify: `src/main.rs` (add `mod algo;`)

- [ ] **Step 1: Add the module declaration**

Add to `src/main.rs`:

```rust
mod model;
mod algo;

fn main() {
    println!("Hello, world!");
}
```

Create `src/algo/mod.rs`:

```rust
pub mod primary;
```

- [ ] **Step 2: Write the failing test**

Create `src/algo/primary.rs`:

```rust
pub fn is_internal(name: &str) -> bool {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edp_is_internal() {
        assert!(is_internal("eDP-1"));
        assert!(is_internal("eDP-2"));
    }

    #[test]
    fn lvds_is_internal() {
        assert!(is_internal("LVDS-1"));
    }

    #[test]
    fn dsi_is_internal() {
        assert!(is_internal("DSI-1"));
    }

    #[test]
    fn case_insensitive() {
        assert!(is_internal("edp-1"));
        assert!(is_internal("EDP-1"));
    }

    #[test]
    fn external_outputs_are_not_internal() {
        assert!(!is_internal("DP-1"));
        assert!(!is_internal("HDMI-A-1"));
        assert!(!is_internal("DVI-D-1"));
        assert!(!is_internal("VGA-1"));
    }

    #[test]
    fn prefix_match_only() {
        // "eDP" in the middle of a name shouldn't count
        assert!(!is_internal("HDMI-eDP-1"));
    }
}
```

- [ ] **Step 3: Run tests — should fail**

```bash
cargo test algo::primary::
```

Expected: FAIL — `unimplemented!()` panic.

- [ ] **Step 4: Implement**

Replace the function in `src/algo/primary.rs`:

```rust
use regex::Regex;

pub fn is_internal(name: &str) -> bool {
    let re = Regex::new(r"^(?i)(eDP|LVDS|DSI)-\d+$").expect("static regex");
    re.is_match(name)
}
```

- [ ] **Step 5: Run tests — should pass**

```bash
cargo test algo::primary::
```

Expected: PASS (6 tests).

- [ ] **Step 6: Commit**

```bash
git add src/algo/ src/main.rs
git commit -m "feat(algo): is_internal panel detection (eDP/LVDS/DSI)"
```

---

## Task 4: Best mode picker

**Files:**
- Create: `src/algo/mode.rs`
- Modify: `src/algo/mod.rs` (add `pub mod mode;`)

- [ ] **Step 1: Add module declaration**

Update `src/algo/mod.rs`:

```rust
pub mod mode;
pub mod primary;
```

- [ ] **Step 2: Write the failing tests**

Create `src/algo/mode.rs`:

```rust
use crate::model::Mode;

pub fn pick_best_mode(modes: &[Mode]) -> Option<Mode> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(w: u32, h: u32, hz: f64) -> Mode {
        Mode { width: w, height: h, refresh_hz: hz }
    }

    #[test]
    fn picks_highest_pixel_count_then_max_hz() {
        let modes = vec![
            m(1920, 1080, 144.0),
            m(2560, 1440, 60.0),
            m(2560, 1440, 165.0),
            m(2560, 1440, 144.0),
            m(1920, 1080, 240.0),
        ];
        assert_eq!(pick_best_mode(&modes), Some(m(2560, 1440, 165.0)));
    }

    #[test]
    fn single_mode_picked() {
        let modes = vec![m(1920, 1080, 60.0)];
        assert_eq!(pick_best_mode(&modes), Some(m(1920, 1080, 60.0)));
    }

    #[test]
    fn empty_returns_none() {
        assert_eq!(pick_best_mode(&[]), None);
    }

    #[test]
    fn fractional_hz_picks_highest() {
        let modes = vec![
            m(3840, 2160, 59.951),
            m(3840, 2160, 60.0),
            m(3840, 2160, 59.94),
        ];
        assert_eq!(pick_best_mode(&modes), Some(m(3840, 2160, 60.0)));
    }
}
```

- [ ] **Step 3: Run tests — should fail**

```bash
cargo test algo::mode::
```

Expected: FAIL.

- [ ] **Step 4: Implement**

Replace the function body in `src/algo/mode.rs`:

```rust
use crate::model::Mode;

pub fn pick_best_mode(modes: &[Mode]) -> Option<Mode> {
    let max_pixels = modes
        .iter()
        .map(|m| m.width as u64 * m.height as u64)
        .max()?;

    modes
        .iter()
        .filter(|m| (m.width as u64 * m.height as u64) == max_pixels)
        .max_by(|a, b| {
            a.refresh_hz
                .partial_cmp(&b.refresh_hz)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
}
```

- [ ] **Step 5: Run tests — should pass**

```bash
cargo test algo::mode::
```

Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add src/algo/mode.rs src/algo/mod.rs
git commit -m "feat(algo): pick_best_mode = max pixels then max Hz"
```

---

## Task 5: EDID dimensions parser

**Files:**
- Create: `src/algo/scale.rs`
- Modify: `src/algo/mod.rs` (add `pub mod scale;`)

- [ ] **Step 1: Add module declaration**

Update `src/algo/mod.rs`:

```rust
pub mod mode;
pub mod primary;
pub mod scale;
```

- [ ] **Step 2: Write the failing tests**

Create `src/algo/scale.rs`:

```rust
/// Parse maximum image size (cm) from EDID block at bytes 0x15-0x16,
/// returning (width_mm, height_mm). Returns None if header is invalid
/// or either dimension is zero.
pub fn parse_edid_dimensions(edid: &[u8]) -> Option<(u32, u32)> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edid_with_dims(w_cm: u8, h_cm: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        // Valid EDID header
        bytes[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        bytes[0x15] = w_cm;
        bytes[0x16] = h_cm;
        bytes
    }

    #[test]
    fn parses_known_dimensions() {
        let edid = edid_with_dims(60, 34); // 24" 16:9-ish monitor
        assert_eq!(parse_edid_dimensions(&edid), Some((600, 340)));
    }

    #[test]
    fn zero_width_returns_none() {
        let edid = edid_with_dims(0, 34);
        assert_eq!(parse_edid_dimensions(&edid), None);
    }

    #[test]
    fn zero_height_returns_none() {
        let edid = edid_with_dims(60, 0);
        assert_eq!(parse_edid_dimensions(&edid), None);
    }

    #[test]
    fn invalid_header_returns_none() {
        let mut edid = vec![0u8; 128];
        edid[0x15] = 60;
        edid[0x16] = 34;
        // Header is all zeros — invalid
        assert_eq!(parse_edid_dimensions(&edid), None);
    }

    #[test]
    fn short_buffer_returns_none() {
        let edid = vec![0u8; 10];
        assert_eq!(parse_edid_dimensions(&edid), None);
    }
}
```

- [ ] **Step 3: Run tests — should fail**

```bash
cargo test algo::scale::parse_edid
```

Expected: FAIL.

- [ ] **Step 4: Implement**

Replace the function body in `src/algo/scale.rs`:

```rust
pub fn parse_edid_dimensions(edid: &[u8]) -> Option<(u32, u32)> {
    const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
    if edid.len() < 0x17 {
        return None;
    }
    if edid[0..8] != HEADER {
        return None;
    }
    let w_cm = edid[0x15];
    let h_cm = edid[0x16];
    if w_cm == 0 || h_cm == 0 {
        return None;
    }
    Some((w_cm as u32 * 10, h_cm as u32 * 10))
}
```

- [ ] **Step 5: Run tests — should pass**

```bash
cargo test algo::scale::parse_edid
```

Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add src/algo/scale.rs src/algo/mod.rs
git commit -m "feat(algo): parse EDID width/height in mm from bytes 0x15-0x16"
```

---

## Task 6: DPI computation + scale picker

**Files:**
- Modify: `src/algo/scale.rs`

- [ ] **Step 1: Append failing tests for compute_dpi and pick_scale**

Append to `src/algo/scale.rs` (above the existing `#[cfg(test)]` block):

```rust
/// Compute physical DPI given pixel size and millimeter size.
/// Returns None if either mm dimension is zero.
pub fn compute_dpi(width_px: u32, height_px: u32, width_mm: u32, height_mm: u32) -> Option<f64> {
    unimplemented!()
}

/// Map DPI to a Hyprland scale factor from the spec table.
pub fn pick_scale_from_dpi(dpi: f64) -> f64 {
    unimplemented!()
}
```

Now add tests inside the existing `#[cfg(test)] mod tests` block (before the closing `}`):

```rust
    #[test]
    fn dpi_zero_mm_returns_none() {
        assert_eq!(compute_dpi(1920, 1080, 0, 340), None);
        assert_eq!(compute_dpi(1920, 1080, 600, 0), None);
    }

    #[test]
    fn dpi_known_monitor() {
        // 24" 1920x1080 → ~92 DPI
        let dpi = compute_dpi(1920, 1080, 530, 300).unwrap();
        assert!((dpi - 92.6).abs() < 1.0, "dpi was {}", dpi);
    }

    #[test]
    fn dpi_hidpi_laptop() {
        // 13.3" 2560x1600 (16:10) → ~226 DPI
        let dpi = compute_dpi(2560, 1600, 286, 179).unwrap();
        assert!((dpi - 226.0).abs() < 2.0, "dpi was {}", dpi);
    }

    #[test]
    fn scale_table_low_dpi() {
        assert_eq!(pick_scale_from_dpi(80.0), 1.0);
        assert_eq!(pick_scale_from_dpi(109.999), 1.0);
    }

    #[test]
    fn scale_table_mid_dpi() {
        assert_eq!(pick_scale_from_dpi(110.0), 1.25);
        assert_eq!(pick_scale_from_dpi(139.9), 1.25);
        assert_eq!(pick_scale_from_dpi(140.0), 1.5);
        assert_eq!(pick_scale_from_dpi(169.9), 1.5);
    }

    #[test]
    fn scale_table_hidpi() {
        assert_eq!(pick_scale_from_dpi(170.0), 1.75);
        assert_eq!(pick_scale_from_dpi(219.9), 1.75);
        assert_eq!(pick_scale_from_dpi(220.0), 2.0);
        assert_eq!(pick_scale_from_dpi(300.0), 2.0);
    }
```

- [ ] **Step 2: Run tests — should fail**

```bash
cargo test algo::scale::
```

Expected: FAIL on the new tests (`unimplemented!()`).

- [ ] **Step 3: Implement**

Replace the stub bodies in `src/algo/scale.rs`:

```rust
pub fn compute_dpi(width_px: u32, height_px: u32, width_mm: u32, height_mm: u32) -> Option<f64> {
    if width_mm == 0 || height_mm == 0 {
        return None;
    }
    let w2 = (width_px as f64).powi(2);
    let h2 = (height_px as f64).powi(2);
    let diag_px = (w2 + h2).sqrt();

    let wm2 = (width_mm as f64).powi(2);
    let hm2 = (height_mm as f64).powi(2);
    let diag_mm = (wm2 + hm2).sqrt();
    let diag_in = diag_mm / 25.4;

    Some(diag_px / diag_in)
}

pub fn pick_scale_from_dpi(dpi: f64) -> f64 {
    match dpi {
        d if d < 110.0 => 1.0,
        d if d < 140.0 => 1.25,
        d if d < 170.0 => 1.5,
        d if d < 220.0 => 1.75,
        _ => 2.0,
    }
}
```

- [ ] **Step 4: Run tests — should pass**

```bash
cargo test algo::scale::
```

Expected: PASS (all scale tests, ~10 total).

- [ ] **Step 5: Commit**

```bash
git add src/algo/scale.rs
git commit -m "feat(algo): compute_dpi + pick_scale_from_dpi threshold table"
```

---

## Task 7: pick_scale orchestrator

**Files:**
- Modify: `src/algo/scale.rs`

- [ ] **Step 1: Append failing test for the orchestrator**

Add to `src/algo/scale.rs` (above the existing `#[cfg(test)]`):

```rust
use crate::model::Monitor;

/// Decide the scale for a monitor. Returns 1.0 if physical_mm is None,
/// width_mm/height_mm is zero, or DPI can't be computed.
pub fn pick_scale(monitor: &Monitor) -> f64 {
    unimplemented!()
}
```

Add tests inside the existing `#[cfg(test)] mod tests`:

```rust
    use crate::model::{Mode, Monitor};

    fn mon(name: &str, w_px: u32, h_px: u32, mm: Option<(u32, u32)>) -> Monitor {
        Monitor {
            name: name.to_string(),
            width_px: w_px,
            height_px: h_px,
            physical_mm: mm,
            available_modes: vec![Mode { width: w_px, height: h_px, refresh_hz: 60.0 }],
        }
    }

    #[test]
    fn scale_no_edid_falls_back_to_1() {
        assert_eq!(pick_scale(&mon("DP-1", 1920, 1080, None)), 1.0);
    }

    #[test]
    fn scale_24_inch_1080p_is_1() {
        // 530mm x 300mm ~ 24"
        assert_eq!(pick_scale(&mon("DP-1", 1920, 1080, Some((530, 300)))), 1.0);
    }

    #[test]
    fn scale_13_inch_4k_is_2() {
        // 286mm x 179mm ~ 13.3" with 3840x2400 ≈ 339 DPI
        assert_eq!(pick_scale(&mon("eDP-1", 3840, 2400, Some((286, 179)))), 2.0);
    }
```

- [ ] **Step 2: Run tests — should fail**

```bash
cargo test algo::scale::
```

Expected: FAIL on the new tests.

- [ ] **Step 3: Implement**

Replace the stub body in `src/algo/scale.rs`:

```rust
pub fn pick_scale(monitor: &Monitor) -> f64 {
    let Some((w_mm, h_mm)) = monitor.physical_mm else {
        return 1.0;
    };
    let Some(dpi) = compute_dpi(monitor.width_px, monitor.height_px, w_mm, h_mm) else {
        return 1.0;
    };
    pick_scale_from_dpi(dpi)
}
```

- [ ] **Step 4: Run tests — should pass**

```bash
cargo test algo::scale::
```

Expected: PASS (all scale tests).

- [ ] **Step 5: Commit**

```bash
git add src/algo/scale.rs
git commit -m "feat(algo): pick_scale orchestrator (EDID -> DPI -> scale)"
```

---

## Task 8: Layout positioning

**Files:**
- Create: `src/algo/layout.rs`
- Modify: `src/algo/mod.rs`

- [ ] **Step 1: Add module declaration**

Update `src/algo/mod.rs`:

```rust
pub mod layout;
pub mod mode;
pub mod primary;
pub mod scale;
```

- [ ] **Step 2: Write the failing tests**

Create `src/algo/layout.rs`:

```rust
use crate::model::Mode;

pub struct LayoutInput {
    pub name: String,
    pub mode: Mode,
    pub scale: f64,
}

/// Given monitors in their desired order, return (x, y) for each at y=0
/// extending left-to-right. Effective width = round(mode.width / scale).
pub fn arrange(inputs: &[LayoutInput]) -> Vec<(i32, i32)> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn li(name: &str, w: u32, h: u32, scale: f64) -> LayoutInput {
        LayoutInput {
            name: name.to_string(),
            mode: Mode { width: w, height: h, refresh_hz: 60.0 },
            scale,
        }
    }

    #[test]
    fn single_monitor_at_origin() {
        let positions = arrange(&[li("DP-1", 1920, 1080, 1.0)]);
        assert_eq!(positions, vec![(0, 0)]);
    }

    #[test]
    fn two_monitors_side_by_side() {
        let positions = arrange(&[
            li("eDP-1", 1920, 1080, 1.0),
            li("DP-1", 2560, 1440, 1.0),
        ]);
        assert_eq!(positions, vec![(0, 0), (1920, 0)]);
    }

    #[test]
    fn scale_reduces_effective_width() {
        // 3840 / 2.0 = 1920
        let positions = arrange(&[
            li("eDP-1", 3840, 2160, 2.0),
            li("DP-1", 1920, 1080, 1.0),
        ]);
        assert_eq!(positions, vec![(0, 0), (1920, 0)]);
    }

    #[test]
    fn fractional_scale_rounds() {
        // 2560 / 1.25 = 2048
        let positions = arrange(&[
            li("eDP-1", 2560, 1600, 1.25),
            li("DP-1", 1920, 1080, 1.0),
        ]);
        assert_eq!(positions, vec![(0, 0), (2048, 0)]);
    }

    #[test]
    fn empty_input_yields_empty() {
        let positions = arrange(&[]);
        assert_eq!(positions, Vec::<(i32, i32)>::new());
    }

    #[test]
    fn three_monitors_chain() {
        let positions = arrange(&[
            li("eDP-1", 1920, 1080, 1.0),
            li("DP-1", 2560, 1440, 1.0),
            li("HDMI-A-1", 1920, 1080, 1.0),
        ]);
        assert_eq!(positions, vec![(0, 0), (1920, 0), (4480, 0)]);
    }
}
```

- [ ] **Step 3: Run tests — should fail**

```bash
cargo test algo::layout::
```

Expected: FAIL.

- [ ] **Step 4: Implement**

Replace the function body in `src/algo/layout.rs`:

```rust
pub fn arrange(inputs: &[LayoutInput]) -> Vec<(i32, i32)> {
    let mut positions = Vec::with_capacity(inputs.len());
    let mut x: i32 = 0;
    for input in inputs {
        positions.push((x, 0));
        let effective_w = (input.mode.width as f64 / input.scale).round() as i32;
        x += effective_w;
    }
    positions
}
```

- [ ] **Step 5: Run tests — should pass**

```bash
cargo test algo::layout::
```

Expected: PASS (6 tests).

- [ ] **Step 6: Commit**

```bash
git add src/algo/layout.rs src/algo/mod.rs
git commit -m "feat(algo): arrange positions left-to-right at y=0"
```

---

## Task 9: plan() orchestrator

**Files:**
- Modify: `src/algo/mod.rs`

- [ ] **Step 1: Write the failing tests**

Replace the contents of `src/algo/mod.rs`:

```rust
pub mod layout;
pub mod mode;
pub mod primary;
pub mod scale;

use crate::model::{Mode, Monitor, MonitorConfig};

/// Plan configurations for the given monitors. The ordering rule:
/// internal panel (eDP/LVDS/DSI) first if present and active, then
/// externals sorted by connector name (lexicographic).
pub fn plan(monitors: &[Monitor]) -> Vec<MonitorConfig> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(w: u32, h: u32, hz: f64) -> Mode {
        Mode { width: w, height: h, refresh_hz: hz }
    }

    fn mon(name: &str, w: u32, h: u32, mm: Option<(u32, u32)>, modes: Vec<Mode>) -> Monitor {
        Monitor {
            name: name.to_string(),
            width_px: w,
            height_px: h,
            physical_mm: mm,
            available_modes: modes,
        }
    }

    #[test]
    fn empty_yields_empty_plan() {
        assert_eq!(plan(&[]), Vec::<MonitorConfig>::new());
    }

    #[test]
    fn single_external_monitor() {
        let monitors = vec![mon(
            "DP-1",
            2560,
            1440,
            Some((600, 340)),
            vec![mode(2560, 1440, 60.0), mode(2560, 1440, 165.0)],
        )];
        let result = plan(&monitors);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "DP-1");
        assert_eq!(result[0].mode, mode(2560, 1440, 165.0));
        assert_eq!(result[0].position, (0, 0));
        assert_eq!(result[0].scale, 1.0);
    }

    #[test]
    fn internal_placed_before_external() {
        let monitors = vec![
            mon("DP-1", 2560, 1440, Some((600, 340)),
                vec![mode(2560, 1440, 60.0)]),
            mon("eDP-1", 1920, 1080, Some((310, 170)),
                vec![mode(1920, 1080, 60.0)]),
        ];
        let result = plan(&monitors);
        assert_eq!(result[0].name, "eDP-1");
        assert_eq!(result[1].name, "DP-1");
        assert_eq!(result[0].position, (0, 0));
        // 1920 / 1.0 = 1920
        assert_eq!(result[1].position, (1920, 0));
    }

    #[test]
    fn externals_sorted_by_name() {
        let monitors = vec![
            mon("HDMI-A-1", 1920, 1080, Some((530, 300)),
                vec![mode(1920, 1080, 60.0)]),
            mon("DP-2", 2560, 1440, Some((600, 340)),
                vec![mode(2560, 1440, 60.0)]),
            mon("DP-1", 1920, 1080, Some((530, 300)),
                vec![mode(1920, 1080, 60.0)]),
        ];
        let result = plan(&monitors);
        assert_eq!(result[0].name, "DP-1");
        assert_eq!(result[1].name, "DP-2");
        assert_eq!(result[2].name, "HDMI-A-1");
    }

    #[test]
    fn monitor_with_no_modes_falls_back_to_current() {
        let monitors = vec![mon(
            "DP-1",
            1920,
            1080,
            None,
            vec![], // empty available_modes
        )];
        let result = plan(&monitors);
        assert_eq!(result.len(), 1);
        // Falls back to current width_px x height_px @ 60Hz (placeholder)
        assert_eq!(result[0].mode.width, 1920);
        assert_eq!(result[0].mode.height, 1080);
    }
}
```

- [ ] **Step 2: Run tests — should fail**

```bash
cargo test algo::tests
```

Expected: FAIL.

- [ ] **Step 3: Implement plan()**

Replace the `unimplemented!()` body in `src/algo/mod.rs`:

```rust
pub fn plan(monitors: &[Monitor]) -> Vec<MonitorConfig> {
    let mut sorted: Vec<&Monitor> = monitors.iter().collect();
    sorted.sort_by(|a, b| {
        let a_internal = primary::is_internal(&a.name);
        let b_internal = primary::is_internal(&b.name);
        match (a_internal, b_internal) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });

    let prepared: Vec<(String, Mode, f64)> = sorted
        .iter()
        .map(|m| {
            let chosen_mode = mode::pick_best_mode(&m.available_modes).unwrap_or(Mode {
                width: m.width_px,
                height: m.height_px,
                refresh_hz: 60.0,
            });
            let s = scale::pick_scale(m);
            (m.name.clone(), chosen_mode, s)
        })
        .collect();

    let layout_inputs: Vec<layout::LayoutInput> = prepared
        .iter()
        .map(|(name, mode, scale)| layout::LayoutInput {
            name: name.clone(),
            mode: mode.clone(),
            scale: *scale,
        })
        .collect();

    let positions = layout::arrange(&layout_inputs);

    prepared
        .into_iter()
        .zip(positions)
        .map(|((name, mode, scale), position)| MonitorConfig {
            name,
            mode,
            position,
            scale,
        })
        .collect()
}
```

- [ ] **Step 4: Run tests — should pass**

```bash
cargo test algo::tests
```

Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/algo/mod.rs
git commit -m "feat(algo): plan() orchestrator (sort, mode, scale, layout)"
```

---

## Task 10: Hyprland adapter — query

**Files:**
- Create: `src/hypr.rs`
- Modify: `src/main.rs` (add `mod hypr;`)

- [ ] **Step 1: Add the module**

Update `src/main.rs`:

```rust
mod algo;
mod hypr;
mod model;

fn main() {
    println!("Hello, world!");
}
```

- [ ] **Step 2: Write the adapter**

Create `src/hypr.rs`:

```rust
use crate::model::{Mode, Monitor, MonitorConfig};
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

/// Read all currently-connected monitors from Hyprland, enriched with
/// EDID-derived physical dimensions.
pub async fn query_monitors() -> Result<Vec<Monitor>> {
    let hypr_monitors = hyprland::data::Monitors::get_async()
        .await
        .context("hyprland::Monitors::get_async")?;

    let mut monitors = Vec::new();
    for hm in hypr_monitors.iter() {
        monitors.push(convert(hm));
    }
    Ok(monitors)
}

fn convert(hm: &hyprland::data::Monitor) -> Monitor {
    let modes = parse_available_modes(&hm.available_modes);
    let physical_mm = read_edid_for_connector(&hm.name);

    Monitor {
        name: hm.name.clone(),
        width_px: hm.width as u32,
        height_px: hm.height as u32,
        physical_mm,
        available_modes: modes,
    }
}

fn parse_available_modes(raw: &[String]) -> Vec<Mode> {
    raw.iter()
        .filter_map(|s| parse_mode_string(s))
        .collect()
}

fn parse_mode_string(s: &str) -> Option<Mode> {
    // Format: "1920x1080@60.000Hz" or "1920x1080@60Hz"
    let (res, hz) = s.split_once('@')?;
    let (w, h) = res.split_once('x')?;
    let width = w.trim().parse().ok()?;
    let height = h.trim().parse().ok()?;
    let hz_str = hz.trim_end_matches("Hz").trim();
    let refresh_hz: f64 = hz_str.parse().ok()?;
    Some(Mode { width, height, refresh_hz })
}

fn read_edid_for_connector(connector: &str) -> Option<(u32, u32)> {
    // /sys/class/drm/card?-<connector>/edid
    let drm = PathBuf::from("/sys/class/drm");
    let entries = fs::read_dir(&drm).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_str()?;
        if let Some(rest) = name.split_once('-') {
            if rest.1 == connector {
                let edid_path = entry.path().join("edid");
                if let Ok(bytes) = fs::read(&edid_path) {
                    return crate::algo::scale::parse_edid_dimensions(&bytes);
                }
            }
        }
    }
    None
}

/// Apply a single monitor configuration via `hyprctl keyword monitor`.
pub async fn apply(cfg: &MonitorConfig) -> Result<()> {
    let arg = cfg.to_string();
    hyprland::keyword::Keyword::set_async("monitor", arg.clone())
        .await
        .with_context(|| format!("hyprctl keyword monitor {}", arg))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mode_with_decimal_hz() {
        assert_eq!(
            parse_mode_string("1920x1080@59.951Hz"),
            Some(Mode { width: 1920, height: 1080, refresh_hz: 59.951 })
        );
    }

    #[test]
    fn parses_mode_with_integer_hz() {
        assert_eq!(
            parse_mode_string("2560x1440@165Hz"),
            Some(Mode { width: 2560, height: 1440, refresh_hz: 165.0 })
        );
    }

    #[test]
    fn parses_mode_without_hz_suffix() {
        assert_eq!(
            parse_mode_string("2560x1440@165"),
            Some(Mode { width: 2560, height: 1440, refresh_hz: 165.0 })
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_mode_string("garbage"), None);
        assert_eq!(parse_mode_string(""), None);
        assert_eq!(parse_mode_string("1920x@60Hz"), None);
    }
}
```

- [ ] **Step 3: Verify it compiles and tests pass**

```bash
cargo test hypr::
```

Expected: PASS (4 tests).

If the hyprland crate API differs (e.g., `Monitors::get_async` is named differently, `available_modes` field name differs, or `Keyword::set_async` is gone), look at the docs.rs page for the installed crate version and adjust the calls. The structure of `convert`, `parse_mode_string`, and `read_edid_for_connector` should stay the same.

- [ ] **Step 4: Commit**

```bash
git add src/hypr.rs src/main.rs
git commit -m "feat(hypr): query_monitors + apply via hyprland crate, EDID lookup"
```

---

## Task 11: Notification wrapper

**Files:**
- Create: `src/notify.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add the module**

Update `src/main.rs`:

```rust
mod algo;
mod hypr;
mod model;
mod notify;

fn main() {
    println!("Hello, world!");
}
```

- [ ] **Step 2: Write the wrapper**

Create `src/notify.rs`:

```rust
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
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check
```

Expected: PASS (no tests for this — manually smoke-tested later).

- [ ] **Step 4: Commit**

```bash
git add src/notify.rs src/main.rs
git commit -m "feat(notify): notify-send wrapper for failure path"
```

---

## Task 12: CLI scaffolding

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the CLI definitions**

Create `src/cli.rs`:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "hyprmonitor", version, about = "Auto-configure Hyprland monitors")]
pub struct Cli {
    #[arg(short, long, global = true, help = "Enable debug logging")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run as a daemon, reconfiguring on monitor hotplug events.
    Daemon,
    /// One-shot: detect monitors, apply best config, exit.
    Apply {
        #[arg(long, help = "Print hyprctl commands instead of running them")]
        dry_run: bool,
    },
    /// Print detected monitors and the plan without applying.
    List,
}

pub async fn run(cli: Cli) -> Result<()> {
    init_tracing(cli.verbose);
    match cli.command {
        Command::Daemon => {
            info!("daemon mode not yet implemented");
            Ok(())
        }
        Command::Apply { dry_run } => {
            info!("apply (dry_run={}) not yet implemented", dry_run);
            Ok(())
        }
        Command::List => {
            info!("list not yet implemented");
            Ok(())
        }
    }
}

fn init_tracing(verbose: bool) {
    let level = if verbose { "debug" } else { "info" };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(format!("hyprmonitor={}", level)));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
```

- [ ] **Step 2: Replace main.rs**

Replace `src/main.rs`:

```rust
mod algo;
mod cli;
mod hypr;
mod model;
mod notify;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Cli::parse();
    cli::run(args).await
}
```

- [ ] **Step 3: Verify it compiles and CLI parses**

```bash
cargo run -- --help
```

Expected: prints the help, listing `daemon`, `apply`, `list`, `--verbose`.

```bash
cargo run -- apply --dry-run
```

Expected: prints `apply (dry_run=true) not yet implemented`.

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat(cli): clap subcommands (daemon/apply/list) + tracing init"
```

---

## Task 13: Wire `list` subcommand

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Implement the list handler**

In `src/cli.rs`, replace the `Command::List` arm:

```rust
        Command::List => list().await,
```

Add this function at the bottom of `src/cli.rs`:

```rust
async fn list() -> Result<()> {
    let monitors = crate::hypr::query_monitors().await?;
    let plan = crate::algo::plan(&monitors);

    println!("Detected monitors:");
    for m in &monitors {
        println!(
            "  {} {}x{} physical_mm={:?} modes={}",
            m.name,
            m.width_px,
            m.height_px,
            m.physical_mm,
            m.available_modes.len()
        );
    }
    println!();
    println!("Plan:");
    for cfg in &plan {
        println!("  monitor = {}", cfg);
    }
    Ok(())
}
```

- [ ] **Step 2: Manual smoke test**

```bash
cargo run -- list
```

Expected: prints your real monitor(s) followed by a plan line like `monitor = DP-1,2560x1440@165,0x0,1`. If you see "physical_mm=None" for a monitor that should have an EDID, debug with `ls /sys/class/drm/` and `cat /sys/class/drm/card*-DP-1/edid | xxd | head` to see what's there.

- [ ] **Step 3: Commit**

```bash
git add src/cli.rs
git commit -m "feat(cli): list subcommand prints detected monitors + plan"
```

---

## Task 14: Wire `apply` subcommand

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Implement apply**

In `src/cli.rs`, replace the `Command::Apply { dry_run }` arm:

```rust
        Command::Apply { dry_run } => apply(dry_run).await,
```

Add at the bottom of `src/cli.rs`:

```rust
async fn apply(dry_run: bool) -> Result<()> {
    let monitors = crate::hypr::query_monitors().await?;
    let plan = crate::algo::plan(&monitors);

    if dry_run {
        for cfg in &plan {
            println!("hyprctl keyword monitor {}", cfg);
        }
        return Ok(());
    }

    for cfg in &plan {
        tracing::info!("applying {}", cfg);
        if let Err(e) = crate::hypr::apply(cfg).await {
            tracing::error!("apply failed for {}: {:?}", cfg.name, e);
            crate::notify::notify_failure(&format!(
                "Failed to configure {}: {}",
                cfg.name, e
            ));
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Smoke-test dry-run**

```bash
cargo run -- apply --dry-run
```

Expected: prints one `hyprctl keyword monitor <cfg>` line per monitor. No side effects on your display.

- [ ] **Step 3: Smoke-test live apply**

```bash
cargo run -- apply -v
```

Expected: monitors get configured as planned (you may see your screens flicker briefly). Check `hyprctl monitors` to confirm.

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs
git commit -m "feat(cli): apply subcommand (with --dry-run)"
```

---

## Task 15: Daemon — basic event loop

**Files:**
- Create: `src/daemon.rs`
- Modify: `src/main.rs` (add `mod daemon;`)
- Modify: `src/cli.rs` (wire daemon subcommand)

- [ ] **Step 1: Add the module**

Update `src/main.rs` — add `mod daemon;`:

```rust
mod algo;
mod cli;
mod daemon;
mod hypr;
mod model;
mod notify;
```

- [ ] **Step 2: Implement the basic loop (no debounce yet)**

Create `src/daemon.rs`:

```rust
use anyhow::Result;
use hyprland::event_listener::AsyncEventListener;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{error, info};

pub async fn run() -> Result<()> {
    info!("hyprmonitor daemon starting");
    reconfigure().await;

    let trigger = Arc::new(Notify::new());

    let trigger_added = trigger.clone();
    let trigger_removed = trigger.clone();

    let mut listener = AsyncEventListener::new();
    listener.add_monitor_added_handler(move |name| {
        let t = trigger_added.clone();
        Box::pin(async move {
            info!("monitor added: {}", name);
            t.notify_one();
        })
    });
    listener.add_monitor_removed_handler(move |name| {
        let t = trigger_removed.clone();
        Box::pin(async move {
            info!("monitor removed: {}", name);
            t.notify_one();
        })
    });

    // Spawn a task that reacts to events.
    let reactor_trigger = trigger.clone();
    tokio::spawn(async move {
        loop {
            reactor_trigger.notified().await;
            reconfigure().await;
        }
    });

    info!("subscribing to hyprland events");
    listener.start_listener_async().await?;
    Ok(())
}

async fn reconfigure() {
    let monitors = match crate::hypr::query_monitors().await {
        Ok(m) => m,
        Err(e) => {
            error!("failed to query monitors: {:?}", e);
            crate::notify::notify_failure(&format!("Failed to query monitors: {}", e));
            return;
        }
    };
    let plan = crate::algo::plan(&monitors);
    info!("applying {} monitor configs", plan.len());
    for cfg in &plan {
        if let Err(e) = crate::hypr::apply(cfg).await {
            error!("apply failed for {}: {:?}", cfg.name, e);
            crate::notify::notify_failure(&format!(
                "Failed to configure {}: {}",
                cfg.name, e
            ));
        }
    }
}
```

- [ ] **Step 3: Wire the daemon subcommand**

In `src/cli.rs`, replace the `Command::Daemon` arm:

```rust
        Command::Daemon => crate::daemon::run().await,
```

- [ ] **Step 4: Smoke-test the daemon**

```bash
cargo run -- daemon -v
```

Expected: logs "hyprmonitor daemon starting", applies initial config, then logs "subscribing to hyprland events" and stays running. Disconnect/reconnect a monitor (or use `hyprctl keyword monitor <name>,disable` and `,enable` to simulate). Confirm you see "monitor added/removed" logs and that reconfigure runs. Ctrl-C to stop.

If the `hyprland` crate's event listener API differs (e.g., `add_monitor_added_handler` is named differently or takes a different closure shape), check docs.rs for the installed version. Adjust the call sites; the rest of the file stays the same.

- [ ] **Step 5: Commit**

```bash
git add src/daemon.rs src/main.rs src/cli.rs
git commit -m "feat(daemon): event-driven reconfigure on monitor hotplug"
```

---

## Task 16: Daemon — 200ms debouncing

**Files:**
- Modify: `src/daemon.rs`

- [ ] **Step 1: Replace the reactor task with a debounced version**

In `src/daemon.rs`, replace the `tokio::spawn(...)` block with:

```rust
    let reactor_trigger = trigger.clone();
    tokio::spawn(async move {
        loop {
            reactor_trigger.notified().await;
            // Coalesce additional events arriving within 200ms.
            loop {
                let sleep = tokio::time::sleep(std::time::Duration::from_millis(200));
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => break,
                    _ = reactor_trigger.notified() => continue,
                }
            }
            reconfigure().await;
        }
    });
```

- [ ] **Step 2: Smoke-test**

```bash
cargo run -- daemon -v
```

In another terminal, fire a couple of rapid disable/enable cycles:

```bash
hyprctl keyword monitor DP-1,disable && sleep 0.05 && hyprctl keyword monitor DP-1,enable
```

(Replace `DP-1` with one of your real monitors.)

Expected: you see both events logged but only one `applying N monitor configs` line follows (within ~200ms after the last event), not two.

- [ ] **Step 3: Commit**

```bash
git add src/daemon.rs
git commit -m "feat(daemon): 200ms debounce on monitor events"
```

---

## Task 17: Daemon — reconnect with exponential backoff

**Files:**
- Modify: `src/daemon.rs`

- [ ] **Step 1: Wrap the listener in a backoff loop**

In `src/daemon.rs`, replace the final two lines of `run()`:

```rust
    info!("subscribing to hyprland events");
    listener.start_listener_async().await?;
    Ok(())
```

with a wrapping loop. Since the `listener` was already built above, we have to restructure: move the listener-construction and the start call into a helper that's invoked in a loop. Replace the full body of `run()`:

```rust
pub async fn run() -> Result<()> {
    info!("hyprmonitor daemon starting");
    reconfigure().await;

    let trigger = Arc::new(Notify::new());

    // Reactor task — debounced reconfigure on each notification.
    let reactor_trigger = trigger.clone();
    tokio::spawn(async move {
        loop {
            reactor_trigger.notified().await;
            loop {
                let sleep = tokio::time::sleep(std::time::Duration::from_millis(200));
                tokio::pin!(sleep);
                tokio::select! {
                    _ = &mut sleep => break,
                    _ = reactor_trigger.notified() => continue,
                }
            }
            reconfigure().await;
        }
    });

    // Listener task — reconnect with exponential backoff.
    let mut backoff_secs: u64 = 1;
    loop {
        let trigger_added = trigger.clone();
        let trigger_removed = trigger.clone();

        let mut listener = AsyncEventListener::new();
        listener.add_monitor_added_handler(move |name| {
            let t = trigger_added.clone();
            Box::pin(async move {
                info!("monitor added: {}", name);
                t.notify_one();
            })
        });
        listener.add_monitor_removed_handler(move |name| {
            let t = trigger_removed.clone();
            Box::pin(async move {
                info!("monitor removed: {}", name);
                t.notify_one();
            })
        });

        info!("subscribing to hyprland events");
        match listener.start_listener_async().await {
            Ok(()) => {
                info!("event listener exited cleanly; reconnecting");
                backoff_secs = 1;
            }
            Err(e) => {
                error!(
                    "event listener error: {:?}; retrying in {}s",
                    e, backoff_secs
                );
                tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(30);
                continue;
            }
        }

        // Listener exited cleanly. Try to reconnect immediately, and run
        // a reconfigure pass since Hyprland may have just restarted.
        reconfigure().await;
    }
}
```

- [ ] **Step 2: Smoke-test reconnect behavior**

```bash
cargo run -- daemon -v
```

In another terminal, stop the daemon would be hard to test directly without killing Hyprland — easiest sanity check: confirm the daemon still runs after several plug/unplug cycles without exiting. (Real Hyprland-crash recovery you can test by running the daemon, killing the Hyprland process briefly, and observing the daemon log "retrying in 1s" then "subscribing to hyprland events" once it's back. Skip this test if you don't want to kill your session.)

- [ ] **Step 3: Commit**

```bash
git add src/daemon.rs
git commit -m "feat(daemon): reconnect to hyprland socket with exponential backoff"
```

---

## Task 18: Verify-after-apply

**Files:**
- Modify: `src/daemon.rs`
- Modify: `src/cli.rs`

- [ ] **Step 1: Add a verify helper to daemon.rs**

Add to `src/daemon.rs`:

```rust
use crate::model::{Monitor, MonitorConfig};

async fn verify_applied(plan: &[MonitorConfig]) {
    let after = match crate::hypr::query_monitors().await {
        Ok(m) => m,
        Err(e) => {
            error!("verify: failed to re-query monitors: {:?}", e);
            return;
        }
    };
    for cfg in plan {
        let Some(actual) = after.iter().find(|m| m.name == cfg.name) else {
            crate::notify::notify_failure(&format!(
                "{} disappeared after apply",
                cfg.name
            ));
            continue;
        };
        if !mode_matches(actual, cfg) {
            crate::notify::notify_failure(&format!(
                "{}: requested {}x{}@{} but got {}x{}",
                cfg.name,
                cfg.mode.width,
                cfg.mode.height,
                cfg.mode.refresh_hz,
                actual.width_px,
                actual.height_px,
            ));
        }
    }
}

fn mode_matches(actual: &Monitor, cfg: &MonitorConfig) -> bool {
    // Our Monitor type only carries current resolution, not refresh rate.
    // Width/height mismatch is what we can detect reliably.
    actual.width_px == cfg.mode.width && actual.height_px == cfg.mode.height
}
```

Note: our `Monitor` type doesn't currently carry the *active* refresh rate (only `available_modes`). The verify check above only compares resolution. If we ever want refresh-rate verification, we'd add a `current_refresh_hz: f64` field to `Monitor` and populate it in `hypr::convert`. Out of scope for this task.

- [ ] **Step 2: Call verify_applied from reconfigure**

In `src/daemon.rs`, modify `reconfigure()`:

```rust
async fn reconfigure() {
    let monitors = match crate::hypr::query_monitors().await {
        Ok(m) => m,
        Err(e) => {
            error!("failed to query monitors: {:?}", e);
            crate::notify::notify_failure(&format!("Failed to query monitors: {}", e));
            return;
        }
    };
    let plan = crate::algo::plan(&monitors);
    info!("applying {} monitor configs", plan.len());
    for cfg in &plan {
        if let Err(e) = crate::hypr::apply(cfg).await {
            error!("apply failed for {}: {:?}", cfg.name, e);
            crate::notify::notify_failure(&format!(
                "Failed to configure {}: {}",
                cfg.name, e
            ));
        }
    }
    verify_applied(&plan).await;
}
```

- [ ] **Step 3: Smoke-test**

```bash
cargo run -- daemon -v
```

Plug/unplug a monitor. Confirm that the verify pass runs (you'll see `query_monitors` called twice per reconfigure — once for planning, once for verifying). No notifications should fire on success.

To force a failure notification: temporarily change `arrange_positions` to produce an invalid position (e.g., scale = -1.0) — should NOT do this in a real run, but useful to confirm the notify path works once.

- [ ] **Step 4: Commit**

```bash
git add src/daemon.rs
git commit -m "feat(daemon): verify-after-apply, notify on mode mismatch"
```

---

## Task 19: Fixture-based integration tests

**Files:**
- Create: `tests/fixtures/single_4k_laptop.json`
- Create: `tests/fixtures/laptop_plus_external.json`
- Create: `tests/plan_test.rs`

- [ ] **Step 1: Capture a real fixture**

Run on your machine and save the output:

```bash
hyprctl monitors -j > tests/fixtures/current_setup.json
```

(Move the file there manually if you don't have `tests/fixtures/` yet — `mkdir -p tests/fixtures` first.)

- [ ] **Step 2: Create two minimal hand-crafted fixtures**

These avoid depending on the user's specific setup. Create `tests/fixtures/single_4k_laptop.json`:

```json
[
  {
    "id": 0,
    "name": "eDP-1",
    "width": 3840,
    "height": 2400,
    "availableModes": ["3840x2400@60.000Hz", "1920x1200@60.000Hz"]
  }
]
```

Create `tests/fixtures/laptop_plus_external.json`:

```json
[
  {
    "id": 0,
    "name": "eDP-1",
    "width": 1920,
    "height": 1080,
    "availableModes": ["1920x1080@60.000Hz"]
  },
  {
    "id": 1,
    "name": "DP-1",
    "width": 2560,
    "height": 1440,
    "availableModes": ["2560x1440@60.000Hz", "2560x1440@165.000Hz"]
  }
]
```

- [ ] **Step 3: Write the integration test**

Create `tests/plan_test.rs`:

```rust
use hyprmonitor::algo::plan;
use hyprmonitor::model::{Mode, Monitor};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct HyprMonitorFixture {
    name: String,
    width: u32,
    height: u32,
    #[serde(rename = "availableModes")]
    available_modes: Vec<String>,
}

fn load(path: &str) -> Vec<Monitor> {
    let bytes = fs::read(Path::new(path)).expect("fixture not found");
    let raw: Vec<HyprMonitorFixture> = serde_json::from_slice(&bytes).expect("parse json");
    raw.into_iter()
        .map(|f| Monitor {
            name: f.name,
            width_px: f.width,
            height_px: f.height,
            physical_mm: None, // fixtures don't carry EDID
            available_modes: f
                .available_modes
                .into_iter()
                .filter_map(|s| parse_mode(&s))
                .collect(),
        })
        .collect()
}

fn parse_mode(s: &str) -> Option<Mode> {
    let (res, hz) = s.split_once('@')?;
    let (w, h) = res.split_once('x')?;
    let hz = hz.trim_end_matches("Hz");
    Some(Mode {
        width: w.parse().ok()?,
        height: h.parse().ok()?,
        refresh_hz: hz.parse().ok()?,
    })
}

#[test]
fn single_4k_laptop_picks_native_at_max_hz() {
    let monitors = load("tests/fixtures/single_4k_laptop.json");
    let plan = plan(&monitors);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].name, "eDP-1");
    assert_eq!(plan[0].mode.width, 3840);
    assert_eq!(plan[0].mode.height, 2400);
    assert_eq!(plan[0].position, (0, 0));
    // No EDID in fixture → scale falls back to 1.0
    assert_eq!(plan[0].scale, 1.0);
}

#[test]
fn laptop_plus_external_orders_internal_first() {
    let monitors = load("tests/fixtures/laptop_plus_external.json");
    let plan = plan(&monitors);
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].name, "eDP-1");
    assert_eq!(plan[1].name, "DP-1");
    assert_eq!(plan[0].position, (0, 0));
    assert_eq!(plan[1].position, (1920, 0));
    assert_eq!(plan[1].mode.refresh_hz, 165.0);
}
```

- [ ] **Step 4: Expose the library surface**

The `tests/` directory tests link against the crate as a library. Add a lib target to make `hyprmonitor::algo` and `hyprmonitor::model` accessible.

Create `src/lib.rs`:

```rust
pub mod algo;
pub mod model;
```

Update `Cargo.toml` to declare both targets (add this section if it's not already there):

```toml
[lib]
name = "hyprmonitor"
path = "src/lib.rs"

[[bin]]
name = "hyprmonitor"
path = "src/main.rs"
```

In `src/main.rs`, change the existing `mod algo;` and `mod model;` lines to `use` them from the lib instead, since they're now `pub` modules of the crate library:

```rust
mod cli;
mod daemon;
mod hypr;
mod notify;

use anyhow::Result;
use clap::Parser;
use hyprmonitor::{algo, model};

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Cli::parse();
    cli::run(args).await
}
```

Now update references inside `src/cli.rs`, `src/daemon.rs`, and `src/hypr.rs`: change `crate::algo::` to `hyprmonitor::algo::` and `crate::model::` to `hyprmonitor::model::`. Other `crate::` references (to `cli`, `daemon`, `hypr`, `notify`) stay the same.

- [ ] **Step 5: Run the integration tests**

```bash
cargo test --test plan_test
```

Expected: PASS (2 tests).

Also re-run all unit tests:

```bash
cargo test
```

Expected: all tests pass, ~30+ tests total across model/algo/hypr/plan_test.

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/main.rs src/cli.rs src/daemon.rs src/hypr.rs Cargo.toml tests/
git commit -m "feat: lib target + fixture-based plan() integration tests"
```

---

## Task 20: Final manual end-to-end check

**Files:** none — verification only.

- [ ] **Step 1: Build a release binary**

```bash
cargo build --release
```

Expected: PASS, no warnings (or only warnings you've decided to accept). Binary at `target/release/hyprmonitor`.

- [ ] **Step 2: Run the full daemon end-to-end**

```bash
./target/release/hyprmonitor daemon -v
```

In another terminal, fire a real hotplug-equivalent (unplug a real monitor cable, or `hyprctl keyword monitor DP-1,disable; hyprctl keyword monitor DP-1,enable`).

Expected:
- Initial reconfigure on startup.
- "monitor removed" / "monitor added" logs on events.
- Debounced reconfigure runs ~200ms after the last event.
- `hyprctl monitors` reflects the planned positions/modes/scales.

- [ ] **Step 3: Set up an exec-once line for your hyprland.conf**

Print a copyable line:

```bash
echo "exec-once = $(realpath ./target/release/hyprmonitor) daemon"
```

The user can add that line to `~/.config/hypr/hyprland.conf` so the daemon starts with the session. (Don't auto-edit their config — leave that to them.)

- [ ] **Step 4: Final commit (no code, just confirms completion)**

If you made any cleanup fixes during this manual pass, commit them. Otherwise nothing to do.

---

## Self-Review

After writing the plan I re-read the spec section by section:

- ✅ **CLI surface** (spec §CLI) → Task 12 (scaffolding), Task 13 (`list`), Task 14 (`apply`), Task 15 (`daemon`).
- ✅ **Architecture** (spec §Architecture) → file structure section + module-per-task layout matches.
- ✅ **Best-mode algorithm** (spec §Algorithm: best mode) → Task 4. Empty `availableModes` fallback handled in Task 9 (plan).
- ✅ **Smart-primary layout** (spec §Algorithm: smart-primary layout) → Task 3 (`is_internal`) + Task 8 (positions) + Task 9 (ordering in `plan`).
- ✅ **DPI scale** (spec §Algorithm: DPI-based scale) → Tasks 5 (EDID parse), 6 (DPI compute + table), 7 (orchestrator). EDID-from-`/sys` in Task 10.
- ✅ **Event loop** (spec §Event loop) → Task 15 (initial reconfigure + handlers), Task 16 (debounce 200ms), Task 17 (reconnect with backoff).
- ✅ **Error handling** (spec §Error handling) → notify wrapper Task 11, hyprctl-fails handled in Task 14/15, verify-after-apply Task 18, EDID fallback Task 7, empty `availableModes` Task 9, socket reconnect Task 17, `notify-send` failure swallowed Task 11.
- ✅ **Testing** (spec §Testing) → unit tests in Tasks 2–9; fixture-based integration tests in Task 19.

**Placeholder scan:** Searched for "TBD", "TODO", "implement later", "appropriate". One spot in Task 18 explicitly notes a deferred decision (refresh-rate verification) — labeled as out of scope, not a placeholder.

**Type consistency:** Cross-checked `Monitor`/`Mode`/`MonitorConfig` field names across Tasks 2, 4, 6, 7, 8, 9, 10, 19. All match. `LayoutInput` (Task 8) is used by `plan` in Task 9 with the same fields. `physical_mm` is consistently `Option<(u32, u32)>` everywhere.
