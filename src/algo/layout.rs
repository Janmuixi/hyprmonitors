use crate::model::{Mode, MonitorConfig};

pub struct LayoutInput {
    pub mode: Mode,
    pub scale: f64,
}

/// Given monitors in their desired order, return (x, y) for each at y=0
/// extending left-to-right. Effective width = round(mode.width / scale).
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

/// Generous snap threshold for re-gluing a saved layout at apply time. Matches
/// the GUI's align-on-save so both paths close the same size of gap.
pub const SNAP_PX_ALIGN: i32 = 200;

/// Logical on-screen size of a monitor, i.e. how many layout pixels it occupies.
/// This is `round(pixels / scale)` — the same rounding Hyprland uses to derive
/// an output's logical size — with the axes swapped for portrait rotations.
pub fn footprint(cfg: &MonitorConfig) -> (i32, i32) {
    let w = (cfg.mode.width as f64 / cfg.scale).round() as i32;
    let h = (cfg.mode.height as f64 / cfg.scale).round() as i32;
    if cfg.rotation == 90 || cfg.rotation == 270 {
        (h, w)
    } else {
        (w, h)
    }
}

/// Re-glue a saved absolute layout so adjacent monitors touch again after the
/// connected set has changed. Positions are stored per-monitor as absolute
/// coordinates captured for one particular combination; when a different
/// monitor takes a shared connector (or one is added), the fixed coordinates no
/// longer chain up and small gaps appear. This snaps every enabled monitor's
/// edges to its neighbours within `snap_px`, mirroring the GUI's align-on-save
/// so the daemon closes the same gaps on hotplug. Gaps wider than `snap_px` are
/// left alone (deliberately separated monitors stay separated).
pub fn close_gaps(configs: &mut [MonitorConfig], snap_px: i32) {
    // Process left-to-right so each monitor snaps against already-snapped
    // neighbours, letting a chain of small gaps collapse in one pass.
    let mut order: Vec<usize> = (0..configs.len()).collect();
    order.sort_by_key(|&i| configs[i].position.0);
    for idx in order {
        if configs[idx].disabled {
            continue;
        }
        let (dx, dy) = snap_delta(configs, idx, snap_px);
        configs[idx].position.0 += dx;
        configs[idx].position.1 += dy;
    }
}

/// Smallest edge-aligning shift for `configs[idx]` against every other enabled
/// monitor, within `snap_px` on each axis. Pure; returns `(dx, dy)`.
fn snap_delta(configs: &[MonitorConfig], idx: usize, snap_px: i32) -> (i32, i32) {
    let me = &configs[idx];
    let (mw, mh) = footprint(me);
    let me_left = me.position.0;
    let me_top = me.position.1;
    let me_right = me_left + mw;
    let me_bottom = me_top + mh;

    let mut best_dx: Option<i32> = None;
    let mut best_dy: Option<i32> = None;

    for (j, other) in configs.iter().enumerate() {
        if j == idx || other.disabled {
            continue;
        }
        let (ow, oh) = footprint(other);
        let o_left = other.position.0;
        let o_top = other.position.1;
        let o_right = o_left + ow;
        let o_bottom = o_top + oh;

        for (a, b) in [
            (me_right, o_left),
            (me_left, o_right),
            (me_left, o_left),
            (me_right, o_right),
        ] {
            let dx = b - a;
            if dx.abs() <= snap_px && best_dx.map_or(true, |bd| dx.abs() < bd.abs()) {
                best_dx = Some(dx);
            }
        }
        for (a, b) in [
            (me_bottom, o_top),
            (me_top, o_bottom),
            (me_top, o_top),
            (me_bottom, o_bottom),
        ] {
            let dy = b - a;
            if dy.abs() <= snap_px && best_dy.map_or(true, |bd| dy.abs() < bd.abs()) {
                best_dy = Some(dy);
            }
        }
    }

    (best_dx.unwrap_or(0), best_dy.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn li(_name: &str, w: u32, h: u32, scale: f64) -> LayoutInput {
        LayoutInput {
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

    use crate::model::MonitorConfig;

    fn cfg(name: &str, w: u32, h: u32, scale: f64, x: i32, y: i32) -> MonitorConfig {
        MonitorConfig {
            name: name.to_string(),
            mode: Mode { width: w, height: h, refresh_hz: 60.0 },
            position: (x, y),
            scale,
            rotation: 0,
            disabled: false,
        }
    }

    /// Every enabled monitor's right edge must equal the next one's left edge:
    /// a contiguous, gapless, overlap-free horizontal chain.
    fn assert_glued(plan: &[MonitorConfig]) {
        let mut chain: Vec<&MonitorConfig> = plan.iter().filter(|c| !c.disabled).collect();
        chain.sort_by_key(|c| c.position.0);
        for pair in chain.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let a_right = a.position.0 + footprint(a).0;
            assert_eq!(
                a_right, b.position.0,
                "{} ends at {} but {} starts at {} — not glued",
                a.name, a_right, b.name, b.position.0
            );
        }
    }

    #[test]
    fn close_gaps_reglues_home_layout_after_hdmi_swap() {
        // Real regression: eDP + a 1080p HDMI panel + DP-3. eDP ends at x=1440,
        // but the HDMI panel's saved x is 1570 (those coords were captured with
        // a 4K monitor on the same port), leaving a 130px gap on eDP's right.
        let mut plan = vec![
            cfg("eDP-1", 2880, 1800, 2.0, 0, 180), // eff 1440x900, ends x=1440
            cfg("HDMI-A-1", 1920, 1080, 1.0, 1570, 0), // 130px gap after eDP
            cfg("DP-3", 1920, 1080, 1.0, 3490, 0),
        ];
        close_gaps(&mut plan, SNAP_PX_ALIGN);
        assert_glued(&plan);
    }

    #[test]
    fn close_gaps_reglues_office_layout() {
        // eDP + 4K HDMI (already glued) + DP-3 whose saved x=3490 was captured
        // for the 1080p HDMI panel, so it sits 130px past the 4K panel's right
        // edge (3360). close_gaps must pull DP-3 left to close that gap.
        let mut plan = vec![
            cfg("eDP-1", 2880, 1800, 2.0, 0, 180),
            cfg("HDMI-A-1", 3840, 2160, 2.0, 1440, 0), // eff 1920x1080, ends 3360
            cfg("DP-3", 1920, 1080, 1.0, 3490, 0),     // 130px gap after 4K
        ];
        close_gaps(&mut plan, SNAP_PX_ALIGN);
        assert_glued(&plan);
    }

    #[test]
    fn close_gaps_preserves_intentional_vertical_offset() {
        // eDP is bottom-aligned (y=180) so its 900px height lines up with the
        // 1080px panel. Closing horizontal gaps must not yank it to y=0.
        let mut plan = vec![
            cfg("eDP-1", 2880, 1800, 2.0, 0, 180),
            cfg("HDMI-A-1", 3840, 2160, 2.0, 1440, 0),
        ];
        close_gaps(&mut plan, SNAP_PX_ALIGN);
        let edp = plan.iter().find(|c| c.name == "eDP-1").unwrap();
        assert_eq!(edp.position.1, 180, "bottom-aligned offset must survive");
    }

    #[test]
    fn close_gaps_leaves_large_intentional_gaps() {
        // A gap wider than the snap threshold is a deliberate layout, not drift.
        let mut plan = vec![
            cfg("DP-1", 1920, 1080, 1.0, 0, 0),
            cfg("DP-2", 1920, 1080, 1.0, 3000, 0), // gap 1080 > 200
        ];
        close_gaps(&mut plan, SNAP_PX_ALIGN);
        assert_eq!(plan[1].position.0, 3000, "gaps beyond snap_px stay put");
    }

    #[test]
    fn close_gaps_ignores_disabled_monitors() {
        let mut plan = vec![
            cfg("eDP-1", 1920, 1080, 1.0, 0, 0),
            {
                let mut c = cfg("HDMI-A-1", 1920, 1080, 1.0, 1950, 0); // 30px gap
                c.disabled = true;
                c
            },
        ];
        close_gaps(&mut plan, SNAP_PX_ALIGN);
        assert_eq!(plan[0].position.0, 0, "must not snap toward a disabled monitor");
        assert_eq!(plan[1].position.0, 1950, "a disabled monitor must not move");
    }
}
