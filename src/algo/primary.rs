use regex::Regex;

pub fn is_internal(name: &str) -> bool {
    let re = Regex::new(r"^(?i)(eDP|LVDS|DSI)-\d+$").expect("static regex");
    re.is_match(name)
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
