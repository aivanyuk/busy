use busy_core::ThemeMode;
use windows::Win32::Foundation::{COLORREF, ERROR_SUCCESS, HWND};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::{BOOL, w};

pub const BG: COLORREF = COLORREF(0x202020);
pub const FIELD: COLORREF = COLORREF(0x2b2b2b);
pub const TEXT: COLORREF = COLORREF(0xffffff);
pub const DIM: COLORREF = COLORREF(0x7a7a7a);
pub const LINE: COLORREF = COLORREF(0x454545);

fn system_dark() -> bool {
    let (mut v, mut len) = (1u32, 4u32);
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut v as *mut u32).cast()),
            Some(&mut len),
        )
    };
    r == ERROR_SUCCESS && v == 0
}

/// `BUSY_FORCE_DARK=1|0` overrides; otherwise the app's theme setting, falling back to the Windows app mode.
pub fn wanted(theme: ThemeMode) -> bool {
    match std::env::var("BUSY_FORCE_DARK").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => match theme {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => system_dark(),
        },
    }
}

pub fn title_bar(hwnd: HWND, dark: bool) {
    let v = BOOL::from(dark);
    let _ = unsafe { DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, (&v as *const BOOL).cast(), 4) };
}
