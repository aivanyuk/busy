//! The settings window's view of the app (`busy_settings::Host`).

use crate::app::{WM_APP_SETTINGS_PAGE, main_hwnd, submit_config, with_readings};
use busy_core::{Config, Module, Snapshot};
use busy_ui::history::History;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// `page` is also called inside `busy_settings::open`, while the app state is borrowed, so configs and pages
/// arrive as posted messages; readings are lent only when the state is free.
pub(crate) struct SettingsHost;

impl busy_settings::Host for SettingsHost {
    fn apply(&self, cfg: Config) {
        submit_config(cfg);
    }

    fn page(&self, m: Option<Module>) {
        // SAFETY: PostMessageW only queues a message to our own window.
        let wp = WPARAM(m.map_or(0, |m| m.index() + 1));
        let _ = unsafe { PostMessageW(Some(main_hwnd()), WM_APP_SETTINGS_PAGE, wp, LPARAM(0)) };
    }

    fn with_data(&self, f: &mut dyn FnMut(&Snapshot, &History)) {
        with_readings(|snap, hist| f(snap, hist));
    }
}
