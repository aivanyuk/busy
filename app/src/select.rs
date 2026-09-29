//! Choices of what to show, made from a snapshot and the config. Pure logic, no Win32.

use busy_core::{Config, NetInterface, NetKind, SensorKind, SensorPick, SensorReading, Snapshot, VolumeInfo};

/// Most bars the CPU "Cores" bar style draws (design: 8).
const CORE_BARS: usize = 8;

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

/// The volume the Disk Text and Bar cells show (`options.disk.drive`, else the system drive). A configured drive
/// that is gone (unplugged) falls back to the system drive, whose letter the cell then shows.
pub(crate) fn disk_volume<'a>(snap: &'a Snapshot, cfg: &Config) -> Option<&'a VolumeInfo> {
    let chosen =
        cfg.options.disk.drive.as_deref().and_then(|d| snap.volumes.iter().find(|v| v.mount.eq_ignore_ascii_case(d)));
    chosen.or_else(|| snap.volumes.iter().find(|v| v.is_system)).or(snap.volumes.first())
}

/// (download, upload) bytes/s of the interfaces `options.network.interface` picks: the physical-interface
/// totals for `Auto`, else the sum over the matching interfaces (0 when none matches, e.g. Wi-Fi off).
pub(crate) fn net_rates(snap: &Snapshot, cfg: &Config) -> Option<(f64, f64)> {
    let n = snap.net.as_ref()?;
    let sum = |pick: &dyn Fn(&busy_core::NetIf) -> bool| {
        n.interfaces.iter().filter(|i| pick(i)).fold((0.0, 0.0), |(rx, tx), i| (rx + i.rx_bps, tx + i.tx_bps))
    };
    Some(match &cfg.options.network.interface {
        NetInterface::Auto => (n.rx_bps, n.tx_bps),
        NetInterface::WiFi => sum(&|i| i.kind == NetKind::Wifi),
        NetInterface::Ethernet => sum(&|i| i.kind == NetKind::Ethernet),
        NetInterface::Named(name) => sum(&|i| &i.name == name),
    })
}

/// The CPU "Cores" bars: at most `CORE_BARS`, each the mean of an equal run of logical processors (pairs on a
/// 16-thread CPU, as in the design; one bar per processor with 8 or fewer).
pub(crate) fn core_bars(per_core: &[f32]) -> Vec<f32> {
    if per_core.is_empty() {
        return Vec::new();
    }
    per_core.chunks(per_core.len().div_ceil(CORE_BARS)).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::{core_bars, disk_volume, net_rates, taskbar_sensor};
    use busy_core::{
        Config, GpuInfo, NetIf, NetInfo, NetInterface, NetKind, SensorKind, SensorPick, SensorReading, Snapshot,
        VolumeInfo,
    };

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

    #[test]
    fn disk_volume_follows_the_drive_option() {
        let vol = |mount: &str, is_system| VolumeInfo { mount: mount.into(), is_system, ..VolumeInfo::default() };
        let s = Snapshot { volumes: vec![vol("D:", false), vol("C:", true), vol("E:", false)], ..Snapshot::default() };
        let mut cfg = Config::default();
        let pick = |cfg: &Config| disk_volume(&s, cfg).map(|v| v.mount.clone());
        assert_eq!(pick(&cfg).as_deref(), Some("C:"));
        cfg.options.disk.drive = Some("e:".into());
        assert_eq!(pick(&cfg).as_deref(), Some("E:"));
        cfg.options.disk.drive = Some("Z:".into());
        assert_eq!(pick(&cfg).as_deref(), Some("C:"));
        assert!(disk_volume(&Snapshot::default(), &cfg).is_none());
    }

    #[test]
    fn net_rates_follow_the_interface_option() {
        let nif =
            |name: &str, kind, rx| NetIf { name: name.into(), kind, rx_bps: rx, tx_bps: rx / 10.0, ..NetIf::default() };
        let s = Snapshot {
            net: Some(NetInfo {
                rx_bps: 300.0,
                tx_bps: 30.0,
                interfaces: vec![
                    nif("Wi-Fi", NetKind::Wifi, 100.0),
                    nif("Ethernet", NetKind::Ethernet, 200.0),
                    nif("vEthernet", NetKind::Other, 50.0),
                ],
                ..NetInfo::default()
            }),
            ..Snapshot::default()
        };
        let mut cfg = Config::default();
        let mut rates = |i| {
            cfg.options.network.interface = i;
            net_rates(&s, &cfg)
        };
        assert_eq!(rates(NetInterface::Auto), Some((300.0, 30.0)));
        assert_eq!(rates(NetInterface::WiFi), Some((100.0, 10.0)));
        assert_eq!(rates(NetInterface::Ethernet), Some((200.0, 20.0)));
        assert_eq!(rates(NetInterface::Named("vEthernet".into())), Some((50.0, 5.0)));
        assert_eq!(rates(NetInterface::Named("gone".into())), Some((0.0, 0.0)));
        assert_eq!(net_rates(&Snapshot::default(), &Config::default()), None);
    }

    #[test]
    fn core_bars_average_equal_runs() {
        let cores: Vec<f32> = (0..16).map(|i| i as f32).collect();
        assert_eq!(core_bars(&cores), vec![0.5, 2.5, 4.5, 6.5, 8.5, 10.5, 12.5, 14.5]);
        assert_eq!(core_bars(&cores[..4]), vec![0.0, 1.0, 2.0, 3.0]);
        // 20 threads: runs of 3, the last one shorter.
        let twenty: Vec<f32> = (0..20).map(|_| 10.0).collect();
        assert_eq!(core_bars(&twenty).len(), 7);
        assert!(core_bars(&[]).is_empty());
    }
}
