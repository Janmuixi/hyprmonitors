pub mod layout;
pub mod mode;
pub mod primary;
pub mod scale;

use crate::model::{Mode, Monitor, MonitorConfig};

/// Plan configurations for the given monitors. The ordering rule:
/// internal panel (eDP/LVDS/DSI) first if present and active, then
/// externals sorted by connector name (lexicographic).
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
            mon("eDP-1", 1920, 1080, None,
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
