use crate::model::Mode;

/// Pick the best mode for a monitor.
///
/// When `preferred` is `Some((w, h))` and at least one mode matches that
/// resolution, the chosen mode is the highest-Hz one at that resolution —
/// EDID's preferred timing is treated as authoritative about the native
/// panel size, which guards against monitors that advertise upscaled modes
/// (e.g. a 1080p panel listing 3840x2160 as an available mode).
///
/// When no mode matches the hint (or `preferred` is `None`), falls back to
/// max pixel count then max Hz.
pub fn pick_best_mode(modes: &[Mode], preferred: Option<(u32, u32)>) -> Option<Mode> {
    if let Some((pw, ph)) = preferred {
        let best_at_preferred = modes
            .iter()
            .filter(|m| m.width == pw && m.height == ph)
            .max_by(|a, b| {
                a.refresh_hz
                    .partial_cmp(&b.refresh_hz)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned();
        if best_at_preferred.is_some() {
            return best_at_preferred;
        }
    }

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
        assert_eq!(pick_best_mode(&modes, None), Some(m(2560, 1440, 165.0)));
    }

    #[test]
    fn single_mode_picked() {
        let modes = vec![m(1920, 1080, 60.0)];
        assert_eq!(pick_best_mode(&modes, None), Some(m(1920, 1080, 60.0)));
    }

    #[test]
    fn empty_returns_none() {
        assert_eq!(pick_best_mode(&[], None), None);
    }

    #[test]
    fn fractional_hz_picks_highest() {
        let modes = vec![
            m(3840, 2160, 59.951),
            m(3840, 2160, 60.0),
            m(3840, 2160, 59.94),
        ];
        assert_eq!(pick_best_mode(&modes, None), Some(m(3840, 2160, 60.0)));
    }

    #[test]
    fn preferred_hint_overrides_max_pixels() {
        // A 1080p panel that advertises upscaled 4K. EDID says preferred is 1080p.
        let modes = vec![
            m(3840, 2160, 30.0),
            m(1920, 1080, 60.0),
            m(1920, 1080, 75.0),
        ];
        assert_eq!(
            pick_best_mode(&modes, Some((1920, 1080))),
            Some(m(1920, 1080, 75.0))
        );
    }

    #[test]
    fn preferred_hint_picks_max_hz_at_native() {
        let modes = vec![
            m(2560, 1440, 60.0),
            m(2560, 1440, 144.0),
            m(2560, 1440, 165.0),
        ];
        assert_eq!(
            pick_best_mode(&modes, Some((2560, 1440))),
            Some(m(2560, 1440, 165.0))
        );
    }

    #[test]
    fn preferred_hint_with_no_match_falls_back() {
        // EDID says preferred is 1366x768 but no mode advertises it.
        let modes = vec![m(1920, 1080, 60.0), m(2560, 1440, 60.0)];
        assert_eq!(
            pick_best_mode(&modes, Some((1366, 768))),
            Some(m(2560, 1440, 60.0))
        );
    }
}
