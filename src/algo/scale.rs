/// Decide the scale for the resolution we're about to apply.
///
/// Computes DPI from the *chosen* mode size (not the monitor's current
/// resolution), so picking a non-native mode produces the scale matching
/// what we'll actually be running at.
///
/// Returns 1.0 if `physical_mm` is None, either mm dimension is zero, or
/// DPI can't be computed.
pub fn pick_scale(width_px: u32, height_px: u32, physical_mm: Option<(u32, u32)>) -> f64 {
    let Some((w_mm, h_mm)) = physical_mm else {
        return 1.0;
    };
    let Some(dpi) = compute_dpi(width_px, height_px, w_mm, h_mm) else {
        return 1.0;
    };
    pick_scale_from_dpi(dpi)
}

/// Parse maximum image size (cm) from EDID block at bytes 0x15-0x16,
/// returning (width_mm, height_mm). Returns None if header is invalid
/// or either dimension is zero.
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

/// Parse the EDID's preferred (native) resolution from the first detailed
/// timing descriptor at bytes 54-71. Returns (h_active, v_active) in pixels,
/// or None if the header is invalid, the buffer is short, or the first
/// descriptor isn't a timing descriptor (pixel clock = 0 means it's a
/// monitor descriptor like manufacturer name).
pub fn parse_edid_preferred_mode(edid: &[u8]) -> Option<(u32, u32)> {
    const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
    if edid.len() < 62 {
        return None;
    }
    if edid[0..8] != HEADER {
        return None;
    }
    // Pixel clock at bytes 54-55 (little-endian, in 10 kHz units). Zero
    // means this slot holds a monitor descriptor, not a timing descriptor.
    let pixel_clock = u16::from_le_bytes([edid[54], edid[55]]);
    if pixel_clock == 0 {
        return None;
    }
    let h_active = (edid[56] as u32) | (((edid[58] >> 4) as u32) << 8);
    let v_active = (edid[59] as u32) | (((edid[61] >> 4) as u32) << 8);
    if h_active == 0 || v_active == 0 {
        return None;
    }
    Some((h_active, v_active))
}

/// Compute physical DPI given pixel size and millimeter size.
/// Returns None if either mm dimension is zero.
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

/// Map DPI to a Hyprland scale factor from the spec table.
pub fn pick_scale_from_dpi(dpi: f64) -> f64 {
    match dpi {
        d if d < 110.0 => 1.0,
        d if d < 140.0 => 1.25,
        d if d < 170.0 => 1.5,
        d if d < 220.0 => 1.75,
        _ => 2.0,
    }
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

    fn edid_with_preferred(h_active: u32, v_active: u32, pixel_clock: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        let pc = pixel_clock.to_le_bytes();
        bytes[54] = pc[0];
        bytes[55] = pc[1];
        bytes[56] = (h_active & 0xFF) as u8;
        bytes[58] = (((h_active >> 8) & 0x0F) as u8) << 4;
        bytes[59] = (v_active & 0xFF) as u8;
        bytes[61] = (((v_active >> 8) & 0x0F) as u8) << 4;
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

    #[test]
    fn parses_preferred_1080p() {
        let edid = edid_with_preferred(1920, 1080, 14850);
        assert_eq!(parse_edid_preferred_mode(&edid), Some((1920, 1080)));
    }

    #[test]
    fn parses_preferred_4k() {
        // 3840 needs the upper nibble: 3840 = 0xF00, low byte = 0x00, high nibble = 0xF
        let edid = edid_with_preferred(3840, 2160, 53850);
        assert_eq!(parse_edid_preferred_mode(&edid), Some((3840, 2160)));
    }

    #[test]
    fn preferred_zero_pixel_clock_returns_none() {
        // Pixel clock 0 means the descriptor is a monitor descriptor, not timing.
        let edid = edid_with_preferred(1920, 1080, 0);
        assert_eq!(parse_edid_preferred_mode(&edid), None);
    }

    #[test]
    fn preferred_invalid_header_returns_none() {
        let mut edid = vec![0u8; 128];
        edid[54] = 0x80;
        edid[55] = 0x39;
        edid[56] = 0x80;
        assert_eq!(parse_edid_preferred_mode(&edid), None);
    }

    #[test]
    fn preferred_short_buffer_returns_none() {
        let edid = vec![0u8; 30];
        assert_eq!(parse_edid_preferred_mode(&edid), None);
    }

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

    #[test]
    fn scale_no_edid_falls_back_to_1() {
        assert_eq!(pick_scale(1920, 1080, None), 1.0);
    }

    #[test]
    fn scale_24_inch_1080p_is_1() {
        // 530mm x 300mm ~ 24"
        assert_eq!(pick_scale(1920, 1080, Some((530, 300))), 1.0);
    }

    #[test]
    fn scale_13_inch_4k_is_2() {
        // 286mm x 179mm ~ 13.3" with 3840x2400 ≈ 339 DPI
        assert_eq!(pick_scale(3840, 2400, Some((286, 179))), 2.0);
    }

    #[test]
    fn scale_uses_chosen_resolution_not_panel_max() {
        // Same 24" panel, but if we choose to drive it at 1080p, the scale
        // should reflect 1080p density (~92 DPI → 1.0), not whatever a 4K
        // signal would imply.
        assert_eq!(pick_scale(1920, 1080, Some((530, 300))), 1.0);
        assert_eq!(pick_scale(3840, 2160, Some((530, 300))), 1.75);
    }
}
