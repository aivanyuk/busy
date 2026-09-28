//! GPU metrics (DXGI + PDH + D3DKMT + NVML/ADL) and hardware sensors.

mod adl;
mod gpu;
mod nvml;

use std::cell::RefCell;
use std::ffi::CStr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use busy_core::{Module, SensorKind, SensorReading, Snapshot, Source};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::core::{PCSTR, PCWSTR};

/// Called on the sampler thread after `CoInitializeEx(COINIT_MULTITHREADED)`.
pub fn sources() -> Vec<Box<dyn Source>> {
    let shared = Rc::new(RefCell::new(Shared::default()));
    vec![Box::new(gpu::GpuSource::new(shared.clone())), Box::new(SensorsSource::new(shared))]
}

/// Vendor GPU readings handed from the GPU source to the sensors source (same thread, same sample).
#[derive(Default)]
pub(crate) struct Shared {
    vendor: Vec<SensorReading>,
    at: Option<Instant>,
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

pub(crate) fn reading(source: &str, hardware: &str, name: &str, kind: SensorKind, value: f32) -> SensorReading {
    SensorReading { source: source.into(), hardware: hardware.into(), name: name.into(), kind, value }
}

struct SensorsSource {
    shared: Rc<RefCell<Shared>>,
}

impl SensorsSource {
    fn new(shared: Rc<RefCell<Shared>>) -> Self {
        Self { shared }
    }
}

impl Source for SensorsSource {
    fn module(&self) -> Module {
        Module::Sensors
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        let sh = self.shared.borrow();
        if sh.at.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
            snap.sensors.extend(sh.vendor.iter().cloned());
        }
    }
}
