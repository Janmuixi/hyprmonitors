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
