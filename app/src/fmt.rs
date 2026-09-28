//! Human-readable formatting of metric values.

use busy_core::{SensorKind, TempUnit};

const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

/// Scales `v` by 1024 until it is below 1000; returns (value, unit index).
fn scale(mut v: f64, mut i: usize) -> (f64, usize) {
    while v >= 999.95 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    (v, i)
}

fn num(v: f64) -> String {
    if v >= 99.95 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// `1536` -> `"1.5 KB"`, `0` -> `"0 B"`.
pub fn bytes(b: u64) -> String {
    if b < 1000 {
        return format!("{b} B");
    }
    let (v, i) = scale(b as f64, 0);
    format!("{} {}", num(v), UNITS[i])
}

/// Transfer rate, never below KB/s to keep the width stable: `"0 KB/s"`, `"350 KB/s"`, `"1.2 MB/s"`.
pub fn rate(bps: f64) -> String {
    let kb = bps.max(0.0) / 1024.0;
    if kb < 0.05 {
        return "0 KB/s".into();
    }
    let (v, i) = scale(kb, 1);
    format!("{} {}/s", num(v), UNITS[i])
}

/// Compact rate for narrow places: `"1.2M"`, `"350K"`.
pub fn rate_short(bps: f64) -> String {
    let (v, i) = scale(bps.max(0.0) / 1024.0, 1);
    format!("{}{}", if v < 0.05 { "0".into() } else { num(v) }, &UNITS[i][..1])
}

pub fn pct(v: f32) -> String {
    format!("{:.0}%", v.clamp(0.0, 100.0))
}

/// `59` -> `"59s"`, `3720` -> `"1h 2m"`, `90000` -> `"1d 1h"`.
pub fn duration(secs: u64) -> String {
    let (d, h, m, s) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    match () {
        _ if d > 0 => format!("{d}d {h}h"),
        _ if h > 0 => format!("{h}h {m}m"),
        _ if m > 0 => format!("{m}m"),
        _ => format!("{s}s"),
    }
}

pub fn temp(c: f32, unit: TempUnit) -> String {
    match unit {
        TempUnit::Celsius => format!("{c:.0}°C"),
        TempUnit::Fahrenheit => format!("{:.0}°F", c * 9.0 / 5.0 + 32.0),
    }
}

pub fn mhz(m: u32) -> String {
    if m >= 1000 { format!("{:.2} GHz", m as f32 / 1000.0) } else { format!("{m} MHz") }
}

/// Thousands separators: `4512` -> `"4,512"`.
pub fn count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn watts(w: f32) -> String {
    if w.abs() >= 99.95 { format!("{w:.0} W") } else { format!("{w:.1} W") }
}

pub fn sensor(value: f32, kind: SensorKind, unit: TempUnit) -> String {
    match kind {
        SensorKind::Temperature => temp(value, unit),
        SensorKind::Fan => format!("{value:.0} rpm"),
        SensorKind::Power => watts(value),
        SensorKind::Voltage => format!("{value:.3} V"),
        SensorKind::Clock => mhz(value.max(0.0) as u32),
        SensorKind::Load => pct(value),
        SensorKind::Other => format!("{value:.1}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_sizes() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(999), "999 B");
        assert_eq!(bytes(1536), "1.5 KB");
        assert_eq!(bytes(16 * 1024 * 1024 * 1024), "16.0 GB");
        assert_eq!(bytes(512 * 1024 * 1024), "512 MB");
        assert_eq!(bytes(1000 * 1024), "1.0 MB");
    }

    #[test]
    fn rates() {
        assert_eq!(rate(0.0), "0 KB/s");
        assert_eq!(rate(-5.0), "0 KB/s");
        assert_eq!(rate(350.0 * 1024.0), "350 KB/s");
        assert_eq!(rate(1.25 * 1024.0 * 1024.0), "1.2 MB/s");
        assert_eq!(rate(12.0 * 1024.0), "12.0 KB/s");
        assert_eq!(rate_short(1.5 * 1024.0 * 1024.0), "1.5M");
        assert_eq!(rate_short(0.0), "0K");
    }

    #[test]
    fn misc() {
        assert_eq!(pct(23.4), "23%");
        assert_eq!(pct(140.0), "100%");
        assert_eq!(duration(59), "59s");
        assert_eq!(duration(3720), "1h 2m");
        assert_eq!(duration(90000), "1d 1h");
        assert_eq!(duration(600), "10m");
        assert_eq!(temp(100.0, TempUnit::Celsius), "100°C");
        assert_eq!(temp(100.0, TempUnit::Fahrenheit), "212°F");
        assert_eq!(mhz(3600), "3.60 GHz");
        assert_eq!(mhz(800), "800 MHz");
        assert_eq!(count(4512), "4,512");
        assert_eq!(count(1234567), "1,234,567");
        assert_eq!(count(12), "12");
        assert_eq!(watts(12.34), "12.3 W");
    }
}
