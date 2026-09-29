use crate::{CONFIG_VERSION, Module, ModuleOptions, OptIn};
use serde::{Deserialize, Deserializer, Serialize};
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
    /// Two stacked rates with colored keys: read/write, upload/download.
    Io,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

/// Number of colors in the module palette (design `PAL`).
pub const PALETTE_LEN: u8 = 6;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModuleCfg {
    pub module: Module,
    /// Shown as a cell on the taskbar.
    #[serde(default)]
    pub taskbar: bool,
    /// Processes: show top-process lists in the CPU, Memory and Disk flyouts. Other modules ignore it: a cell
    /// always opens its module's flyout (design); it stays in the file for the settings window and older builds.
    #[serde(default = "yes")]
    pub flyout: bool,
    #[serde(default = "text")]
    pub style: CellStyle,
    /// Small caption above the value on the taskbar cell.
    #[serde(default = "yes")]
    pub show_label: bool,
    /// Palette index `0..PALETTE_LEN`; `None` = `Module::default_color()`.
    #[serde(default)]
    pub color: Option<u8>,
    /// Shift green → amber → red as the value rises, instead of the module color.
    #[serde(default)]
    pub color_by_load: bool,
    /// Update interval in seconds (1..=10); `None` = the general `Config::interval_ms`.
    #[serde(default)]
    pub interval_s: Option<u32>,
}

impl ModuleCfg {
    pub fn new(module: Module, taskbar: bool, style: CellStyle) -> Self {
        Self {
            module,
            taskbar,
            flyout: true,
            style,
            show_label: true,
            color: None,
            color_by_load: false,
            interval_s: None,
        }
    }

    /// Palette index in effect.
    pub fn color_index(&self) -> u8 {
        self.color.unwrap_or(self.module.default_color())
    }
}

fn yes() -> bool {
    true
}

fn text() -> CellStyle {
    CellStyle::Text
}

fn v1() -> u32 {
    1
}

/// Drops entries that don't parse (a module or style from a newer build, a hand-edit) instead of failing the
/// whole file; `normalize()` then re-adds any module that went missing.
fn valid_modules<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<ModuleCfg>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(d)?;
    Ok(raw.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Schema version the file was written with; a file without it is v1. `normalize()` migrates it.
    #[serde(default = "v1")]
    pub version: u32,
    /// First-run setup was completed or skipped. False on a fresh install; v1 files migrate to true.
    pub onboarded: bool,
    pub interval_ms: u32,
    /// Order = display order.
    #[serde(deserialize_with = "valid_modules")]
    pub modules: Vec<ModuleCfg>,
    pub anchor: Anchor,
    /// Extra horizontal offset from the anchor, in logical (96-dpi) pixels. Positive = away from anchor.
    pub offset_px: i32,
    pub theme: ThemeMode,
    pub autostart: bool,
    pub history_secs: u32,
    pub temp_unit: TempUnit,
    pub options: ModuleOptions,
    pub opt_in: OptIn,
    /// v1's `pinned_sensor` ("hardware/name", empty = hottest CPU). Only read: `normalize()` moves it to
    /// `options.sensors.sensor` and it is never written back.
    #[serde(rename = "pinned_sensor", skip_serializing)]
    pub legacy_pinned_sensor: String,
}

impl Default for Config {
    fn default() -> Self {
        let m = ModuleCfg::new;
        Self {
            version: CONFIG_VERSION,
            onboarded: false,
            interval_ms: 1000,
            modules: vec![
                m(Module::Cpu, true, CellStyle::Graph),
                m(Module::Memory, true, CellStyle::Text),
                m(Module::Gpu, true, CellStyle::Text),
                m(Module::Network, true, CellStyle::Io),
                m(Module::Disk, false, CellStyle::Text),
                m(Module::Sensors, false, CellStyle::Text),
                m(Module::Battery, false, CellStyle::Text),
                m(Module::Processes, false, CellStyle::Text),
            ],
            anchor: Anchor::NearTray,
            offset_px: 0,
            theme: ThemeMode::System,
            autostart: false,
            history_secs: 120,
            temp_unit: TempUnit::Celsius,
            options: ModuleOptions::default(),
            opt_in: OptIn::default(),
            legacy_pinned_sensor: String::new(),
        }
    }
}

impl Module {
    /// Taskbar styles this module can be drawn in, preferred first (design `MODS.styles`).
    /// Empty = flyout-only.
    pub fn allowed_styles(self) -> &'static [CellStyle] {
        use CellStyle::*;
        match self {
            Module::Cpu | Module::Gpu => &[Graph, Text, Bar],
            Module::Memory => &[Bar, Text, Graph],
            Module::Disk => &[Io, Text, Bar],
            Module::Network => &[Io, Graph],
            Module::Battery => &[Text, Bar],
            Module::Sensors => &[Text, Graph],
            Module::Processes => &[],
        }
    }

    /// Palette index used when `ModuleCfg::color` is `None` (design `MODS.color`).
    pub fn default_color(self) -> u8 {
        match self {
            Module::Cpu | Module::Network | Module::Processes => 0,
            Module::Gpu => 1,
            Module::Memory | Module::Battery => 2,
            Module::Disk => 3,
            Module::Sensors => 4,
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
        self.migrate();
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
        for m in &mut self.modules {
            m.color = m.color.filter(|&c| c < PALETTE_LEN);
            m.interval_s = m.interval_s.map(|s| s.clamp(1, 10));
            let allowed = m.module.allowed_styles();
            match allowed.first() {
                None => m.taskbar = false,
                Some(&first) if !allowed.contains(&m.style) => m.style = first,
                _ => {}
            }
        }
        self.interval_ms = self.interval_ms.clamp(250, 10_000);
        self.history_secs = self.history_secs.clamp(10, 3600);
    }

    pub fn module(&self, m: Module) -> Option<&ModuleCfg> {
        self.modules.iter().find(|c| c.module == m)
    }

    /// Update interval of a module: its own override, else the general interval.
    pub fn module_interval_ms(&self, m: Module) -> u32 {
        self.module(m).and_then(|c| c.interval_s).map_or(self.interval_ms, |s| s.saturating_mul(1000))
    }

    /// Whether a module needs sampling: it has a taskbar cell, or the open flyout (`open`) shows it, as its own
    /// module or as data it borrows (`Module::flyout_needs`; top-process lists only with Processes' `flyout`).
    pub fn is_active(&self, m: Module, open: Option<Module>) -> bool {
        let Some(c) = self.module(m) else { return false };
        let borrowed = |o: Module| o.flyout_needs().contains(&m) && (m != Module::Processes || c.flyout);
        c.taskbar || open.is_some_and(|o| o == m || borrowed(o))
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
    fn default_styles_are_allowed() {
        for m in Config::default().modules {
            let allowed = m.module.allowed_styles();
            assert!(allowed.is_empty() || allowed.contains(&m.style), "{:?} {:?}", m.module, m.style);
        }
    }

    #[test]
    fn normalize_coerces_disallowed_style_to_first_allowed() {
        let mut cfg = Config::default();
        for m in &mut cfg.modules {
            m.style = match m.module {
                Module::Network => CellStyle::Text,
                Module::Battery => CellStyle::Io,
                Module::Disk => CellStyle::Graph,
                Module::Cpu => CellStyle::Bar,
                _ => m.style,
            };
        }
        cfg.normalize();
        let style = |m| cfg.module(m).map(|c| c.style);
        assert_eq!(style(Module::Network), Some(CellStyle::Io));
        assert_eq!(style(Module::Battery), Some(CellStyle::Text));
        assert_eq!(style(Module::Disk), Some(CellStyle::Io));
        assert_eq!(style(Module::Cpu), Some(CellStyle::Bar));
    }

    #[test]
    fn normalize_keeps_processes_off_the_taskbar() {
        let mut cfg = Config::default();
        for m in &mut cfg.modules {
            m.taskbar = true;
        }
        cfg.normalize();
        assert_eq!(cfg.module(Module::Processes).map(|c| c.taskbar), Some(false));
        assert_eq!(cfg.module(Module::Battery).map(|c| c.taskbar), Some(true));
    }

    #[test]
    fn io_style_roundtrips() {
        assert_eq!(serde_json::to_string(&CellStyle::Io).unwrap(), r#""Io""#);
        assert_eq!(serde_json::from_str::<CellStyle>(r#""Io""#).unwrap(), CellStyle::Io);
    }

    #[test]
    fn partial_json_falls_back_to_defaults() {
        let cfg: Config = serde_json::from_str(r#"{"interval_ms": 2000}"#).unwrap();
        assert_eq!(cfg.interval_ms, 2000);
        assert_eq!(cfg.anchor, Anchor::NearTray);
    }

    #[test]
    fn module_entry_missing_fields_gets_defaults() {
        let cfg: Config = serde_json::from_str(r#"{"modules": [{"module": "Gpu", "taskbar": true}]}"#).unwrap();
        assert_eq!(cfg.modules, vec![ModuleCfg::new(Module::Gpu, true, CellStyle::Text)]);
    }

    #[test]
    fn unparsable_module_entry_is_dropped_not_the_whole_file() {
        let json = r#"{"interval_ms": 2000, "modules": [
            {"module": "Cpu", "taskbar": false, "flyout": false, "style": "Bar"},
            {"module": "Fans", "taskbar": true},
            {"module": "Memory", "style": "Hologram"}
        ]}"#;
        let mut cfg: Config = serde_json::from_str(json).unwrap();
        cfg.normalize();
        assert_eq!(cfg.interval_ms, 2000);
        assert_eq!(cfg.modules[0], ModuleCfg { flyout: false, ..ModuleCfg::new(Module::Cpu, false, CellStyle::Bar) });
        assert_eq!(cfg.modules.len(), Module::ALL.len());
    }

    #[test]
    fn module_appearance_defaults() {
        for m in Config::default().modules {
            assert!(m.show_label && !m.color_by_load, "{:?}", m.module);
            assert_eq!((m.color, m.interval_s), (None, None));
        }
        let cfg = Config::default();
        let color = |m| cfg.module(m).map(ModuleCfg::color_index);
        assert_eq!(color(Module::Cpu), Some(0));
        assert_eq!(color(Module::Gpu), Some(1));
        assert_eq!(color(Module::Memory), Some(2));
        assert_eq!(color(Module::Disk), Some(3));
        assert_eq!(color(Module::Sensors), Some(4));
        assert!(Module::ALL.iter().all(|m| m.default_color() < PALETTE_LEN));
    }

    #[test]
    fn normalize_drops_bad_color_and_clamps_module_interval() {
        let mut cfg = Config::default();
        cfg.modules[0].color = Some(PALETTE_LEN);
        cfg.modules[0].interval_s = Some(0);
        cfg.modules[1].color = Some(5);
        cfg.modules[1].interval_s = Some(3600);
        cfg.normalize();
        assert_eq!((cfg.modules[0].color, cfg.modules[0].interval_s), (None, Some(1)));
        assert_eq!((cfg.modules[1].color, cfg.modules[1].interval_s), (Some(5), Some(10)));
    }

    #[test]
    fn module_interval_overrides_general() {
        let mut cfg = Config { interval_ms: 2000, ..Config::default() };
        cfg.modules[0].interval_s = Some(5);
        assert_eq!(cfg.module_interval_ms(cfg.modules[0].module), 5000);
        assert_eq!(cfg.module_interval_ms(cfg.modules[1].module), 2000);
    }

    #[test]
    fn module_without_new_fields_loads_with_defaults() {
        let m: ModuleCfg =
            serde_json::from_str(r#"{"module":"Memory","taskbar":true,"flyout":false,"style":"Bar"}"#).unwrap();
        assert_eq!(m, ModuleCfg { flyout: false, ..ModuleCfg::new(Module::Memory, true, CellStyle::Bar) });
    }

    #[test]
    fn json_roundtrip() {
        let mut cfg = Config { offset_px: -42, theme: ThemeMode::Dark, onboarded: true, ..Config::default() };
        cfg.modules[2] = ModuleCfg {
            show_label: false,
            color: Some(4),
            color_by_load: true,
            interval_s: Some(5),
            ..cfg.modules[2].clone()
        };
        cfg.options.network.interface = crate::NetInterface::Named("Ethernet 2".into());
        cfg.options.sensors.sensor = crate::SensorPick::Gpu;
        cfg.opt_in.memory_speed = true;
        let back: Config = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn is_active_requires_a_cell_or_the_open_flyout() {
        let cfg = Config::default();
        let active =
            |cfg: &Config, open| Module::ALL.into_iter().filter(|&m| cfg.is_active(m, open)).collect::<Vec<_>>();
        let cells = vec![Module::Cpu, Module::Memory, Module::Network, Module::Gpu];
        assert_eq!(active(&cfg, None), cells);
        // CPU's flyout borrows temperatures and processes; Battery's adds itself.
        assert_eq!(active(&cfg, Some(Module::Cpu)), [cells.clone(), vec![Module::Sensors, Module::Processes]].concat());
        assert_eq!(active(&cfg, Some(Module::Battery)), [cells.clone(), vec![Module::Battery]].concat());
        let disk = vec![Module::Cpu, Module::Memory, Module::Disk, Module::Network, Module::Gpu, Module::Sensors];
        assert_eq!(active(&cfg, Some(Module::Disk)), [disk, vec![Module::Processes]].concat());
        // Processes' `flyout` turns the top-process lists (and their sampling) off.
        let mut off = cfg.clone();
        off.modules.iter_mut().filter(|c| c.module == Module::Processes).for_each(|c| c.flyout = false);
        assert!(!off.is_active(Module::Processes, Some(Module::Disk)));
        assert!(off.is_active(Module::Sensors, Some(Module::Disk)));
    }
}
