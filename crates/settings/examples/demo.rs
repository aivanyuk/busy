#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Standalone host for the settings window. `BUSY_FORCE_DARK=1` forces dark mode.
// link.exe's manifest schema predates <dpiAwareness> and warns (81010002); the element is still embedded.
#![allow(linker_messages)]

use busy_core::Config;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, TranslateMessage};

fn main() {
    busy_settings::open(HWND::default(), &Config::load(), Box::new(|c| println!("{c:#?}")), None);
    let mut msg = MSG::default();
    while busy_settings::is_open() && unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        if !busy_settings::is_dialog_message(&msg) {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
