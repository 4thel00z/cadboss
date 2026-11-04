//! Short decimal rendering of measurements.

/// Two decimals with trailing zeros and a trailing point removed; negative zero prints as `0`.
pub fn format_number(value: f64) -> String {
    let fixed = format!("{value:.2}");
    let trimmed = fixed.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        return "0".to_owned();
    }
    trimmed.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_trim_trailing_zeros() {
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(65.0), "65");
        assert_eq!(format_number(100.0), "100");
        assert_eq!(format_number(12.5), "12.5");
        assert_eq!(format_number(3.456), "3.46");
        assert_eq!(format_number(-0.001), "0");
        assert_eq!(format_number(f64::NAN), "NaN");
    }
}
