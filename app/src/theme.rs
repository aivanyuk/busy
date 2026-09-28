//! Light/dark resolution, accent color and palette.

use busy_core::ThemeMode;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Dwm::DwmGetColorizationColor;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::{BOOL, w};

pub type Color = D2D1_COLOR_F;

pub const fn rgb(v: u32) -> Color {
    rgba(v, 1.0)
}

pub const fn rgba(v: u32, a: f32) -> Color {
    Color { r: ((v >> 16) & 0xFF) as f32 / 255.0, g: ((v >> 8) & 0xFF) as f32 / 255.0, b: (v & 0xFF) as f32 / 255.0, a }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a: c.a * a, ..c }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub dark: bool,
    pub text: Color,
    pub secondary: Color,
    pub tertiary: Color,
    /// Solid flyout background when no system backdrop is available.
    pub background: Color,
    /// Subtle card/track fill (graph backgrounds, empty bar parts).
    pub track: Color,
    pub separator: Color,
    pub hover: Color,
    pub accent: Color,
    pub mem: Color,
    pub gpu: Color,
    pub rx: Color,
    pub tx: Color,
    pub battery: Color,
    pub warn: Color,
    pub crit: Color,
}

impl Theme {
    pub fn resolve(mode: ThemeMode) -> Self {
        let dark = match mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            // The taskbar and its flyouts follow the *system* (not app) theme.
            ThemeMode::System => {
                reg_dword(
                    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                    w!("SystemUsesLightTheme"),
                ) == Some(0)
            }
        };
        let accent = accent(dark);
        if dark {
            Self {
                dark,
                text: rgb(0xFFFFFF),
                secondary: rgba(0xFFFFFF, 0.62),
                tertiary: rgba(0xFFFFFF, 0.40),
                background: rgb(0x202020),
                track: rgba(0xFFFFFF, 0.07),
                separator: rgba(0xFFFFFF, 0.09),
                hover: rgba(0xFFFFFF, 0.08),
                accent,
                mem: rgb(0x6CCB5F),
                gpu: rgb(0xC09BFF),
                rx: rgb(0x4CC2FF),
                tx: rgb(0xFF8F5E),
                battery: rgb(0x6CCB5F),
                warn: rgb(0xFCE100),
                crit: rgb(0xFF6B6B),
            }
        } else {
            Self {
                dark,
                text: rgba(0x000000, 0.90),
                secondary: rgba(0x000000, 0.60),
                tertiary: rgba(0x000000, 0.42),
                background: rgb(0xF3F3F3),
                track: rgba(0x000000, 0.06),
                separator: rgba(0x000000, 0.08),
                hover: rgba(0x000000, 0.06),
                accent,
                mem: rgb(0x0F7B0F),
                gpu: rgb(0x8764B8),
                rx: rgb(0x0067C0),
                tx: rgb(0xCA5010),
                battery: rgb(0x0F7B0F),
                warn: rgb(0x9D5D00),
                crit: rgb(0xC42B1C),
            }
        }
    }

    /// Color for a utilization value: normal until 80%, then warning, critical from 95%.
    pub fn level(&self, base: Color, pct: f32) -> Color {
        match pct {
            p if p >= 95.0 => self.crit,
            p if p >= 80.0 => self.warn,
            _ => base,
        }
    }

    pub fn temp(&self, c: f32) -> Color {
        match c {
            c if c >= 90.0 => self.crit,
            c if c >= 75.0 => self.warn,
            _ => self.text,
        }
    }
}

fn reg_dword(key: windows::core::PCWSTR, value: windows::core::PCWSTR) -> Option<u32> {
    let (mut v, mut n) = (0u32, 4u32);
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key, value, RRF_RT_REG_DWORD, None, Some(&mut v as *mut _ as _), Some(&mut n))
    };
    r.is_ok().then_some(v)
}

/// Accent shade tuned for the theme: `AccentPalette` holds 8 RGBA swatches, light (0) to dark (7), base at 3.
fn accent(dark: bool) -> Color {
    let mut pal = [0u8; 32];
    let mut n = pal.len() as u32;
    let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent");
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            w!("AccentPalette"),
            RRF_RT_REG_BINARY,
            None,
            Some(pal.as_mut_ptr() as _),
            Some(&mut n),
        )
    };
    if ok.is_ok() && n == 32 {
        let i = if dark { 1 } else { 4 } * 4;
        return rgb(u32::from_be_bytes([0, pal[i], pal[i + 1], pal[i + 2]]));
    }
    let (mut c, mut opaque) = (0u32, BOOL(0));
    if unsafe { DwmGetColorizationColor(&mut c, &mut opaque) }.is_ok() {
        return rgb(c & 0xFF_FFFF);
    }
    if dark { rgb(0x60CDFF) } else { rgb(0x005FB8) }
}
