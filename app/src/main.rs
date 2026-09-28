#![windows_subsystem = "windows"]
// link.exe's built-in manifest validator predates <dpiAwareness> (warning 81010002); it is embedded regardless.
#![allow(linker_messages)]

mod app;
mod fake;
// Building blocks partly used only by the flyout, which lands in a later commit.
#[allow(dead_code)]
mod fmt;
#[allow(dead_code)]
mod history;
#[allow(dead_code)]
mod render;
mod sampler;
mod taskbar;
#[allow(dead_code)]
mod theme;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

fn main() {
    // Held (never closed) for the process lifetime.
    let Ok(_mutex) = (unsafe { CreateMutexW(None, true, w!("Local\\busy-single-instance")) }) else { return };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return;
    }
    let _ = app::run();
}
