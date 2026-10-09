//! Durations (`500ms`, `10s`) and printing readings back.

use crate::protocol::Reading;
use anyhow::{Result, anyhow, bail};
use std::time::Duration;

/// `500ms`, `10s`, `2m`, `1h`; a bare number is seconds.
pub fn duration(text: &str) -> Result<Duration> {
    let t = text.trim();
    let (num, scale) = if let Some(n) = t.strip_suffix("ms") {
        (n, 1e-3)
    } else if let Some(n) = t.strip_suffix('s') {
        (n, 1.0)
    } else if let Some(n) = t.strip_suffix('m') {
        (n, 60.0)
    } else if let Some(n) = t.strip_suffix('h') {
        (n, 3600.0)
    } else {
        (t, 1.0)
    };
    let v: f64 = num
        .trim()
        .parse()
        .map_err(|_| anyhow!("`{text}` is not a duration (examples: 500ms, 10s, 2m)"))?;
    if !v.is_finite() || v < 0.0 {
        bail!("`{text}` is not a duration");
    }
    Ok(Duration::from_secs_f64(v * scale))
}

/// A reading at the precision its display actually shows (`digits` can be negative: round to tens, hundreds,
/// ...), `OVER` on overload, with its unit symbol.
pub fn fmt_reading(r: &Reading) -> String {
    let number = if r.value.is_infinite() {
        let sign = if r.value.is_sign_negative() { "-" } else { "" };
        format!("{sign}OVER")
    } else if r.digits >= 0 {
        format!("{:.*}", r.digits as usize, r.value)
    } else {
        let scale = 10f64.powi(-r.digits);
        format!("{:.0}", (r.value / scale).round() * scale)
    };
    let unit = r.unit.symbol();
    if unit.is_empty() {
        number
    } else {
        format!("{number} {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Quantity, Unit};

    #[test]
    fn parses_durations() {
        assert_eq!(duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(duration("10s").unwrap(), Duration::from_secs(10));
        assert_eq!(duration("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(duration("3").unwrap(), Duration::from_secs(3));
        assert!(duration("-1s").is_err());
        assert!(duration("x").is_err());
    }

    fn reading(value: f64, digits: i32, unit: Unit) -> Reading {
        Reading {
            value,
            digits,
            quantity: Quantity::Voltage,
            unit,
            overload: false,
            ac: false,
            dc: true,
        }
    }

    #[test]
    fn formats_readings() {
        assert_eq!(fmt_reading(&reading(-0.0021, 4, Unit::Volt)), "-0.0021 V");
        assert_eq!(fmt_reading(&reading(24.0, 1, Unit::Celsius)), "24.0 degC");
        assert_eq!(
            fmt_reading(&reading(50_000_000.0, -3, Unit::Ohm)),
            "50000000 ohm"
        );
        assert_eq!(
            fmt_reading(&reading(f64::INFINITY, 1, Unit::Volt)),
            "OVER V"
        );
        assert_eq!(
            fmt_reading(&reading(f64::NEG_INFINITY, 1, Unit::Volt)),
            "-OVER V"
        );
    }
}
