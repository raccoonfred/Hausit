use serde::{Deserialize, Serialize};

/// Display unit for lengths. Values are always stored in millimetres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    #[default]
    Millimeters,
    Meters,
    FeetInches,
}

impl LengthUnit {
    pub const ALL: [LengthUnit; 3] = [Self::Millimeters, Self::Meters, Self::FeetInches];

    pub fn label(self) -> &'static str {
        match self {
            Self::Millimeters => "Millimeters",
            Self::Meters => "Meters",
            Self::FeetInches => "Feet and fractional inches",
        }
    }
}

pub const MM_PER_FOOT: f64 = 304.8;
pub const MM_PER_INCH: f64 = 25.4;

/// Formats a length in millimetres for display.
pub fn format_length(mm: f64, unit: LengthUnit) -> String {
    match unit {
        LengthUnit::Millimeters => format!("{}", round_to(mm, 1.0)),
        LengthUnit::Meters => format!("{:.3}", mm / 1000.0),
        LengthUnit::FeetInches => {
            let sign = if mm < 0.0 { "-" } else { "" };
            let total_in = (mm.abs() / MM_PER_INCH * 256.0).round() / 256.0;
            let feet = (total_in / 12.0).floor();
            let inches = total_in - feet * 12.0;
            let whole = inches.floor();
            let frac = ((inches - whole) * 256.0).round() as u32;
            let frac_str = if frac == 0 {
                String::new()
            } else {
                let g = gcd(frac, 256);
                format!(" {}/{}", frac / g, 256 / g)
            };
            format!("{sign}{feet}' - {whole}{frac_str}\"")
        }
    }
}

/// Parses user input into millimetres. Accepts plain numbers (interpreted in
/// `unit`), and explicit suffixes `mm`, `m`, `'` and `"`.
pub fn parse_length(input: &str, unit: LengthUnit) -> Option<f64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(v) = s.strip_suffix("mm") {
        return v.trim().parse().ok();
    }
    if let Some(v) = s.strip_suffix('m') {
        return v.trim().parse::<f64>().ok().map(|m| m * 1000.0);
    }
    if s.contains('\'') || s.contains('"') {
        let mut total = 0.0;
        let mut rest = s;
        if let Some(i) = rest.find('\'') {
            total += rest[..i].trim().parse::<f64>().ok()? * MM_PER_FOOT;
            rest = rest[i + 1..].trim().trim_start_matches('-').trim();
        }
        let rest = rest.trim_end_matches('"').trim();
        if !rest.is_empty() {
            total += rest.parse::<f64>().ok()? * MM_PER_INCH;
        }
        return Some(total);
    }
    let v: f64 = s.parse().ok()?;
    Some(match unit {
        LengthUnit::Millimeters => v,
        LengthUnit::Meters => v * 1000.0,
        LengthUnit::FeetInches => v * MM_PER_FOOT,
    })
}

fn round_to(v: f64, step: f64) -> f64 {
    let r = (v / step).round() * step;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format() {
        assert_eq!(parse_length("3000", LengthUnit::Millimeters), Some(3000.0));
        assert_eq!(parse_length("3m", LengthUnit::Millimeters), Some(3000.0));
        assert_eq!(parse_length("1'", LengthUnit::Millimeters), Some(304.8));
        assert_eq!(parse_length("1' 6\"", LengthUnit::Millimeters), Some(457.2));
        assert_eq!(format_length(2999.6, LengthUnit::Millimeters), "3000");
        assert_eq!(format_length(457.2, LengthUnit::FeetInches), "1' - 6\"");
    }
}
