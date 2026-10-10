//! Durations (`500ms`, `10s`).

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations() {
        assert_eq!(duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(duration("10s").unwrap(), Duration::from_secs(10));
        assert_eq!(duration("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(duration("3").unwrap(), Duration::from_secs(3));
        assert!(duration("-1s").is_err());
        assert!(duration("x").is_err());
    }
}
