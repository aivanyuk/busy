//! Whether the user can see anything: session lock/unlock and console display on/off notifications, which
//! pause the sampler. Registered and unregistered with the main window.

use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM};
use windows::Win32::System::Power::{
    HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification, UnregisterPowerSettingNotification,
};
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_WINDOW_HANDLE;

/// `RegisterPowerSettingNotification` handle for the display state, unregistered with the main window.
static DISPLAY_NOTIFY: AtomicIsize = AtomicIsize::new(0);

/// Lock/unlock (`WM_WTSSESSION_CHANGE`) and display on/off (`WM_POWERBROADCAST`) for `hwnd`. Registration
/// sends the current display state at once.
pub(crate) fn register(hwnd: HWND) {
    // SAFETY: `hwnd` is our live main window; the GUID outlives the call.
    unsafe {
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        if let Ok(h) =
            RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE)
        {
            DISPLAY_NOTIFY.store(h.0, Ordering::Relaxed);
        }
    }
}

/// Undoes `register`. Both calls are RPCs to system services: call it only once our child of explorer's
/// taskbar is gone, so a slow one can't stall the taskbar's input queue.
pub(crate) fn unregister(hwnd: HWND) {
    // SAFETY: `hwnd` is the window `register` was called with; the handle is taken once.
    unsafe {
        let _ = WTSUnRegisterSessionNotification(hwnd);
        let notify = DISPLAY_NOTIFY.swap(0, Ordering::Relaxed);
        if notify != 0 {
            let _ = UnregisterPowerSettingNotification(HPOWERNOTIFY(notify));
        }
    }
}

/// Whether a `PBT_POWERSETTINGCHANGE` for `GUID_CONSOLE_DISPLAY_STATE` says the display is on (dimmed counts
/// as on); `None` for any other setting.
pub(crate) fn display_state(lp: LPARAM) -> Option<bool> {
    let p = lp.0 as *const POWERBROADCAST_SETTING;
    if p.is_null() {
        return None;
    }
    // SAFETY: for PBT_POWERSETTINGCHANGE the system passes a POWERBROADCAST_SETTING valid for this message,
    // with `DataLength` bytes of data from `Data`; the length is checked before the unaligned read.
    unsafe {
        let s = &*p;
        if s.PowerSetting != GUID_CONSOLE_DISPLAY_STATE || (s.DataLength as usize) < size_of::<u32>() {
            return None;
        }
        Some(std::ptr::addr_of!(s.Data).cast::<u32>().read_unaligned() != 0)
    }
}
