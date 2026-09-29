//! Choices of what to show, made from a snapshot and the config. Pure logic, no Win32.

use busy_core::{Config, SensorKind, SensorPick, SensorReading, Snapshot};

/// The reading picked for the taskbar Sensors cell (`options.sensors.sensor`). `Named` and `Cpu` fall back to
/// the hottest temperature so the cell isn't empty; `Gpu` and `Storage` show nothing rather than another part.
pub(crate) fn taskbar_sensor<'a>(snap: &'a Snapshot, cfg: &Config) -> Option<&'a SensorReading> {
    let temps = || snap.sensors.iter().filter(|s| s.kind == SensorKind::Temperature);
    let hottest = |a: &&SensorReading, b: &&SensorReading| a.value.total_cmp(&b.value);
    let any = |keys: &[&str], s: &str| {
        let s = s.to_lowercase();
        keys.iter().any(|k| s.contains(k))
    };
    let is_cpu = |s: &&SensorReading| {
        any(&["cpu", "ryzen", "intel", "core i", "processor"], &s.hardware)
            || any(&["cpu", "package", "tctl", "tdie"], &s.name)
    };
    let is_gpu = |s: &&SensorReading| {
        let hw = s.hardware.to_lowercase();
        snap.gpus.iter().any(|g| !g.name.is_empty() && hw.contains(&g.name.to_lowercase()))
            || any(&["gpu", "geforce", "radeon"], &hw)
    };
    let is_storage = |s: &&SensorReading| any(&["nvme", "ssd", "hdd", "disk", "drive"], &s.hardware);
    let cpu = || temps().filter(is_cpu).max_by(hottest).or_else(|| temps().max_by(hottest));
    match &cfg.options.sensors.sensor {
        SensorPick::Cpu => cpu(),
        SensorPick::Gpu => temps().filter(is_gpu).max_by(hottest),
        SensorPick::Storage => temps().filter(is_storage).max_by(hottest),
        SensorPick::Named(key) => snap
            .sensors
            .iter()
            .find(|s| key.split_once('/') == Some((s.hardware.as_str(), s.name.as_str())))
            .or_else(cpu),
    }
}

#[cfg(test)]
mod tests {
    use super::taskbar_sensor;
    use busy_core::{Config, GpuInfo, SensorKind, SensorPick, SensorReading, Snapshot};

    fn r(hardware: &str, name: &str, kind: SensorKind, value: f32) -> SensorReading {
        SensorReading { source: "test".into(), hardware: hardware.into(), name: name.into(), kind, value }
    }

    fn temp(hardware: &str, name: &str, value: f32) -> SensorReading {
        r(hardware, name, SensorKind::Temperature, value)
    }

    fn snap(sensors: Vec<SensorReading>) -> Snapshot {
        Snapshot { sensors, ..Snapshot::default() }
    }

    fn pick(s: &Snapshot, sensor: SensorPick) -> Option<String> {
        let mut cfg = Config::default();
        cfg.options.sensors.sensor = sensor;
        taskbar_sensor(s, &cfg).map(|s| format!("{}/{}", s.hardware, s.name))
    }

    fn named(key: &str) -> SensorPick {
        SensorPick::Named(key.into())
    }

    #[test]
    fn named_wins_even_if_not_a_temperature() {
        let s =
            snap(vec![temp("Intel Core i9", "CPU Package", 70.0), r("NVIDIA RTX", "GPU Fan", SensorKind::Fan, 1200.0)]);
        assert_eq!(pick(&s, named("NVIDIA RTX/GPU Fan")).as_deref(), Some("NVIDIA RTX/GPU Fan"));
    }

    #[test]
    fn unknown_name_falls_back_to_hottest_cpu_temperature() {
        let s = snap(vec![
            temp("NVIDIA RTX", "GPU Core", 85.0),
            temp("AMD Ryzen 7", "Tctl", 60.0),
            temp("AMD Ryzen 7", "CCD1", 65.0),
            r("AMD Ryzen 7", "Core Clock", SensorKind::Clock, 4500.0),
        ]);
        assert_eq!(pick(&s, named("gone/sensor")).as_deref(), Some("AMD Ryzen 7/CCD1"));
        assert_eq!(pick(&s, SensorPick::Cpu).as_deref(), Some("AMD Ryzen 7/CCD1"));
    }

    #[test]
    fn without_cpu_readings_the_hottest_temperature_wins() {
        let s = snap(vec![temp("NVIDIA RTX", "GPU Core", 55.0), temp("Samsung SSD", "Composite", 48.0)]);
        assert_eq!(pick(&s, SensorPick::Cpu).as_deref(), Some("NVIDIA RTX/GPU Core"));
    }

    #[test]
    fn follows_the_pick() {
        let s = Snapshot {
            gpus: vec![GpuInfo { name: "NVIDIA RTX 3080".into(), ..GpuInfo::default() }],
            ..snap(vec![
                temp("AMD Ryzen 7 7840U", "Tctl", 61.0),
                temp("NVIDIA RTX 3080", "GPU Temperature", 70.0),
                temp("Samsung SSD 980 PRO", "Temperature", 45.0),
                temp("ACPI", "Thermal zone", 80.0),
            ])
        };
        assert_eq!(pick(&s, SensorPick::Cpu).as_deref(), Some("AMD Ryzen 7 7840U/Tctl"));
        assert_eq!(pick(&s, SensorPick::Gpu).as_deref(), Some("NVIDIA RTX 3080/GPU Temperature"));
        assert_eq!(pick(&s, SensorPick::Storage).as_deref(), Some("Samsung SSD 980 PRO/Temperature"));
        assert_eq!(pick(&s, named("ACPI/Thermal zone")).as_deref(), Some("ACPI/Thermal zone"));
    }

    #[test]
    fn a_missing_part_is_not_substituted() {
        let s = snap(vec![temp("ACPI", "Thermal zone", 50.0)]);
        assert_eq!(pick(&s, SensorPick::Cpu).as_deref(), Some("ACPI/Thermal zone"));
        assert_eq!(pick(&s, SensorPick::Gpu), None);
        assert_eq!(pick(&s, SensorPick::Storage), None);
    }

    #[test]
    fn nothing_to_show() {
        assert_eq!(pick(&snap(vec![r("NVIDIA RTX", "GPU Fan", SensorKind::Fan, 900.0)]), SensorPick::Cpu), None);
        assert_eq!(pick(&Snapshot::default(), SensorPick::Cpu), None);
    }
}
