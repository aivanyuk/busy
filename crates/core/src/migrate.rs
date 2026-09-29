//! Schema migrations, run by `Config::normalize` on every loaded or submitted config.

use crate::{CellStyle, Config, Module};

/// Schema version written by this build. v1 = before the design migration (no `version` field).
pub const CONFIG_VERSION: u32 = 2;

impl Config {
    /// Brings a config written by any older schema up to `CONFIG_VERSION`.
    pub(crate) fn migrate(&mut self) {
        if self.version < 2 {
            self.migrate_v1();
        }
        self.version = CONFIG_VERSION;
    }

    /// v1 → v2. Anyone with a v1 file already uses busy, so first-run setup is skipped. v1's Disk "Text"
    /// cell showed read/write rates, which is the design's `Io` style (its Disk `Text` is the drive's fill).
    fn migrate_v1(&mut self) {
        self.onboarded = true;
        for m in self.modules.iter_mut().filter(|m| m.module == Module::Disk && m.style == CellStyle::Text) {
            m.style = CellStyle::Io;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, TempUnit, ThemeMode};

    /// A complete file as written by the v1 schema (before `version` existed).
    const V1_JSON: &str = r#"{
        "interval_ms": 2000,
        "modules": [
            {"module": "Disk", "taskbar": true, "flyout": true, "style": "Text"},
            {"module": "Cpu", "taskbar": true, "flyout": true, "style": "Graph"},
            {"module": "Memory", "taskbar": true, "flyout": false, "style": "Bar"},
            {"module": "Gpu", "taskbar": false, "flyout": true, "style": "Text"},
            {"module": "Network", "taskbar": true, "flyout": true, "style": "Text"},
            {"module": "Sensors", "taskbar": true, "flyout": true, "style": "Text"},
            {"module": "Battery", "taskbar": false, "flyout": false, "style": "Text"},
            {"module": "Processes", "taskbar": false, "flyout": true, "style": "Text"}
        ],
        "anchor": "Left",
        "offset_px": 12,
        "theme": "Light",
        "autostart": true,
        "history_secs": 300,
        "temp_unit": "Fahrenheit",
        "pinned_sensor": "CPU/Package"
    }"#;

    fn load_str(json: &str) -> Config {
        let mut cfg: Config = serde_json::from_str(json).unwrap();
        cfg.normalize();
        cfg
    }

    #[test]
    fn fresh_install_is_not_onboarded() {
        let cfg = Config::default();
        assert_eq!((cfg.version, cfg.onboarded), (CONFIG_VERSION, false));
    }

    #[test]
    fn default_is_normalized() {
        let mut cfg = Config::default();
        cfg.normalize();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn v1_file_migrates() {
        let cfg = load_str(V1_JSON);
        assert_eq!((cfg.version, cfg.onboarded), (CONFIG_VERSION, true));
        assert_eq!(cfg.interval_ms, 2000);
        assert_eq!((cfg.anchor, cfg.offset_px, cfg.theme), (Anchor::Left, 12, ThemeMode::Light));
        assert_eq!((cfg.autostart, cfg.history_secs, cfg.temp_unit), (true, 300, TempUnit::Fahrenheit));
        let order: Vec<_> = cfg.modules.iter().map(|m| m.module).collect();
        assert_eq!(order[..3], [Module::Disk, Module::Cpu, Module::Memory]);
        let m = |m| cfg.module(m).unwrap();
        assert_eq!(m(Module::Disk).style, CellStyle::Io);
        assert_eq!(m(Module::Network).style, CellStyle::Io);
        assert_eq!(m(Module::Memory).style, CellStyle::Bar);
        assert!(!m(Module::Memory).flyout && !m(Module::Battery).taskbar);
        assert!(m(Module::Cpu).show_label && m(Module::Cpu).color.is_none());
    }

    #[test]
    fn v2_file_keeps_onboarding_state_and_disk_text() {
        let mut cfg = Config::default();
        for m in &mut cfg.modules {
            if m.module == Module::Disk {
                m.style = CellStyle::Text;
            }
        }
        let back = load_str(&serde_json::to_string(&cfg).unwrap());
        assert!(!back.onboarded);
        assert_eq!(back.module(Module::Disk).map(|m| m.style), Some(CellStyle::Text));
        cfg.onboarded = true;
        assert!(load_str(&serde_json::to_string(&cfg).unwrap()).onboarded);
    }
}
