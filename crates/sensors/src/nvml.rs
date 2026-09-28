//! NVIDIA NVML (nvml.dll, installed into System32 by the driver).
//! Adapters are matched to DXGI by PCI bus/device (DXGI side via D3DKMT ADAPTERADDRESS),
//! falling back to vendor/device/subsystem id + occurrence order.

use std::ffi::c_void;

use windows::core::w;

use crate::{Dll, gpu::Stats};

type Dev = *mut c_void;
type Ret = i32;

/// nvmlPciInfo_t (nvml.h, v2/v3 layout).
#[repr(C)]
struct PciInfo {
    bus_id_legacy: [u8; 16],
    domain: u32,
    bus: u32,
    device: u32,
    pci_device_id: u32,
    pci_subsystem_id: u32,
    bus_id: [u8; 32],
}

const TEMPERATURE_GPU: u32 = 0;
const CLOCK_GRAPHICS: u32 = 0;
const CLOCK_MEM: u32 = 2;

pub struct Nvml {
    shutdown: unsafe extern "C" fn() -> Ret,
    temp: Option<unsafe extern "C" fn(Dev, u32, *mut u32) -> Ret>,
    fan: Option<unsafe extern "C" fn(Dev, *mut u32) -> Ret>,
    power: Option<unsafe extern "C" fn(Dev, *mut u32) -> Ret>,
    clock: Option<unsafe extern "C" fn(Dev, u32, *mut u32) -> Ret>,
    pub devices: Vec<Device>,
    _dll: Dll,
}

#[derive(Clone, Copy)]
pub struct Device {
    pub handle: Dev,
    pub bus: u32,
    pub dev: u32,
    /// device_id << 16 | vendor_id
    pub ids: u32,
    pub subsys: u32,
}

impl Nvml {
    pub fn load() -> Option<Self> {
        let dll = Dll::load(w!("nvml.dll"))?;
        unsafe {
            let init: unsafe extern "C" fn() -> Ret = dll.sym(c"nvmlInit_v2")?;
            let shutdown = dll.sym(c"nvmlShutdown")?;
            let count: unsafe extern "C" fn(*mut u32) -> Ret = dll.sym(c"nvmlDeviceGetCount_v2")?;
            let by_index: unsafe extern "C" fn(u32, *mut Dev) -> Ret = dll.sym(c"nvmlDeviceGetHandleByIndex_v2")?;
            let pci: unsafe extern "C" fn(Dev, *mut PciInfo) -> Ret =
                dll.sym(c"nvmlDeviceGetPciInfo_v3").or_else(|| dll.sym(c"nvmlDeviceGetPciInfo_v2"))?;
            if init() != 0 {
                return None;
            }
            let mut n = 0;
            if count(&mut n) != 0 {
                n = 0;
            }
            let devices = (0..n)
                .filter_map(|i| {
                    let mut handle = std::ptr::null_mut();
                    let mut p: PciInfo = std::mem::zeroed();
                    (by_index(i, &mut handle) == 0 && pci(handle, &mut p) == 0).then_some(Device {
                        handle,
                        bus: p.bus,
                        dev: p.device,
                        ids: p.pci_device_id,
                        subsys: p.pci_subsystem_id,
                    })
                })
                .collect();
            Some(Self {
                shutdown,
                temp: dll.sym(c"nvmlDeviceGetTemperature"),
                fan: dll.sym(c"nvmlDeviceGetFanSpeed"),
                power: dll.sym(c"nvmlDeviceGetPowerUsage"),
                clock: dll.sym(c"nvmlDeviceGetClockInfo"),
                devices,
                _dll: dll,
            })
        }
    }

    pub fn stats(&self, d: Dev) -> Stats {
        let get = |f: &dyn Fn(*mut u32) -> Ret| {
            let mut v = 0;
            (f(&mut v) == 0).then_some(v)
        };
        unsafe {
            Stats {
                temp_c: self.temp.and_then(|f| get(&|v| f(d, TEMPERATURE_GPU, v))).map(|v| v as f32),
                fan_pct: self.fan.and_then(|f| get(&|v| f(d, v))).map(|v| v as f32),
                power_w: self.power.and_then(|f| get(&|v| f(d, v))).map(|v| v as f32 / 1000.0),
                core_mhz: self.clock.and_then(|f| get(&|v| f(d, CLOCK_GRAPHICS, v))),
                mem_mhz: self.clock.and_then(|f| get(&|v| f(d, CLOCK_MEM, v))),
                ..Default::default()
            }
        }
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        unsafe { (self.shutdown)() };
    }
}
