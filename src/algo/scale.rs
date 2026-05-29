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
}
