//! Which design color a reading is drawn in: the module palette, the load colors and the rules that pick
//! between them (design `col()`, `valueColor`, `upColor()`). Pure logic, no Win32: `Theme::color` turns a
//! `Tone` into the current theme's value.

use busy_core::{BatteryInfo, ModuleCfg};

/// Load color steps (design `col()`): below 60 → normal, below 85 → elevated, else high.
const LOAD_STEPS: [f32; 2] = [60.0, 85.0];

/// A color by its role in the design, the same in dark and light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Primary text (`--fg`).
    Fg,
    /// Module palette entry (`PAL`), `0..PALETTE_LEN`.
    Pal(u8),
    /// Load color (`LOAD`).
    Load(Load),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    Normal,
    Elevated,
    High,
}

/// Second series next to the module color: upload, write (design `upColor()` = `PAL[4]`).
pub const SECOND: Tone = Tone::Pal(4);

pub fn load(pct: f32) -> Load {
    match LOAD_STEPS.iter().take_while(|&&s| pct >= s).count() {
        0 => Load::Normal,
        1 => Load::Elevated,
        _ => Load::High,
    }
}

/// The module's palette color (its `color`, else `Module::default_color`).
pub fn module(mc: &ModuleCfg) -> Tone {
    Tone::Pal(mc.color_index())
}

/// Graph and bar color at `pct` load (design `col()`): the load color with `color_by_load`, else the module's.
/// Rates have no percentage and use `module`/`SECOND` directly.
pub fn fill(mc: &ModuleCfg, pct: f32) -> Tone {
    if mc.color_by_load { Tone::Load(load(pct)) } else { module(mc) }
}

/// Value text color (design `valueColor`): the load color with `color_by_load`, else `Fg`.
pub fn value(mc: &ModuleCfg, pct: f32) -> Tone {
    if mc.color_by_load { Tone::Load(load(pct)) } else { Tone::Fg }
}

/// (fill, value) of the battery. Its load is how empty it is (100 − charge), so a full battery is not red;
/// below 20 % on battery both are the high-load color whatever the settings, as a low-battery warning.
pub fn battery(mc: &ModuleCfg, b: &BatteryInfo) -> (Tone, Tone) {
    if b.percent < 20.0 && !b.charging {
        (Tone::Load(Load::High), Tone::Load(Load::High))
    } else {
        let drained = 100.0 - b.percent;
        (fill(mc, drained), value(mc, drained))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use busy_core::{CellStyle, Module};

    #[test]
    fn load_steps() {
        let cases = [
            (0.0, Load::Normal),
            (59.9, Load::Normal),
            (60.0, Load::Elevated),
            (84.9, Load::Elevated),
            (85.0, Load::High),
            (100.0, Load::High),
            (250.0, Load::High),
            (f32::NAN, Load::Normal),
        ];
        for (pct, l) in cases {
            assert_eq!(load(pct), l, "{pct}");
        }
    }

    #[test]
    fn module_colors_follow_config() {
        let mut mc = ModuleCfg::new(Module::Gpu, true, CellStyle::Graph);
        assert_eq!(module(&mc), Tone::Pal(1));
        mc.color = Some(5);
        assert_eq!(fill(&mc, 99.0), Tone::Pal(5));
        assert_eq!(value(&mc, 99.0), Tone::Fg);
        mc.color_by_load = true;
        assert_eq!(fill(&mc, 99.0), Tone::Load(Load::High));
        assert_eq!(value(&mc, 10.0), Tone::Load(Load::Normal));
        assert_eq!(module(&mc), Tone::Pal(5));
    }

    #[test]
    fn battery_load_is_depletion() {
        let mut mc = ModuleCfg::new(Module::Battery, true, CellStyle::Text);
        let bat = |percent, charging| BatteryInfo { percent, charging, ..BatteryInfo::default() };
        let high = Tone::Load(Load::High);
        assert_eq!(battery(&mc, &bat(90.0, false)), (Tone::Pal(2), Tone::Fg));
        assert_eq!(battery(&mc, &bat(10.0, false)), (high, high));
        assert_eq!(battery(&mc, &bat(10.0, true)).0, Tone::Pal(2));
        mc.color_by_load = true;
        assert_eq!(battery(&mc, &bat(90.0, false)), (Tone::Load(Load::Normal), Tone::Load(Load::Normal)));
        assert_eq!(battery(&mc, &bat(30.0, false)).0, Tone::Load(Load::Elevated));
        assert_eq!(battery(&mc, &bat(10.0, true)).0, high);
    }
}
