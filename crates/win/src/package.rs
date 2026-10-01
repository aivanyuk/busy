//! Whether busy runs from a package (installed from the Microsoft Store) or as the portable exe. The same exe
//! ships both ways; the few things that differ ask here.

use crate::from_wide;
use std::sync::OnceLock;
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;
use windows::core::PWSTR;

/// The package family name this process runs under ("Publisher.busy_8wekyb3d8bbwe"), or None for the portable
/// exe. A process's package identity never changes, so it is read once.
pub fn package_family() -> Option<&'static str> {
    static FAMILY: OnceLock<Option<String>> = OnceLock::new();
    FAMILY.get_or_init(read).as_deref()
}

fn read() -> Option<String> {
    let mut len = 0;
    // SAFETY: a length query; without a package it fails with APPMODEL_ERROR_NO_PACKAGE.
    if unsafe { GetCurrentPackageFamilyName(&mut len, None) } != ERROR_INSUFFICIENT_BUFFER {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    // SAFETY: `buf` holds the `len` characters, NUL included, that the first call asked for.
    let e = unsafe { GetCurrentPackageFamilyName(&mut len, Some(PWSTR(buf.as_mut_ptr()))) };
    (e == ERROR_SUCCESS).then(|| from_wide(&buf)).filter(|f| !f.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn tests_run_unpackaged() {
        assert_eq!(super::package_family(), None);
    }
}
