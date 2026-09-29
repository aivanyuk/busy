//! Choices of what to show, made from a snapshot and the config. Pure logic, no Win32.

use busy_core::{Config, SensorKind, SensorReading, Snapshot};

/// The user's pinned sensor ("hardware/name"), else the hottest CPU temperature, else the hottest temperature.
pub(crate) fn pinned_sensor<'a>(snap: &'a Snapshot, cfg: &Config) -> Option<&'a SensorReading> {
    if !cfg.pinned_sensor.is_empty()
        && let Some(s) = snap
            .sensors
            .iter()
            .find(|s| cfg.pinned_sensor.split_once('/') == Some((s.hardware.as_str(), s.name.as_str())))
    {
        return Some(s);
    }
    let temps = || snap.sensors.iter().filter(|s| s.kind == SensorKind::Temperature);
    let is_cpu = |s: &&SensorReading| {
        let (h, n) = (s.hardware.to_lowercase(), s.name.to_lowercase());
        ["cpu", "ryzen", "intel", "core i", "processor"].iter().any(|k| h.contains(k))
            || ["cpu", "package", "tctl", "tdie"].iter().any(|k| n.contains(k))
    };
    let hottest = |a: &&SensorReading, b: &&SensorReading| a.value.total_cmp(&b.value);
    temps().filter(is_cpu).max_by(hottest).or_else(|| temps().max_by(hottest))
}

#[cfg(test)]
mod tests {
    use super::pinned_sensor;
    use busy_core::{Config, SensorKind, SensorReading, Snapshot};

    fn r(hardware: &str, name: &str, kind: SensorKind, value: f32) -> SensorReading {
        SensorReading { source: "test".into(), hardware: hardware.into(), name: name.into(), kind, value }
    }

    fn snap(sensors: Vec<SensorReading>) -> Snapshot {
        Snapshot { sensors, ..Snapshot::default() }
    }

    fn pick(s: &Snapshot, pinned: &str) -> Option<String> {
        let cfg = Config { pinned_sensor: pinned.into(), ..Config::default() };
        pinned_sensor(s, &cfg).map(|s| format!("{}/{}", s.hardware, s.name))
    }

    #[test]
    fn pinned_wins_even_if_not_a_temperature() {
        let s = snap(vec![
            r("Intel Core i9", "CPU Package", SensorKind::Temperature, 70.0),
            r("NVIDIA RTX", "GPU Fan", SensorKind::Fan, 1200.0),
        ]);
        assert_eq!(pick(&s, "NVIDIA RTX/GPU Fan").as_deref(), Some("NVIDIA RTX/GPU Fan"));
    }

    #[test]
    fn unknown_pin_falls_back_to_hottest_cpu_temperature() {
        let s = snap(vec![
            r("NVIDIA RTX", "GPU Core", SensorKind::Temperature, 85.0),
            r("AMD Ryzen 7", "Tctl", SensorKind::Temperature, 60.0),
            r("AMD Ryzen 7", "CCD1", SensorKind::Temperature, 65.0),
            r("AMD Ryzen 7", "Core Clock", SensorKind::Clock, 4500.0),
        ]);
        assert_eq!(pick(&s, "gone/sensor").as_deref(), Some("AMD Ryzen 7/CCD1"));
        assert_eq!(pick(&s, "").as_deref(), Some("AMD Ryzen 7/CCD1"));
    }

    #[test]
    fn without_cpu_readings_the_hottest_temperature_wins() {
        let s = snap(vec![
            r("NVIDIA RTX", "GPU Core", SensorKind::Temperature, 55.0),
            r("Samsung SSD", "Composite", SensorKind::Temperature, 48.0),
        ]);
        assert_eq!(pick(&s, "").as_deref(), Some("NVIDIA RTX/GPU Core"));
    }

    #[test]
    fn nothing_to_show() {
        assert_eq!(pick(&snap(vec![r("NVIDIA RTX", "GPU Fan", SensorKind::Fan, 900.0)]), ""), None);
        assert_eq!(pick(&Snapshot::default(), ""), None);
    }
}
