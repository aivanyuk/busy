//! Human-readable formatting of metric values. Numbers take the user's decimal and thousands separators
//! (`set_separators`); unit abbreviations are the same in every language.

use crate::i18n::{fill, t};
use busy_core::{RateUnit, SensorKind, TempUnit};
use std::sync::OnceLock;

/// The user's (decimal, thousands) separators, from `set_separators`; "." and "," until then, as in tests.
static SEPARATORS: OnceLock<(String, String)> = OnceLock::new();

/// Sets the separators numbers are written with, from the user's regional format
/// (`busy_win::number_separators`), once at startup; later calls are ignored. A separator that is empty or
/// longer than three characters keeps the default.
pub fn set_separators(decimal: Option<&str>, group: Option<&str>) {
    let ok = |s: Option<&str>, default: &str| {
        s.filter(|s| !s.is_empty() && s.chars().count() <= 3).unwrap_or(default).to_owned()
    };
    let _ = SEPARATORS.set((ok(decimal, "."), ok(group, ",")));
}

fn separators() -> (&'static str, &'static str) {
    SEPARATORS.get().map_or((".", ","), |(d, g)| (d.as_str(), g.as_str()))
}

/// A number Rust formatted (`"2.5"`) with the user's decimal separator (`"2,5"`).
fn local(s: String) -> String {
    with_decimal(s, separators().0)
}

fn with_decimal(s: String, decimal: &str) -> String {
    if decimal == "." { s } else { s.replacen('.', decimal, 1) }
}

/// `v` with `places` decimals and the user's decimal separator: `decimal(2.45, 1)` -> `"2.5"` (or `"2,5"`).
pub fn decimal(v: f64, places: usize) -> String {
    local(format!("{v:.places$}"))
}

const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
const BIT_UNITS: [&str; 6] = ["b", "Kb", "Mb", "Gb", "Tb", "Pb"];

/// Scales `v` by 1024 until it is below 1000; returns (value, unit index).
fn scale(mut v: f64, mut i: usize) -> (f64, usize) {
    while v >= 999.95 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    (v, i)
}

fn num(v: f64) -> String {
    if v >= 99.95 { format!("{v:.0}") } else { decimal(v, 1) }
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
    rate_in(bps, RateUnit::Bytes)
}

/// `rate` in bytes or bits per second (`"2.4 Mb/s"`), scaled by 1024 either way as in the design.
pub fn rate_in(bps: f64, unit: RateUnit) -> String {
    let (v, units) = match unit {
        RateUnit::Bytes => (bps, &UNITS),
        RateUnit::Bits => (bps * 8.0, &BIT_UNITS),
    };
    let k = v.max(0.0) / 1024.0;
    if k < 0.05 {
        return format!("0 {}/s", units[1]);
    }
    let (v, i) = scale(k, 1);
    format!("{} {}/s", num(v), units[i])
}

/// `rate_in` without the unit's B/b and "/s", for a vertical taskbar's narrow cells (design `rateS`):
/// `"0K"`, `"350K"`, `"1.2M"`, `"15M"`, `"1.1G"`.
pub fn rate_short(bps: f64, unit: RateUnit) -> String {
    let v = match unit {
        RateUnit::Bytes => bps,
        RateUnit::Bits => bps * 8.0,
    };
    let k = v.max(0.0) / 1024.0;
    if k < 999.5 {
        return format!("{k:.0}K");
    }
    let (v, suffix) = if k / 1024.0 < 999.5 { (k / 1024.0, 'M') } else { (k / 1024.0 / 1024.0, 'G') };
    if v < 9.95 { format!("{}{suffix}", decimal(v, 1)) } else { format!("{v:.0}{suffix}") }
}

/// Link speed of a network adapter: `"2.5 Gbps"`, `"866 Mbps"`.
pub fn link_speed(bps: u64) -> String {
    if bps >= 1_000_000_000 {
        format!("{} Gbps", local((bps as f64 / 1e9).to_string()))
    } else {
        format!("{} Mbps", bps / 1_000_000)
    }
}

/// Wi-Fi band of a channel's center frequency: `"2.4 GHz"`, `"5 GHz"`, `"6 GHz"`.
pub fn wifi_band(mhz: u32) -> String {
    match mhz {
        ..3000 => format!("{} GHz", decimal(2.4, 1)),
        3000..5925 => "5 GHz".into(),
        _ => "6 GHz".into(),
    }
}

/// Time left in words (design `remaining(false)`): `4500` -> `"1 h 15 min"`, `600` -> `"10 min"`.
pub fn hours_minutes_long(secs: u32) -> String {
    let (m, t) = (secs.div_ceil(60), &t().time);
    if m < 60 { fill(t.minutes_long, &[&m]) } else { fill(t.hours_minutes_long, &[&(m / 60), &(m % 60)]) }
}

/// Time left as `h:mm` (design `remaining(short)`): `4500` -> `"1:15"`.
pub fn hours_minutes(secs: u32) -> String {
    let m = secs.div_ceil(60);
    format!("{}:{:02}", m / 60, m % 60)
}

pub fn pct(v: f32) -> String {
    format!("{:.0}%", v.clamp(0.0, 100.0))
}

/// One decimal, for per-process shares: `"12.3%"`.
pub fn pct1(v: f32) -> String {
    format!("{}%", decimal(f64::from(v.clamp(0.0, 100.0)), 1))
}

/// Task Manager's up time: `"3:04:05:06"` (days:hours:minutes:seconds).
pub fn uptime(secs: u64) -> String {
    format!("{}:{:02}:{:02}:{:02}", secs / 86400, secs / 3600 % 24, secs / 60 % 60, secs % 60)
}

/// `59` -> `"59s"`, `3720` -> `"1h 2m"`, `90000` -> `"1d 1h"`.
pub fn duration(secs: u64) -> String {
    let (d, h, m, s) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    let t = &t().time;
    match () {
        _ if d > 0 => fill(t.days_hours, &[&d, &h]),
        _ if h > 0 => fill(t.hours_minutes, &[&h, &m]),
        _ if m > 0 => fill(t.minutes, &[&m]),
        _ => fill(t.seconds, &[&s]),
    }
}

pub fn temp(c: f32, unit: TempUnit) -> String {
    match unit {
        TempUnit::Celsius => format!("{c:.0}°C"),
        TempUnit::Fahrenheit => format!("{:.0}°F", c * 9.0 / 5.0 + 32.0),
    }
}

pub fn mhz(m: u32) -> String {
    if m >= 1000 { format!("{} GHz", decimal(f64::from(m) / 1000.0, 2)) } else { format!("{m} MHz") }
}

/// Thousands separators: `4512` -> `"4,512"` (or `"4.512"`, `"4 512"`).
pub fn count(n: u64) -> String {
    grouped(n, separators().1)
}

fn grouped(n: u64, group: &str) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3 * group.len());
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push_str(group);
        }
        out.push(c);
    }
    out
}

pub fn watts(w: f32) -> String {
    if w.abs() >= 99.95 { format!("{w:.0} W") } else { format!("{} W", decimal(f64::from(w), 1)) }
}

pub fn sensor(value: f32, kind: SensorKind, unit: TempUnit) -> String {
    match kind {
        SensorKind::Temperature => temp(value, unit),
        SensorKind::Fan => format!("{value:.0} rpm"),
        SensorKind::Power => watts(value),
        SensorKind::Voltage => format!("{} V", decimal(f64::from(value), 3)),
        SensorKind::Clock => mhz(value.max(0.0) as u32),
        SensorKind::Load => pct(value),
        SensorKind::Other => decimal(f64::from(value), 1),
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
        assert_eq!(rate_in(0.0, RateUnit::Bits), "0 Kb/s");
        assert_eq!(rate_in(350.0 * 1024.0, RateUnit::Bits), "2.7 Mb/s");
        assert_eq!(rate_in(64.0, RateUnit::Bits), "0.5 Kb/s");
        let short = |kib: f64| rate_short(kib * 1024.0, RateUnit::Bytes);
        assert_eq!([0.0, 350.0, 999.0, 1229.0, 15_360.0].map(short), ["0K", "350K", "999K", "1.2M", "15M"]);
        assert_eq!(short(1.1 * 1024.0 * 1024.0), "1.1G");
        assert_eq!(rate_short(-5.0, RateUnit::Bytes), "0K");
        assert_eq!(rate_short(350.0 * 1024.0, RateUnit::Bits), "2.7M");
        assert_eq!(hours_minutes(4500), "1:15");
        assert_eq!((hours_minutes_long(4500), hours_minutes_long(600)), ("1 h 15 min".into(), "10 min".into()));
        assert_eq!((link_speed(2_500_000_000), link_speed(866_000_000)), ("2.5 Gbps".into(), "866 Mbps".into()));
        assert_eq!([2437, 5180, 6115].map(wifi_band), ["2.4 GHz", "5 GHz", "6 GHz"]);
        assert_eq!(hours_minutes(59), "0:01");
        assert_eq!(hours_minutes(36_000), "10:00");
    }

    #[test]
    fn misc() {
        assert_eq!(pct(23.4), "23%");
        assert_eq!(pct(140.0), "100%");
        assert_eq!(duration(59), "59s");
        assert_eq!(duration(3720), "1h 2m");
        assert_eq!(duration(90000), "1d 1h");
        assert_eq!(duration(600), "10m");
        assert_eq!(pct1(12.345), "12.3%");
        assert_eq!(uptime(3 * 86400 + 4 * 3600 + 5 * 60 + 6), "3:04:05:06");
        assert_eq!(temp(100.0, TempUnit::Celsius), "100°C");
        assert_eq!(temp(100.0, TempUnit::Fahrenheit), "212°F");
        assert_eq!(mhz(3600), "3.60 GHz");
        assert_eq!(mhz(800), "800 MHz");
        assert_eq!(count(4512), "4,512");
        assert_eq!(count(1234567), "1,234,567");
        assert_eq!(count(12), "12");
        assert_eq!(watts(12.34), "12.3 W");
    }

    #[test]
    fn separators() {
        assert_eq!(with_decimal("2.5".into(), ","), "2,5");
        assert_eq!(with_decimal("2.5".into(), "."), "2.5");
        assert_eq!(with_decimal("12".into(), ","), "12");
        assert_eq!(grouped(1_234_567, "."), "1.234.567");
        assert_eq!(grouped(1_234_567, "\u{202f}"), "1\u{202f}234\u{202f}567");
        assert_eq!(grouped(999, " "), "999");
    }
}
