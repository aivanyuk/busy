use crate::text::{from_wide, wide};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, KEY_ENUMERATE_SUB_KEYS, REG_ROUTINE_FLAGS, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RRF_RT_REG_QWORD,
    RRF_RT_REG_SZ, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows::core::{Owned, PCWSTR, PWSTR};

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

/// `REG_QWORD` value.
pub fn reg_qword(root: HKEY, key: &str, name: &str) -> Option<u64> {
    let b = reg_raw(root, key, name, RRF_RT_REG_QWORD)?;
    Some(u64::from_le_bytes(b.get(..8)?.try_into().ok()?))
}

/// Names of the subkeys of `root\key`, at most `max` of them; empty if the key cannot be opened.
pub fn reg_subkeys(root: HKEY, key: &str, max: usize) -> Vec<String> {
    let k = wide(key);
    let mut h = HKEY::default();
    // SAFETY: `k` is NUL-terminated and outlives the call; `h` is a valid out-pointer. Enumeration is the only
    // right the key is opened with.
    if unsafe { RegOpenKeyExW(root, PCWSTR(k.as_ptr()), None, KEY_ENUMERATE_SUB_KEYS, &mut h) } != ERROR_SUCCESS {
        return Vec::new();
    }
    // SAFETY: on success we own the opened key; `Owned` closes it.
    let h = unsafe { Owned::new(h) };
    // Key names are at most 255 characters.
    let mut name = [0u16; 256];
    let mut out = Vec::new();
    for i in 0..max.min(u32::MAX as usize) as u32 {
        let mut len = name.len() as u32;
        // SAFETY: `name` holds `len` writable u16s, and `len` tells the API so.
        let r = unsafe { RegEnumKeyExW(*h, i, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) };
        if r != ERROR_SUCCESS {
            break;
        }
        out.push(from_wide(name.get(..len as usize).unwrap_or(&name)));
    }
    out
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
        assert_eq!(reg_qword(HKEY_LOCAL_MACHINE, NT, "CurrentMajorVersionNumber"), None);
        assert_eq!(reg_string(HKEY_LOCAL_MACHINE, NT, "busy-no-such-value"), None);
    }

    #[test]
    fn reads_qwords() {
        // Setup writes the install time as a REG_QWORD FILETIME.
        assert!(reg_qword(HKEY_LOCAL_MACHINE, NT, "InstallTime").is_some_and(|t| t > 0));
    }

    #[test]
    fn lists_subkeys() {
        let parent = r"SOFTWARE\Microsoft\Windows NT";
        let all = reg_subkeys(HKEY_LOCAL_MACHINE, parent, usize::MAX);
        assert!(all.iter().any(|k| k == "CurrentVersion"), "{all:?}");
        assert_eq!(reg_subkeys(HKEY_LOCAL_MACHINE, parent, 1), all[..1]);
        assert!(reg_subkeys(HKEY_LOCAL_MACHINE, r"SOFTWAREusy-no-such-key", 10).is_empty());
    }
}
