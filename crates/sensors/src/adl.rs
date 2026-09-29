//! AMD ADL (atiadlxx.dll, System32). Overdrive8 PMLog (RX 5000+) with OverdriveN temperature fallback.
//! Untested on real AMD hardware; any failure leaves fields `None`.

use std::ffi::c_void;

use windows::Win32::System::Com::CoTaskMemAlloc;
use windows::core::w;

use crate::gpu::Stats;
use busy_win::Dll;

type Ctx = *mut c_void;

/// AdapterInfo (adl_structures.h, Windows layout).
#[repr(C)]
struct AdapterInfo {
    size: i32,
    adapter_index: i32,
    udid: [u8; 256],
    bus: i32,
    device: i32,
    function: i32,
    vendor_id: i32,
    adapter_name: [u8; 256],
    display_name: [u8; 256],
    present: i32,
    exist: i32,
    driver_path: [u8; 256],
    driver_path_ext: [u8; 256],
    pnp_string: [u8; 256],
    os_display_index: i32,
}

/// ADLPMLogDataOutput: size + ADL_PMLOG_MAX_SENSORS × {supported, value}.
#[repr(C)]
struct PmLog {
    size: i32,
    sensors: [[i32; 2]; 256],
}

// ADL_PMLOG_SENSORS indices.
const CLK_GFX: usize = 1;
const CLK_MEM: usize = 2;
const TEMP_EDGE: usize = 8;
const FAN_RPM: usize = 14;
const FAN_PCT: usize = 15;
const ASIC_POWER: usize = 23;
const TEMP_HOTSPOT: usize = 27;
const TEMP_GFX: usize = 28;

type PmLogGet = unsafe extern "system" fn(Ctx, i32, *mut PmLog) -> i32;
type OdnTempGet = unsafe extern "system" fn(Ctx, i32, i32, *mut i32) -> i32;

unsafe extern "system" fn adl_alloc(size: i32) -> *mut c_void {
    unsafe { CoTaskMemAlloc(size.max(0) as usize) }
}

pub struct Adl {
    ctx: Ctx,
    destroy: unsafe extern "system" fn(Ctx) -> i32,
    pmlog: Option<PmLogGet>,
    odn_temp: Option<OdnTempGet>,
    /// (adapter index, bus, device)
    pub adapters: Vec<(i32, u32, u32)>,
    _dll: Dll,
}

impl Adl {
    pub fn load() -> Option<Self> {
        let dll = Dll::load(w!("atiadlxx.dll"))?;
        unsafe {
            type Alloc = unsafe extern "system" fn(i32) -> *mut c_void;
            let create: unsafe extern "system" fn(Alloc, i32, *mut Ctx) -> i32 =
                dll.sym(c"ADL2_Main_Control_Create")?;
            let destroy = dll.sym(c"ADL2_Main_Control_Destroy")?;
            let count: unsafe extern "system" fn(Ctx, *mut i32) -> i32 =
                dll.sym(c"ADL2_Adapter_NumberOfAdapters_Get")?;
            let info: unsafe extern "system" fn(Ctx, *mut AdapterInfo, i32) -> i32 =
                dll.sym(c"ADL2_Adapter_AdapterInfo_Get")?;
            let mut ctx = std::ptr::null_mut();
            if create(adl_alloc, 1, &mut ctx) != 0 || ctx.is_null() {
                return None;
            }
            let mut adl = Self {
                ctx,
                destroy,
                pmlog: dll.sym(c"ADL2_New_QueryPMLogData_Get"),
                odn_temp: dll.sym(c"ADL2_OverdriveN_Temperature_Get"),
                adapters: Vec::new(),
                _dll: dll,
            };
            let mut n = 0;
            if count(ctx, &mut n) != 0 || !(1..=64).contains(&n) {
                return Some(adl);
            }
            let mut buf: Vec<AdapterInfo> = (0..n).map(|_| std::mem::zeroed()).collect();
            if info(ctx, buf.as_mut_ptr(), n * size_of::<AdapterInfo>() as i32) != 0 {
                return Some(adl);
            }
            // One entry per display output; dedupe by PCI location.
            for a in buf.iter().filter(|a| a.vendor_id == 0x1002 && a.udid[0] != 0) {
                let loc = (a.bus as u32, a.device as u32);
                if !adl.adapters.iter().any(|x| (x.1, x.2) == loc) {
                    adl.adapters.push((a.adapter_index, loc.0, loc.1));
                }
            }
            Some(adl)
        }
    }

    pub fn stats(&self, idx: i32) -> Stats {
        let mut s = Stats::default();
        unsafe {
            if let Some(f) = self.pmlog {
                let mut log: PmLog = std::mem::zeroed();
                if f(self.ctx, idx, &mut log) == 0 {
                    let get = |i: usize| (log.sensors[i][0] != 0).then_some(log.sensors[i][1]);
                    s.temp_c = get(TEMP_EDGE).or_else(|| get(TEMP_GFX)).map(|v| v as f32);
                    s.hotspot_c = get(TEMP_HOTSPOT).map(|v| v as f32);
                    s.fan_rpm = get(FAN_RPM).map(|v| v as u32);
                    s.fan_pct = get(FAN_PCT).map(|v| v as f32);
                    s.power_w = get(ASIC_POWER).map(|v| v as f32);
                    s.core_mhz = get(CLK_GFX).map(|v| v as u32);
                    s.mem_mhz = get(CLK_MEM).map(|v| v as u32);
                }
            }
            if s.temp_c.is_none()
                && let Some(f) = self.odn_temp
            {
                // Type 1 = edge; millidegrees.
                let mut v = 0;
                if f(self.ctx, idx, 1, &mut v) == 0 && v > 0 {
                    s.temp_c = Some(v as f32 / 1000.0);
                }
            }
        }
        s
    }
}

impl Drop for Adl {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.ctx) };
    }
}
