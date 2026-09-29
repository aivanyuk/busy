use busy_core::ThemeMode;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::System::Registry::HKEY_CURRENT_USER;
use windows::core::BOOL;

/// Windows app mode (`AppsUseLightTheme`). Reads the registry: call it off the UI thread.
pub(crate) fn system_dark() -> bool {
    let key = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    busy_win::reg_dword(HKEY_CURRENT_USER, key, "AppsUseLightTheme") == Some(0)
}

/// `BUSY_FORCE_DARK=1|0` overrides; otherwise the app's theme setting, falling back to the Windows app mode
/// (`system_dark`, as last read by the caller's worker).
pub fn wanted(theme: ThemeMode, system_dark: bool) -> bool {
    match std::env::var("BUSY_FORCE_DARK").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => match theme {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => system_dark,
        },
    }
}

pub fn title_bar(hwnd: HWND, dark: bool) {
    let v = BOOL::from(dark);
    let _ = unsafe { DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, (&v as *const BOOL).cast(), 4) };
}
