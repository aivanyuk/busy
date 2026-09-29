//! Light/dark resolution and the design's colors: tokens (`design/Meterbar.dc.html`, `[data-mb]` for dark and
//! `[data-mb][data-theme=light]`), copied 1:1.

use busy_core::ThemeMode;
use busy_win::reg_dword;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::System::Registry::HKEY_CURRENT_USER;

pub(crate) type Color = D2D1_COLOR_F;

pub(crate) const fn rgb(v: u32) -> Color {
    rgba(v, 1.0)
}

pub(crate) const fn rgba(v: u32, a: f32) -> Color {
    Color { r: ((v >> 16) & 0xFF) as f32 / 255.0, g: ((v >> 8) & 0xFF) as f32 / 255.0, b: (v & 0xFF) as f32 / 255.0, a }
}

pub(crate) fn alpha(c: Color, a: f32) -> Color {
    Color { a: c.a * a, ..c }
}

/// One field per design token (`--tb` → `tb`, `--on-accent` → `on_accent`). Tokens only the settings window
/// uses (`--win`, `--card`, `--ctl*`, `--pop`, `--knob`) are left to Phase 4.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Theme {
    pub(crate) dark: bool,
    /// Taskbar surface. The widget draws over the real taskbar, so only the debug dump paints it.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(crate) tb: Color,
    #[allow(dead_code)] // Phase 2: divider between the cells and the tray.
    pub(crate) tb_line: Color,
    /// Flyout surface, painted over the acrylic backdrop (opaque when there is none).
    pub(crate) fly: Color,
    #[allow(dead_code)] // Used by the flyout border (widget-active commit).
    pub(crate) fly_line: Color,
    pub(crate) fg: Color,
    pub(crate) fg2: Color,
    pub(crate) fg3: Color,
    pub(crate) hover: Color,
    #[allow(dead_code)] // Used by the widget while its flyout is open (widget-active commit).
    pub(crate) active: Color,
    /// Empty part of bars and sparklines.
    pub(crate) track: Color,
    /// Chart background in the flyout.
    pub(crate) well: Color,
    pub(crate) line: Color,
    pub(crate) grid: Color,
    #[allow(dead_code)] // Phase 3: flyout footer.
    pub(crate) footer: Color,
    pub(crate) accent: Color,
    pub(crate) on_accent: Color,
    #[allow(dead_code)] // Phase 3: "Open Task Manager" link.
    pub(crate) link: Color,
    #[allow(dead_code)] // Phase 3: process icon placeholder.
    pub(crate) tile: Color,
    pub(crate) mem: Color,
    pub(crate) gpu: Color,
    pub(crate) rx: Color,
    pub(crate) tx: Color,
    pub(crate) battery: Color,
    pub(crate) warn: Color,
    pub(crate) crit: Color,
}

const DARK: Theme = Theme {
    dark: true,
    tb: rgba(0x1C1C1C, 0.84),
    tb_line: rgba(0xFFFFFF, 0.08),
    fly: rgba(0x2C2C2C, 0.86),
    fly_line: rgba(0xFFFFFF, 0.09),
    fg: rgb(0xFFFFFF),
    fg2: rgb(0xC8C8C8),
    fg3: rgb(0xA6A6A6),
    hover: rgba(0xFFFFFF, 0.06),
    active: rgba(0xFFFFFF, 0.09),
    track: rgba(0xFFFFFF, 0.12),
    well: rgba(0x000000, 0.2),
    line: rgba(0xFFFFFF, 0.07),
    grid: rgba(0xFFFFFF, 0.06),
    footer: rgba(0x000000, 0.18),
    accent: rgb(0x4CC2FF),
    on_accent: rgb(0x000000),
    link: rgb(0x60CDFF),
    tile: rgba(0xFFFFFF, 0.16),
    mem: rgb(0x6CCB5F),
    gpu: rgb(0xC09BFF),
    rx: rgb(0x4CC2FF),
    tx: rgb(0xFF8F5E),
    battery: rgb(0x6CCB5F),
    warn: rgb(0xFCE100),
    crit: rgb(0xFF6B6B),
};

const LIGHT: Theme = Theme {
    dark: false,
    tb: rgba(0xEEF0F4, 0.86),
    tb_line: rgba(0x000000, 0.08),
    fly: rgba(0xFAFAFA, 0.88),
    fly_line: rgba(0x000000, 0.08),
    fg: rgb(0x1A1A1A),
    fg2: rgb(0x4D4D4D),
    fg3: rgb(0x616161),
    hover: rgba(0x000000, 0.04),
    active: rgba(0x000000, 0.07),
    track: rgba(0x000000, 0.1),
    well: rgba(0x000000, 0.035),
    line: rgba(0x000000, 0.08),
    grid: rgba(0x000000, 0.06),
    footer: rgba(0x000000, 0.035),
    accent: rgb(0x005FB8),
    on_accent: rgb(0xFFFFFF),
    link: rgb(0x005FB8),
    tile: rgba(0x000000, 0.14),
    mem: rgb(0x0F7B0F),
    gpu: rgb(0x8764B8),
    rx: rgb(0x0067C0),
    tx: rgb(0xCA5010),
    battery: rgb(0x0F7B0F),
    warn: rgb(0x9D5D00),
    crit: rgb(0xC42B1C),
};

impl Theme {
    /// Reads the registry for `ThemeMode::System`: call on the theme reader, or at startup before the widget
    /// exists.
    pub(crate) fn resolve(mode: ThemeMode) -> Self {
        let dark = match mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            // The taskbar and its flyouts follow the *system* (not app) theme.
            ThemeMode::System => {
                reg_dword(
                    HKEY_CURRENT_USER,
                    r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                    "SystemUsesLightTheme",
                ) == Some(0)
            }
        };
        if dark { DARK } else { LIGHT }
    }

    /// Color for a utilization value: normal until 80%, then warning, critical from 95%.
    pub(crate) fn level(&self, base: Color, pct: f32) -> Color {
        match pct {
            p if p >= 95.0 => self.crit,
            p if p >= 80.0 => self.warn,
            _ => base,
        }
    }

    pub(crate) fn temp(&self, c: f32) -> Color {
        match c {
            c if c >= 90.0 => self.crit,
            c if c >= 75.0 => self.warn,
            _ => self.fg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_match_design() {
        assert_eq!((DARK.fg, DARK.accent, DARK.fg3), (rgb(0xFFFFFF), rgb(0x4CC2FF), rgb(0xA6A6A6)));
        assert_eq!((LIGHT.fg, LIGHT.accent, LIGHT.on_accent), (rgb(0x1A1A1A), rgb(0x005FB8), rgb(0xFFFFFF)));
        assert_eq!(DARK.fly, Color { r: 44.0 / 255.0, g: 44.0 / 255.0, b: 44.0 / 255.0, a: 0.86 });
        assert!(Theme::resolve(ThemeMode::Dark).dark && !Theme::resolve(ThemeMode::Light).dark);
    }
}
