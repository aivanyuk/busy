//! Settings that exist for one module only (design `rows()`).

use serde::{Deserialize, Serialize};

/// One struct per module, so a value can't end up on the wrong module and survives reordering or
/// deduplication of `Config::modules`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModuleOptions {
    pub cpu: CpuOptions,
    pub disk: DiskOptions,
    pub network: NetOptions,
    pub battery: BatteryOptions,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CpuOptions {
    /// What the Bar style shows.
    pub bar: CpuBar,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuBar {
    /// One thin bar per pair of logical processors.
    #[default]
    Cores,
    Total,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiskOptions {
    /// Volume for the Text and Bar styles, as `VolumeInfo::mount` (e.g. `"D:"`). `None` = the system drive.
    pub drive: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetOptions {
    pub units: RateUnit,
    pub interface: NetInterface,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateUnit {
    /// KB/s, MB/s.
    #[default]
    Bytes,
    /// Kb/s, Mb/s.
    Bits,
}

/// Which adapter(s) the Network module measures. JSON: `"Auto"`, `"WiFi"`, `"Ethernet"` or
/// `{"Named": "<adapter alias>"}` with the alias as shown in `NetIf::name`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetInterface {
    /// Sum of the physical, connected interfaces (the `NetInfo` totals).
    #[default]
    Auto,
    WiFi,
    Ethernet,
    Named(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BatteryOptions {
    /// Show the time left (h:mm) in place of the label.
    pub show_remaining: bool,
}

impl Default for BatteryOptions {
    fn default() -> Self {
        Self { show_remaining: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let o = ModuleOptions::default();
        assert_eq!(o.cpu.bar, CpuBar::Cores);
        assert_eq!(o.disk.drive, None);
        assert_eq!((o.network.units, &o.network.interface), (RateUnit::Bytes, &NetInterface::Auto));
        assert!(o.battery.show_remaining);
    }

    #[test]
    fn json_shape() {
        let o: ModuleOptions =
            serde_json::from_str(r#"{"network": {"interface": {"Named": "Wi-Fi 2"}, "units": "Bits"}, "battery": {}}"#)
                .unwrap();
        assert_eq!(o.network.interface, NetInterface::Named("Wi-Fi 2".into()));
        assert_eq!(o.network.units, RateUnit::Bits);
        assert!(o.battery.show_remaining);
        assert_eq!(o.cpu, CpuOptions::default());
    }

    #[test]
    fn roundtrip() {
        let o = ModuleOptions {
            cpu: CpuOptions { bar: CpuBar::Total },
            disk: DiskOptions { drive: Some("D:".into()) },
            network: NetOptions { units: RateUnit::Bits, interface: NetInterface::Ethernet },
            battery: BatteryOptions { show_remaining: false },
        };
        let back: ModuleOptions = serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert_eq!(o, back);
    }
}
