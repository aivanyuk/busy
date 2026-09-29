#![windows_subsystem = "windows"]
// link.exe's built-in manifest validator predates <dpiAwareness> (warning 81010002); it is embedded regardless.
#![allow(linker_messages)]

mod app;
mod ctx;
#[cfg(debug_assertions)]
mod fake;
mod flyout;
mod fmt;
mod history;
mod launch;
mod menu;
mod render;
mod sampler;
mod select;
mod sync;
mod taskbar;
mod theme;
mod tone;
mod win;
mod worker;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::LibraryLoader::{LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

fn main() {
    // Every later LoadLibrary (COM in-proc servers, delay loads, our own) searches System32 only, so a DLL
    // planted next to the exe or in the working directory is never picked up. Best effort: on failure the
    // defaults stay, and vendor DLLs are still loaded with an explicit System32 flag.
    // SAFETY: takes a flag value only; called before any thread or DLL load of ours.
    let _ = unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) };
    // Held (never closed) for the process lifetime.
    let Ok(_mutex) = (unsafe { CreateMutexW(None, true, w!("Local\\busy-single-instance")) }) else { return };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return;
    }
    // Debug builds: `--open-flyout` opens the flyout after the first sample (screenshots).
    #[cfg(debug_assertions)]
    let open_flyout = std::env::args().any(|a| a == "--open-flyout");
    #[cfg(not(debug_assertions))]
    let open_flyout = false;
    let _ = app::run(open_flyout);
}
