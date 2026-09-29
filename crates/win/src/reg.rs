use crate::text::{from_wide, wide};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, REG_ROUTINE_FLAGS, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::core::PCWSTR;

/// Largest value we read. Registry values we care about are a few hundred bytes; a bigger one is not ours.
const MAX_LEN: u32 = 64 * 1024;

/// Raw bytes of `root\key\name` if its type matches `flags` (`RRF_RT_*`).
fn reg_raw(root: HKEY, key: &str, name: &str, flags: REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
    let (k, n) = (wide(key), wide(name));
    let (k, n) = (PCWSTR(k.as_ptr()), PCWSTR(n.as_ptr()));
    let mut len = 0u32;
    // SAFETY: `k` and `n` are NUL-terminated and outlive the call; a null data pointer asks only for the size.
    let r = unsafe { RegGetValueW(root, k, n, flags, None, None, Some(&mut len)) };
    if r != ERROR_SUCCESS || len > MAX_LEN {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` holds `len` writable bytes, and `len` tells the API so.
    let r = unsafe { RegGetValueW(root, k, n, flags, None, Some(buf.as_mut_ptr().cast()), Some(&mut len)) };
    // The value can change between the calls; ERROR_MORE_DATA then, and we just report it missing.
    (r == ERROR_SUCCESS).then(|| {
        buf.truncate(len as usize);
        buf
    })
}

/// `REG_BINARY` value.
pub fn reg_bytes(root: HKEY, key: &str, name: &str) -> Option<Vec<u8>> {
    reg_raw(root, key, name, RRF_RT_REG_BINARY)
}

/// `REG_DWORD` value.
pub fn reg_dword(root: HKEY, key: &str, name: &str) -> Option<u32> {
    let b = reg_raw(root, key, name, RRF_RT_REG_DWORD)?;
    Some(u32::from_le_bytes(b.get(..4)?.try_into().ok()?))
}

/// `REG_SZ` value, trimmed.
pub fn reg_string(root: HKEY, key: &str, name: &str) -> Option<String> {
    let b = reg_raw(root, key, name, RRF_RT_REG_SZ)?;
    let w: Vec<u16> = b.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).collect();
    Some(from_wide(&w).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;

    const NT: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

    #[test]
    fn reads_values_present_on_every_windows() {
        assert!(reg_string(HKEY_LOCAL_MACHINE, NT, "ProductName").is_some_and(|s| s.contains("Windows")));
        assert!(reg_dword(HKEY_LOCAL_MACHINE, NT, "CurrentMajorVersionNumber").is_some_and(|v| v >= 10));
    }

    #[test]
    fn wrong_type_or_missing_is_none() {
        assert_eq!(reg_dword(HKEY_LOCAL_MACHINE, NT, "ProductName"), None);
        assert_eq!(reg_string(HKEY_LOCAL_MACHINE, NT, "busy-no-such-value"), None);
    }
}
