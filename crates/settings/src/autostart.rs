//! Per-user autostart via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `busy`.

use busy_win::{from_wide, reg_bytes, reg_string, wide};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;
use windows::core::{Error, HRESULT, PCWSTR, Result};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Task Manager / Settings "Startup apps" toggle; an odd first byte means disabled.
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const NAME: &str = "busy";

fn check(e: WIN32_ERROR) -> Result<()> {
    if e == ERROR_SUCCESS { Ok(()) } else { Err(Error::from_hresult(HRESULT::from_win32(e.0))) }
}

fn exe() -> Result<String> {
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(None, &mut buf) } as usize;
    if n == 0 {
        return Err(Error::from_thread());
    }
    Ok(from_wide(&buf[..n]))
}

fn delete(subkey: &str) -> Result<()> {
    let (k, n) = (wide(subkey), wide(NAME));
    // SAFETY: both strings are NUL-terminated and outlive the call.
    match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(n.as_ptr())) } {
        ERROR_FILE_NOT_FOUND => Ok(()),
        e => check(e),
    }
}

/// True if the Run entry exists, points at the current executable and isn't disabled in Task Manager.
pub fn is_enabled() -> bool {
    let Some(cmd) = reg_string(HKEY_CURRENT_USER, RUN, NAME) else { return false };
    let path = match cmd.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or_default(),
        None => &cmd,
    };
    let approved = reg_bytes(HKEY_CURRENT_USER, APPROVED, NAME).is_none_or(|b| b.first().is_none_or(|f| f & 1 == 0));
    approved && exe().is_ok_and(|e| e.to_lowercase() == path.to_lowercase())
}

pub fn set(enabled: bool) -> Result<()> {
    if enabled {
        let data = wide(&format!("\"{}\"", exe()?));
        let (k, n) = (wide(RUN), wide(NAME));
        // SAFETY: the strings are NUL-terminated; `data` holds `data.len() * 2` readable bytes including the NUL.
        check(unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(k.as_ptr()),
                PCWSTR(n.as_ptr()),
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        })?;
    } else {
        delete(RUN)?;
    }
    delete(APPROVED)
}
