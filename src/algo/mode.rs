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
