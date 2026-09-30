//! Settings window for busy. The window functions must be called on the UI thread; the window does its
//! registry work on a thread of its own. [`autostart`] blocks on the registry, so keep it off a UI thread.
//!
//! Host integration: call [`open`] from the UI thread with a [`Host`]; while [`is_open`], call [`sync`] after
//! applying any config and [`refresh`] after each new sample; in the message loop call [`is_dialog_message`]
//! before `TranslateMessage`/`DispatchMessageW`.

pub mod autostart;
mod dark;
mod window;

use busy_core::{Config, Module, Snapshot};
use busy_ui::history::History;
use std::rc::Rc;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::MSG;

/// What the settings window needs from the app that opens it, called on the UI thread. `shown` is also called
/// from inside [`open`], so no method may call back into the window synchronously: post a message instead.
pub trait Host {
    /// A changed config, after every edit (changes apply live); the host applies and persists it.
    fn apply(&self, cfg: Config);
    /// The modules whose live readings the window shows (a module page's preview), empty when none and once
    /// the window closes: the host samples them while shown, even those without a taskbar cell.
    fn shown(&self, modules: &[Module]);
    /// Calls `f` with the host's latest readings and history, for drawing the preview; may skip the call
    /// when they are not available right now.
    fn with_data(&self, f: &mut dyn FnMut(&Snapshot, &History));
}

/// Opens the modeless settings window, or focuses it if already open. Autostart is applied to the registry by
/// the window itself, off the UI thread. `_owner` is unused: the window is an unowned top-level window with
/// its own taskbar button, since the host's window may be a child of explorer's taskbar. `page` shows that
/// module's page (a flyout's "<Module> settings" button), also when the window is already open.
pub fn open(_owner: HWND, cfg: &Config, host: Rc<dyn Host>, page: Option<Module>) {
    window::open(cfg, host, page);
}

/// Opens setup (onboarding; design "FIRST RUN") in the settings window, or turns the open window into it.
/// Choices apply live through [`Host::apply`]; finishing it (Skip, Start monitoring or Close) applies a config
/// with `onboarded` set and closes the window. The host opens it at startup while `onboarded` is false.
pub fn setup(cfg: &Config, host: Rc<dyn Host>) {
    window::setup(cfg, host);
}

/// The host applied `cfg`, from the window or anywhere else (the widget's menu): the window shows it.
pub fn sync(cfg: &Config) {
    window::sync(cfg);
}

/// New readings: the window redraws its preview if what it shows changed, and lists the machine's drives,
/// adapters and sensors in its dropdowns.
pub fn refresh(snap: &Snapshot, hist: &History) {
    window::refresh(snap, hist);
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
