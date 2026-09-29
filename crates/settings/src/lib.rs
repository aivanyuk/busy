//! Settings window for busy. The window functions must be called on the UI thread; the window does its
//! registry work on a thread of its own. [`autostart`] blocks on the registry, so keep it off a UI thread.
//!
//! Host integration: call [`open`] from the UI thread, and in the message loop call
//! [`is_dialog_message`] before `TranslateMessage`/`DispatchMessageW` so keyboard navigation works.

pub mod autostart;
mod dark;
mod window;

use busy_core::{Config, Module};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::MSG;

/// Opens the modeless settings window, or focuses it if already open.
/// `on_apply` is invoked on the UI thread with the new config on Apply/OK (only if something changed);
/// the caller persists it. Autostart is applied to the registry by the window itself, off the UI thread.
/// `_owner` is unused: the window is an unowned top-level window with its own taskbar button,
/// since the host's window may be a child of explorer's taskbar. `page` selects that module's row (a flyout's
/// "<Module> settings" button), also when the window is already open.
pub fn open(_owner: HWND, cfg: &Config, on_apply: Box<dyn Fn(Config)>, page: Option<Module>) {
    window::open(cfg, on_apply, page);
}

/// Whether the settings window currently exists.
pub fn is_open() -> bool {
    window::hwnd().is_some()
}

/// Returns true if `msg` was consumed by the settings window's keyboard handling. The custom-drawn window
/// handles its keys in its own window procedure, so nothing is consumed here; hosts keep calling it so the
/// message loop needs no change if that moves.
pub fn is_dialog_message(_msg: &MSG) -> bool {
    false
}
