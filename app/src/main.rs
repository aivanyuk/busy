#![windows_subsystem = "windows"]
// link.exe's built-in manifest validator predates <dpiAwareness> (warning 81010002); it is embedded regardless.
#![allow(linker_messages)]

mod app;
mod ctx;
mod fake;
mod flyout;
mod fmt;
mod history;
mod persist;
mod render;
mod sampler;
mod select;
mod taskbar;
mod theme;
mod win;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

fn main() {
    // Held (never closed) for the process lifetime.
    let Ok(_mutex) = (unsafe { CreateMutexW(None, true, w!("Local\\busy-single-instance")) }) else { return };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return;
    }
    // `--open-flyout`: open the flyout after the first sample (debugging/screenshots).
    let open_flyout = std::env::args().any(|a| a == "--open-flyout");
    let _ = app::run(open_flyout);
}
