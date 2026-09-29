//! Light/dark resolution and the design's colors: tokens (`design/Meterbar.dc.html`, `[data-mb]` for dark and
//! `[data-mb][data-theme=light]`), module palette `PAL` and load colors `LOAD`, copied 1:1.

use crate::tone::{Load, Tone};
use busy_core::{PALETTE_LEN, ThemeMode};
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
/// uses (`--win`, `--card`, `--ctl*`, `--pop`, `--knob`) are left to Phase 4; `dim` is a color the design's `fly()`
/// hard-codes per theme, like `standby`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Theme {
    pub(crate) dark: bool,
    /// Taskbar surface. The widget draws over the real taskbar, so only the debug dump paints it.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(crate) tb: Color,
    #[allow(dead_code)] // The taskbar's top border: Windows draws it (the divider before the tray is `line`).
    pub(crate) tb_line: Color,
    /// Flyout surface, painted over the acrylic backdrop (opaque when there is none).
    pub(crate) fly: Color,
    /// Flyout border (DWM draws it; blended over `fly` since it takes no alpha).
    pub(crate) fly_line: Color,
    pub(crate) fg: Color,
    pub(crate) fg2: Color,
    pub(crate) fg3: Color,
    pub(crate) hover: Color,
    /// The widget while its flyout is open.
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
    /// Process icon placeholder.
    pub(crate) tile: Color,
    /// Idle / free parts of a composition (design `dim`).
    pub(crate) dim: Color,
    /// Memory standby list (design `sbc`).
    pub(crate) standby: Color,
    /// Module palette (design `PAL`), indexed by `ModuleCfg::color_index`.
    pub(crate) pal: [Color; PALETTE_LEN as usize],
    /// Load colors (design `LOAD`): normal, elevated, high.
    pub(crate) load: [Color; 3],
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
    dim: rgb(0x6B6B6B),
    standby: rgb(0x3E6A80),
    pal: [rgb(0x60CDFF), rgb(0xC3A1FF), rgb(0x6FD49A), rgb(0xE8C46A), rgb(0xFF9B7A), rgb(0xFF8AC6)],
    load: [rgb(0x6FD49A), rgb(0xF2C661), rgb(0xFF7B72)],
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
    dim: rgb(0xB8B8B8),
    standby: rgb(0x9CC3E0),
    pal: [rgb(0x0067C0), rgb(0x7A4FC4), rgb(0x0E7A45), rgb(0x8A5E00), rgb(0xC24A26), rgb(0xB8327A)],
    load: [rgb(0x0E7A45), rgb(0x8A5E00), rgb(0xC42B1C)],
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

    /// This theme's value of a design color role (`tone.rs` decides which role).
    pub(crate) fn color(&self, tone: Tone) -> Color {
        match tone {
            Tone::Fg => self.fg,
            Tone::Pal(i) => self.pal[(i as usize).min(self.pal.len() - 1)],
            Tone::Load(Load::Normal) => self.load[0],
            Tone::Load(Load::Elevated) => self.load[1],
            Tone::Load(Load::High) => self.load[2],
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
        assert_eq!((DARK.pal[0], LIGHT.load[2]), (rgb(0x60CDFF), rgb(0xC42B1C)));
    }

    #[test]
    fn tones_map_to_the_theme() {
        assert_eq!(DARK.color(Tone::Fg), DARK.fg);
        assert_eq!(LIGHT.color(Tone::Pal(4)), LIGHT.pal[4]);
        assert_eq!(LIGHT.color(Tone::Pal(PALETTE_LEN)), LIGHT.pal[5]);
        assert_eq!(DARK.color(Tone::Load(Load::Elevated)), rgb(0xF2C661));
    }
}
