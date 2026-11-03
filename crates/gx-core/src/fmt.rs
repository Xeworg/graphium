//! Canonical number formatting (PRD §8.5.1).
//!
//! Every numeric attribute of the SVG goes through [`fmt_num`]: two decimals,
//! round-half-to-even and no negative zero. A single formatter is the only
//! way the golden hashes stay stable across machines.

/// Formats a float with the canonical rules of PRD §8.5.1.
pub fn fmt_num(value: f64) -> String {
    if !value.is_finite() {
        return "0.00".to_string();
    }
    let rendered = format!("{value:.2}");
    if rendered == "-0.00" {
        "0.00".to_string()
    } else {
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::fmt_num;

    #[test]
    fn keeps_two_decimals() {
        assert_eq!(fmt_num(1.0), "1.00");
        assert_eq!(fmt_num(12.5), "12.50");
        assert_eq!(fmt_num(-3.25), "-3.25");
    }

    #[test]
    fn rounds_half_to_even() {
        // 0.125 is exact in binary: the tie goes to the even neighbour.
        assert_eq!(fmt_num(0.125), "0.12");
        assert_eq!(fmt_num(0.135), "0.14");
        // 2.345 is 2.345000000000000195... in binary, so it rounds up.
        assert_eq!(fmt_num(2.345), "2.35");
        assert_eq!(fmt_num(2.125), "2.12");
    }

    #[test]
    fn drops_negative_zero() {
        assert_eq!(fmt_num(-0.001), "0.00");
        assert_eq!(fmt_num(-0.0), "0.00");
    }

    #[test]
    fn non_finite_values_are_zero() {
        assert_eq!(fmt_num(f64::NAN), "0.00");
        assert_eq!(fmt_num(f64::INFINITY), "0.00");
    }
}
