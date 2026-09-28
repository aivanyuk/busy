//! GPU metrics (DXGI + PDH + D3DKMT + NVML/ADL).

mod adl;
mod gpu;
mod nvml;

use std::ffi::CStr;

use busy_core::Source;
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::core::{PCSTR, PCWSTR};

/// Called on the sampler thread after `CoInitializeEx(COINIT_MULTITHREADED)`.
pub fn sources() -> Vec<Box<dyn Source>> {
    vec![Box::new(gpu::GpuSource::new())]
}

/// A DLL loaded strictly from System32.
pub(crate) struct Dll(HMODULE);

impl Dll {
    pub fn load(name: PCWSTR) -> Option<Self> {
        unsafe { LoadLibraryExW(name, None, LOAD_LIBRARY_SEARCH_SYSTEM32) }.ok().map(Self)
    }

    /// # Safety
    /// `T` must be the `extern fn` type matching the export.
    pub unsafe fn sym<T: Copy>(&self, name: &CStr) -> Option<T> {
        debug_assert_eq!(size_of::<T>(), size_of::<usize>());
        unsafe { GetProcAddress(self.0, PCSTR(name.as_ptr().cast())) }.map(|f| unsafe { std::mem::transmute_copy(&f) })
    }
}

impl Drop for Dll {
    fn drop(&mut self) {
        unsafe { _ = FreeLibrary(self.0) };
    }
}
