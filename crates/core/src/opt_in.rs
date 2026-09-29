//! Opt-in data sources: off by default, each with a risk stated in docs/plans/design-migration.md.

use serde::{Deserialize, Serialize};

/// Data sources that contact an external service, need administrator rights, run a slow API or read another
/// program's data. All off by default; a source that is off is not touched at all, not merely hidden.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OptIn {
    /// Public IP from an external lookup service (Network flyout).
    pub public_ip: bool,
    /// Per-process network usage through an ETW kernel session; needs administrator.
    pub process_network: bool,
    /// Per-app battery usage from the SRUM database; needs administrator.
    pub app_battery: bool,
    /// Memory speed from one slow WMI query.
    pub memory_speed: bool,
    /// LibreHardwareMonitor (WMI) and HWiNFO (shared memory) readings: CPU temps, fans, power.
    pub third_party_sensors: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    fn load_str(json: &str) -> Config {
        let mut cfg: Config = serde_json::from_str(json).unwrap();
        cfg.normalize();
        cfg
    }

    #[test]
    fn default_off() {
        let o = OptIn::default();
        assert!(!(o.public_ip || o.process_network || o.app_battery || o.memory_speed || o.third_party_sensors));
        assert_eq!(Config::default().opt_in, o);
        assert_eq!(load_str(r#"{"interval_ms": 2000}"#).opt_in, o);
    }

    #[test]
    fn partial_json() {
        let cfg = load_str(r#"{"version": 2, "opt_in": {"third_party_sensors": true}}"#);
        assert_eq!(cfg.opt_in, OptIn { third_party_sensors: true, ..OptIn::default() });
    }

    #[test]
    fn roundtrip() {
        let o = OptIn {
            public_ip: true,
            process_network: false,
            app_battery: true,
            memory_speed: false,
            third_party_sensors: true,
        };
        assert_eq!(serde_json::from_str::<OptIn>(&serde_json::to_string(&o).unwrap()).unwrap(), o);
    }
}
