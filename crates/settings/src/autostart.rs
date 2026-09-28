//! Per-user autostart via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `busy`.

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;
use windows::core::{Error, HRESULT, PCWSTR, Result, w};

const RUN: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
/// Task Manager / Settings "Startup apps" toggle; an odd first byte means disabled.
const APPROVED: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");
const NAME: PCWSTR = w!("busy");

fn check(e: WIN32_ERROR) -> Result<()> {
    if e == ERROR_SUCCESS { Ok(()) } else { Err(Error::from_hresult(HRESULT::from_win32(e.0))) }
}

fn exe() -> Result<String> {
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(None, &mut buf) } as usize;
    if n == 0 {
        return Err(Error::from_thread());
    }
    Ok(String::from_utf16_lossy(&buf[..n]))
}

fn read(subkey: PCWSTR, flags: REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
    let mut len = 0u32;
    unsafe {
        check(RegGetValueW(HKEY_CURRENT_USER, subkey, NAME, flags, None, None, Some(&mut len))).ok()?;
        let mut buf = vec![0u8; len as usize];
        check(RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            NAME,
            flags,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        ))
        .ok()?;
        buf.truncate(len as usize);
        Some(buf)
    }
}

fn delete(subkey: PCWSTR) -> Result<()> {
    match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey, NAME) } {
        ERROR_FILE_NOT_FOUND => Ok(()),
        e => check(e),
    }
}

/// True if the Run entry exists, points at the current executable and isn't disabled in Task Manager.
pub fn is_enabled() -> bool {
    let Some(raw) = read(RUN, RRF_RT_REG_SZ) else { return false };
    let wide: Vec<u16> =
        raw.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).take_while(|&c| c != 0).collect();
    let cmd = String::from_utf16_lossy(&wide);
    let cmd = cmd.trim();
    let path = match cmd.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or_default(),
        None => cmd,
    };
    let approved = read(APPROVED, RRF_RT_REG_BINARY).is_none_or(|b| b.first().is_none_or(|f| f & 1 == 0));
    approved && exe().is_ok_and(|e| e.to_lowercase() == path.to_lowercase())
}

pub fn set(enabled: bool) -> Result<()> {
    if enabled {
        let data: Vec<u16> = format!("\"{}\"", exe()?).encode_utf16().chain([0]).collect();
        check(unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, RUN, NAME, REG_SZ.0, Some(data.as_ptr().cast()), (data.len() * 2) as u32)
        })?;
    } else {
        delete(RUN)?;
    }
    delete(APPROVED)
}
