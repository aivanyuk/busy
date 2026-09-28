//! GPU metrics (DXGI + PDH + D3DKMT + NVML/ADL) and hardware sensors (LibreHardwareMonitor).

mod adl;
mod gpu;
mod lhm;
mod nvml;

use std::cell::RefCell;
use std::ffi::CStr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use busy_core::{GpuInfo, Module, SensorKind, SensorReading, Snapshot, Source};
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
    lhm: lhm::Lhm,
}

impl SensorsSource {
    fn new(shared: Rc<RefCell<Shared>>) -> Self {
        Self { shared, lhm: lhm::Lhm::new() }
    }
}

impl Source for SensorsSource {
    fn module(&self) -> Module {
        Module::Sensors
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        // Third-party source preferred; vendor readings only without it (avoids duplicates).
        let mut out = self.lhm.read();
        if out.is_empty() {
            let sh = self.shared.borrow();
            if sh.at.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
                out = sh.vendor.clone();
            }
        } else {
            fill_gpus(&mut snap.gpus, &out);
        }
        snap.sensors.extend(out);
    }
}

fn fill_gpus(gpus: &mut [GpuInfo], rs: &[SensorReading]) {
    for g in gpus {
        let name = g.name.to_lowercase();
        let mine = || rs.iter().filter(|r| r.hardware.to_lowercase().contains(&name));
        let temps = || mine().filter(|r| r.kind == SensorKind::Temperature);
        let is_hot = |r: &&SensorReading| {
            let n = r.name.to_lowercase();
            n.contains("hot spot") || n.contains("hotspot")
        };
        if g.hotspot_c.is_none() {
            g.hotspot_c = temps().find(is_hot).map(|r| r.value);
        }
        if g.temp_c.is_none() {
            g.temp_c = temps()
                .filter(|r| !is_hot(r))
                .find(|r| {
                    let n = r.name.to_lowercase();
                    n.contains("core") || n.contains("gpu temperature") || n == "gpu"
                })
                .or_else(|| temps().find(|r| !is_hot(r)))
                .map(|r| r.value);
        }
        if g.fan_rpm.is_none() {
            g.fan_rpm = mine().find(|r| r.kind == SensorKind::Fan).map(|r| r.value as u32);
        }
    }
}
