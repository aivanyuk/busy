use crate::Module;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    /// Just left of the notification area (tray), growing leftwards.
    NearTray,
    /// Left edge of the taskbar, growing rightwards.
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellStyle {
    /// Two-line label/value text.
    Text,
    /// Mini sparkline with value overlay.
    Graph,
    /// Vertical bar(s).
    Bar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModuleCfg {
    pub module: Module,
    /// Shown as a cell on the taskbar.
    pub taskbar: bool,
    /// Shown as a section in the flyout.
    pub flyout: bool,
    pub style: CellStyle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub interval_ms: u32,
    /// Order = display order.
    pub modules: Vec<ModuleCfg>,
    pub anchor: Anchor,
    /// Extra horizontal offset from the anchor, in logical (96-dpi) pixels. Positive = away from anchor.
    pub offset_px: i32,
    pub theme: ThemeMode,
    pub autostart: bool,
    pub history_secs: u32,
    pub temp_unit: TempUnit,
    /// Sensor to show on the taskbar Sensors cell, matched as "hardware/name". Empty = hottest CPU temp.
    pub pinned_sensor: String,
}

impl Default for Config {
    fn default() -> Self {
        let m = |module, taskbar, style| ModuleCfg { module, taskbar, flyout: true, style };
        Self {
            interval_ms: 1000,
            modules: vec![
                m(Module::Cpu, true, CellStyle::Graph),
                m(Module::Memory, true, CellStyle::Text),
                m(Module::Gpu, true, CellStyle::Text),
                m(Module::Network, true, CellStyle::Text),
                m(Module::Disk, false, CellStyle::Text),
                m(Module::Sensors, false, CellStyle::Text),
                m(Module::Battery, false, CellStyle::Text),
                ModuleCfg { module: Module::Processes, taskbar: false, flyout: true, style: CellStyle::Text },
            ],
            anchor: Anchor::NearTray,
            offset_px: 0,
            theme: ThemeMode::System,
            autostart: false,
            history_secs: 120,
            temp_unit: TempUnit::Celsius,
            pinned_sensor: String::new(),
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("busy").join("config.json"))
    }

    /// Loads config; missing/invalid file yields defaults. Ensures every module appears exactly once.
    pub fn load() -> Self {
        let mut cfg: Config = Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        cfg.normalize();
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path().ok_or_else(|| std::io::Error::other("APPDATA not set"))?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, p)
    }

    pub fn normalize(&mut self) {
        let mut seen = Vec::new();
        self.modules.retain(|m| {
            let dup = seen.contains(&m.module);
            seen.push(m.module);
            !dup
        });
        let defaults = Config::default().modules;
        for d in defaults {
            if !self.modules.iter().any(|m| m.module == d.module) {
                self.modules.push(d);
            }
        }
        self.interval_ms = self.interval_ms.clamp(250, 10_000);
        self.history_secs = self.history_secs.clamp(10, 3600);
    }

    pub fn module(&self, m: Module) -> Option<&ModuleCfg> {
        self.modules.iter().find(|c| c.module == m)
    }

    /// Whether a module needs sampling at all.
    pub fn is_active(&self, m: Module) -> bool {
        self.module(m).is_some_and(|c| c.taskbar || c.flyout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_contains_every_module_once() {
        let cfg = Config::default();
        for m in Module::ALL {
            assert_eq!(cfg.modules.iter().filter(|c| c.module == m).count(), 1, "{m:?}");
        }
    }

    #[test]
    fn normalize_dedups_fills_and_clamps() {
        let mut cfg = Config { interval_ms: 1, history_secs: 1_000_000, ..Config::default() };
        let cpu = cfg.modules[0].clone();
        cfg.modules = vec![cpu.clone(), cpu];
        cfg.normalize();
        assert_eq!(cfg.modules.len(), Module::ALL.len());
        assert_eq!(cfg.modules[0].module, Module::Cpu);
        assert_eq!(cfg.interval_ms, 250);
        assert_eq!(cfg.history_secs, 3600);
    }

    #[test]
    fn partial_json_falls_back_to_defaults() {
        let cfg: Config = serde_json::from_str(r#"{"interval_ms": 2000}"#).unwrap();
        assert_eq!(cfg.interval_ms, 2000);
        assert_eq!(cfg.anchor, Anchor::NearTray);
    }

    #[test]
    fn json_roundtrip() {
        let cfg = Config { offset_px: -42, theme: ThemeMode::Dark, ..Config::default() };
        let back: Config = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn is_active_requires_taskbar_or_flyout() {
        let mut cfg = Config::default();
        for m in &mut cfg.modules {
            if m.module == Module::Disk {
                m.taskbar = false;
                m.flyout = false;
            }
        }
        assert!(!cfg.is_active(Module::Disk));
        assert!(cfg.is_active(Module::Cpu));
    }
}
