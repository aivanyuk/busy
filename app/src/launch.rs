//! Starting other programs. Blocking (the shell may take a while), so it runs on the launcher worker, never
//! on the UI thread.

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, w};

/// Opens Task Manager from System32 by full path (never a search-path lookup). `ShellExecuteW`, not
/// `CreateProcessW`: Task Manager asks for the highest available elevation, which only the shell handles.
pub(crate) fn task_manager() {
    let mut dir = [0u16; 260];
    // SAFETY: `dir` is a writable buffer of the length passed.
    let n = unsafe { GetSystemDirectoryW(Some(&mut dir)) } as usize;
    if n == 0 || n >= dir.len() {
        return;
    }
    let path = HSTRING::from(format!("{}\\Taskmgr.exe", String::from_utf16_lossy(&dir[..n])));
    // SAFETY: COM is initialised for this call and balanced below; `path` and the verb outlive ShellExecuteW.
    unsafe {
        // The shell wants an apartment-threaded caller.
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        ShellExecuteW(None, w!("open"), &path, None, None, SW_SHOWNORMAL);
        if com {
            CoUninitialize();
        }
    }
}
