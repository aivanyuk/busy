//! Settings window for busy. All functions must be called on the UI thread.
//!
//! Host integration: call [`open`] from the UI thread, and in the message loop call
//! [`is_dialog_message`] before `TranslateMessage`/`DispatchMessageW` so keyboard navigation works.

pub mod autostart;
mod window;

use busy_core::Config;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IsDialogMessageW, MSG};

/// Opens the modeless settings window, or focuses it if already open.
/// `on_apply` is invoked on the UI thread with the new config on Apply/OK (only if something changed);
/// the caller persists it. Autostart is applied to the registry by the window itself.
/// `_owner` is unused: the window is an unowned top-level window with its own taskbar button,
/// since the host's window may be a child of explorer's taskbar.
pub fn open(_owner: HWND, cfg: &Config, on_apply: Box<dyn Fn(Config)>) {
    window::open(cfg, on_apply);
}

/// Whether the settings window currently exists.
pub fn is_open() -> bool {
    window::hwnd().is_some()
}

/// Returns true if `msg` was consumed by the settings window's dialog navigation (Tab, Enter, Esc, mnemonics).
pub fn is_dialog_message(msg: &MSG) -> bool {
    window::hwnd().is_some_and(|h| unsafe { IsDialogMessageW(h, msg) }.as_bool())
}
