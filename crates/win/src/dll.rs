use std::ffi::CStr;
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::core::{PCSTR, PCWSTR};

/// A DLL loaded strictly from System32, freed on drop. The only `LoadLibraryExW` in the workspace.
pub struct Dll(HMODULE);

impl Dll {
    /// `name` is a bare file name; absence is normal and yields `None`.
    pub fn load(name: PCWSTR) -> Option<Self> {
        // SAFETY: `name` is a valid NUL-terminated string; the search is restricted to System32.
        unsafe { LoadLibraryExW(name, None, LOAD_LIBRARY_SEARCH_SYSTEM32) }.ok().map(Self)
    }

    /// # Safety
    /// `T` must be the `extern fn` type matching the export's real signature and calling convention.
    pub unsafe fn sym<T: Copy>(&self, name: &CStr) -> Option<T> {
        const { assert!(size_of::<T>() == size_of::<usize>()) };
        // SAFETY: `name` is NUL-terminated; `self.0` stays loaded while `self` lives.
        let f = unsafe { GetProcAddress(self.0, PCSTR(name.as_ptr().cast())) }?;
        // SAFETY: same size (asserted above); the signature is the caller's contract.
        Some(unsafe { std::mem::transmute_copy(&f) })
    }
}

impl Drop for Dll {
    fn drop(&mut self) {
        // SAFETY: we own the module reference taken by `load`.
        unsafe { _ = FreeLibrary(self.0) };
    }
}
