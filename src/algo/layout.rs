use crate::model::Mode;

#[allow(dead_code)]
pub struct LayoutInput {
    pub name: String,
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
