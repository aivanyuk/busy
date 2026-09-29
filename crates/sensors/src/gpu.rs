//! GPU adapters (DXGI), utilization/memory/per-process (PDH "GPU Engine"/"GPU Adapter Memory"),
//! generic temperature/fan (D3DKMT ADAPTERPERFDATA, what Task Manager shows) and NVML/ADL extras.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use busy_core::{GpuInfo, Module, ProcEntry, SensorKind, SensorReading, Snapshot, Source, TOP_N};
use busy_win::pdh::{ArrayBuf, Counter, PDH_FMT_NOCAP100, Query};
use windows::Wdk::Graphics::Direct3D::*;
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS};
use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIDevice, IDXGIFactory1};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{Interface, Owned, PWSTR};

use crate::{Shared, adl::Adl, dx, nvml::Nvml, reading};

const REENUM: Duration = Duration::from_secs(60);
/// Distinct engine type names kept (drivers report a handful: 3D, Copy, VideoDecode, …).
const MAX_ENGINE_TYPES: usize = 32;

/// Sensor-ish GPU values, from one backend.
#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub temp_c: Option<f32>,
    pub hotspot_c: Option<f32>,
    pub fan_rpm: Option<u32>,
    pub fan_pct: Option<f32>,
    pub power_w: Option<f32>,
    pub core_mhz: Option<u32>,
    pub mem_mhz: Option<u32>,
}

impl Stats {
    fn or(self, o: Stats) -> Stats {
        Stats {
            temp_c: self.temp_c.or(o.temp_c),
            hotspot_c: self.hotspot_c.or(o.hotspot_c),
            fan_rpm: self.fan_rpm.or(o.fan_rpm),
            fan_pct: self.fan_pct.or(o.fan_pct),
            power_w: self.power_w.or(o.power_w),
            core_mhz: self.core_mhz.or(o.core_mhz),
            mem_mhz: self.mem_mhz.or(o.mem_mhz),
        }
    }

    fn readings(&self, src: &str, hw: &str, out: &mut Vec<SensorReading>) {
        let f = [
            ("GPU Core", SensorKind::Temperature, self.temp_c),
            ("GPU Hot Spot", SensorKind::Temperature, self.hotspot_c),
            ("GPU Fan", SensorKind::Fan, self.fan_rpm.map(|v| v as f32)),
            ("GPU Fan", SensorKind::Load, self.fan_pct),
            ("GPU Power", SensorKind::Power, self.power_w),
            ("GPU Core", SensorKind::Clock, self.core_mhz.map(|v| v as f32)),
            ("GPU Memory", SensorKind::Clock, self.mem_mhz.map(|v| v as f32)),
        ];
        out.extend(f.into_iter().filter_map(|(n, k, v)| Some(reading(src, hw, n, k, v?))));
    }

    fn apply(&self, g: &mut GpuInfo) {
        g.temp_c = self.temp_c;
        g.hotspot_c = self.hotspot_c;
        g.fan_rpm = self.fan_rpm;
        g.fan_pct = self.fan_pct;
        g.power_w = self.power_w;
        g.core_clock_mhz = self.core_mhz;
        g.mem_clock_mhz = self.mem_mhz;
    }
}

struct Kmt(u32);

impl Kmt {
    fn open(luid: LUID) -> Option<Self> {
        let mut o = D3DKMT_OPENADAPTERFROMLUID { AdapterLuid: luid, hAdapter: 0 };
        (unsafe { D3DKMTOpenAdapterFromLuid(&mut o) }.0 >= 0).then_some(Kmt(o.hAdapter))
    }

    fn query<T>(&self, ty: KMTQUERYADAPTERINFOTYPE, v: &mut T) -> bool {
        let mut q = D3DKMT_QUERYADAPTERINFO {
            hAdapter: self.0,
            Type: ty,
            pPrivateDriverData: (v as *mut T).cast(),
            PrivateDriverDataSize: size_of::<T>() as u32,
        };
        unsafe { D3DKMTQueryAdapterInfo(&mut q) }.0 >= 0
    }
}

impl Drop for Kmt {
    fn drop(&mut self) {
        unsafe { _ = D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: self.0 }) };
    }
}

enum Vendor {
    None,
    Nvml(*mut std::ffi::c_void),
    Adl(i32),
}

struct Adapter {
    name: String,
    luid: u64,
    vram_total: u64,
    vendor_id: u32,
    ids: u32,
    subsys: u32,
    pci: Option<(u32, u32)>,
    kmt: Option<Kmt>,
    kmt_caps: D3DKMT_ADAPTER_PERFDATACAPS,
    vendor: Vendor,
    driver_version: Option<String>,
    feature_level: Option<(u8, u8)>,
}

impl Adapter {
    fn kmt_stats(&self) -> Stats {
        let mut p = D3DKMT_ADAPTER_PERFDATA::default();
        if !self.kmt.as_ref().is_some_and(|k| k.query(KMTQAITYPE_ADAPTERPERFDATA, &mut p)) {
            return Stats::default();
        }
        let c = &self.kmt_caps;
        Stats {
            // Deci-°C.
            temp_c: (c.TemperatureMax > 0 || p.Temperature > 0).then_some(p.Temperature as f32 / 10.0),
            fan_rpm: (c.MaxFanRPM > 0).then_some(p.FanRPM),
            mem_mhz: (p.MemoryFrequency > 0).then_some((p.MemoryFrequency / 1_000_000) as u32),
            ..Default::default()
        }
    }
}

fn enumerate() -> Vec<Adapter> {
    let Ok(f) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else { return Vec::new() };
    let dx_cache = dx::cached_adapters();
    let mut out: Vec<Adapter> = Vec::new();
    for i in 0.. {
        let Ok(a) = (unsafe { f.EnumAdapters1(i) }) else { break };
        let Ok(d) = (unsafe { a.GetDesc1() }) else { continue };
        // Skip Microsoft Basic Render Driver and other software adapters.
        if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 || d.VendorId == 0x1414 {
            continue;
        }
        let luid = (d.AdapterLuid.HighPart as u32 as u64) << 32 | d.AdapterLuid.LowPart as u64;
        if out.iter().any(|a| a.luid == luid) {
            continue;
        }
        let kmt = Kmt::open(d.AdapterLuid);
        let mut addr = D3DKMT_ADAPTERADDRESS::default();
        let mut kmt_caps = D3DKMT_ADAPTER_PERFDATACAPS::default();
        let pci = kmt
            .as_ref()
            .filter(|k| k.query(KMTQAITYPE_ADAPTERADDRESS, &mut addr))
            .map(|_| (addr.BusNumber, addr.DeviceNumber));
        if let Some(k) = &kmt {
            k.query(KMTQAITYPE_ADAPTERPERFDATA_CAPS, &mut kmt_caps);
        }
        let len = d.Description.iter().position(|&c| c == 0).unwrap_or(d.Description.len());
        out.push(Adapter {
            name: String::from_utf16_lossy(&d.Description[..len]).trim().to_owned(),
            luid,
            vram_total: d.DedicatedVideoMemory as u64,
            vendor_id: d.VendorId,
            ids: d.DeviceId << 16 | d.VendorId,
            subsys: d.SubSysId,
            pci,
            kmt,
            kmt_caps,
            vendor: Vendor::None,
            // ~0.5 ms per adapter; answered by the kernel driver without loading the user-mode driver.
            // SAFETY: the IID is a valid GUID that outlives the call; `a` is a live adapter.
            driver_version: unsafe { a.CheckInterfaceSupport(&IDXGIDevice::IID) }.ok().map(dx::driver_version),
            feature_level: dx::level_for(&dx_cache, luid, (d.VendorId, d.DeviceId)),
        });
    }
    out
}

struct Pdh {
    q: Query,
    engine: Counter,
    dedicated: Counter,
    shared: Counter,
}

impl Pdh {
    fn open() -> Option<Self> {
        let q = Query::open()?;
        let p = Self {
            engine: q.add(r"\GPU Engine(*)\Utilization Percentage")?,
            dedicated: q.add(r"\GPU Adapter Memory(*)\Dedicated Usage")?,
            shared: q.add(r"\GPU Adapter Memory(*)\Shared Usage")?,
            q,
        };
        // Prime the rate counter.
        p.q.collect().then_some(p)
    }
}

/// Digits/hex run following `key`.
fn field<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let rest = &s[s.find(key)? + key.len()..];
    Some(&rest[..rest.find('_').unwrap_or(rest.len())])
}

/// `luid_0xHHHHHHHH_0xLLLLLLLL` -> packed u64.
fn parse_luid(s: &str) -> Option<u64> {
    let hi = u32::from_str_radix(field(s, "luid_0x")?, 16).ok()?;
    let rest = &s[s.find("luid_0x")? + 7..];
    let lo = u32::from_str_radix(field(rest, "_0x")?, 16).ok()?;
    Some((hi as u64) << 32 | lo as u64)
}

fn proc_name(pid: u32) -> String {
    if pid == 4 {
        return "System".into();
    }
    let mut buf = [0u16; 512];
    let mut n = buf.len() as u32;
    // SAFETY: on success we own the returned handle; `Owned` closes it.
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.map(|h| unsafe { Owned::new(h) });
    let ok = h.is_ok_and(|h| {
        // SAFETY: `buf` holds `n` writable u16s, and `n` tells the API so.
        unsafe { QueryFullProcessImageNameW(*h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n) }.is_ok()
    }) || nt_image_name(pid, &mut buf, &mut n);
    if !ok {
        return format!("pid {pid}");
    }
    let path = String::from_utf16_lossy(&buf[..n as usize]);
    path.rsplit('\\').next().unwrap_or(&path).to_owned()
}

/// SystemProcessIdInformation (class 88): image path by pid without a process handle
/// (works for processes unelevated OpenProcess can't open, e.g. dwm.exe).
fn nt_image_name(pid: u32, buf: &mut [u16], n: &mut u32) -> bool {
    #[repr(C)]
    struct Info {
        pid: usize,
        len: u16,
        max: u16,
        buf: *mut u16,
    }
    let mut info = Info { pid: pid as usize, len: 0, max: (buf.len() * 2) as u16, buf: buf.as_mut_ptr() };
    let st = unsafe {
        NtQuerySystemInformation(
            SYSTEM_INFORMATION_CLASS(88),
            (&raw mut info).cast(),
            size_of::<Info>() as u32,
            std::ptr::null_mut(),
        )
    };
    *n = info.len as u32 / 2;
    st.0 >= 0 && *n > 0
}

pub struct GpuSource {
    shared: Rc<RefCell<Shared>>,
    adapters: Vec<Adapter>,
    enum_at: Option<Instant>,
    pdh: Option<Pdh>,
    nvml: Option<Nvml>,
    adl: Option<Adl>,
    names: HashMap<u32, String>,
    buf: ArrayBuf,
    scratch: Scratch,
}

/// Per-sample containers, cleared and refilled every tick so their allocations are reused.
#[derive(Default)]
struct Scratch {
    /// (luid, eng) -> (sum over pids, index into `types`, `usize::MAX` for none)
    engines: HashMap<(u64, u32), (f64, usize)>,
    /// (pid, luid, eng) -> sum
    procs: HashMap<(u32, u64, u32), f64>,
    dedicated: HashMap<u64, u64>,
    shared: HashMap<u64, u64>,
    per_pid: HashMap<u32, f64>,
    top: Vec<(u32, f64)>,
    /// Engine type names seen so far; never cleared, at most `MAX_ENGINE_TYPES`.
    types: Vec<String>,
}

impl Scratch {
    fn clear(&mut self) {
        self.engines.clear();
        self.procs.clear();
        self.dedicated.clear();
        self.shared.clear();
        self.per_pid.clear();
        self.top.clear();
    }
}

/// Index of engine type `ty` in `types`, adding it while there is room.
fn engine_type(types: &mut Vec<String>, ty: &str) -> usize {
    match types.iter().position(|t| t == ty) {
        Some(i) => i,
        None if types.len() < MAX_ENGINE_TYPES => {
            types.push(ty.to_owned());
            types.len() - 1
        }
        None => usize::MAX,
    }
}

impl GpuSource {
    pub(crate) fn new(shared: Rc<RefCell<Shared>>) -> Self {
        Self {
            shared,
            adapters: Vec::new(),
            enum_at: None,
            pdh: None,
            nvml: None,
            adl: None,
            names: HashMap::new(),
            buf: ArrayBuf::default(),
            scratch: Scratch::default(),
        }
    }

    fn refresh(&mut self) {
        self.enum_at = Some(Instant::now());
        self.adapters = enumerate();
        if self.pdh.is_none() {
            self.pdh = Pdh::open();
        }
        let has = |v| self.adapters.iter().any(|a| a.vendor_id == v);
        if self.nvml.is_none() && has(0x10DE) {
            self.nvml = Nvml::load();
        }
        if self.adl.is_none() && has(0x1002) {
            self.adl = Adl::load();
        }
        // Bind vendor devices: PCI bus/device first, else ids + occurrence order.
        let mut used = HashSet::new();
        for a in &mut self.adapters {
            if let Some(n) = &self.nvml {
                let pick = n.devices.iter().position(|d| a.pci == Some((d.bus, d.dev))).or_else(|| {
                    (0..n.devices.len())
                        .find(|i| !used.contains(i) && n.devices[*i].ids == a.ids && n.devices[*i].subsys == a.subsys)
                });
                if let Some(i) = pick {
                    used.insert(i);
                    a.vendor = Vendor::Nvml(n.devices[i].handle);
                }
            }
            if let (Some(adl), Some(pci)) = (&self.adl, a.pci)
                && let Some(x) = adl.adapters.iter().find(|x| (x.1, x.2) == pci)
            {
                a.vendor = Vendor::Adl(x.0);
            }
        }
    }
}

impl Source for GpuSource {
    fn module(&self) -> Module {
        Module::Gpu
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        if self.enum_at.is_none_or(|t| t.elapsed() >= REENUM) {
            self.refresh();
        }

        self.scratch.clear();
        let Scratch { engines, procs, dedicated, shared: shared_mem, per_pid, top, types } = &mut self.scratch;
        if let Some(p) = &self.pdh
            && p.q.collect()
        {
            p.engine.each_double(PDH_FMT_NOCAP100, &mut self.buf, |name, v| {
                let (Some(luid), Some(eng), Some(pid)) = (
                    parse_luid(name),
                    field(name, "_eng_").and_then(|s| s.parse().ok()),
                    field(name, "pid_").and_then(|s| s.parse().ok()),
                ) else {
                    return;
                };
                let e = engines.entry((luid, eng)).or_insert_with(|| {
                    let ty = name.find("_engtype_").map_or("", |i| &name[i + 9..]);
                    (0.0, engine_type(types, ty))
                });
                e.0 += v;
                *procs.entry((pid, luid, eng)).or_default() += v;
            });
            for (c, map) in [(p.dedicated, &mut *dedicated), (p.shared, &mut *shared_mem)] {
                c.each_large(&mut self.buf, |name, v| {
                    if let Some(luid) = parse_luid(name) {
                        *map.entry(luid).or_default() += v.max(0) as u64;
                    }
                });
            }
        }

        let mut vendor_readings = Vec::new();
        snap.gpus = self
            .adapters
            .iter()
            .map(|a| {
                let mut g = GpuInfo {
                    name: a.name.clone(),
                    luid: a.luid,
                    vram_total: a.vram_total,
                    vram_used: dedicated.get(&a.luid).copied().unwrap_or(0),
                    shared_used: shared_mem.get(&a.luid).copied().unwrap_or(0),
                    driver_version: a.driver_version.clone(),
                    feature_level: a.feature_level,
                    ..Default::default()
                };
                let mut by_type: Vec<(String, f32)> = Vec::new();
                for ((_, _), (sum, t)) in engines.iter().filter(|((l, _), _)| *l == a.luid) {
                    let v = sum.clamp(0.0, 100.0) as f32;
                    g.util_pct = g.util_pct.max(v);
                    let Some(ty) = types.get(*t).filter(|ty| !ty.is_empty()) else { continue };
                    match by_type.iter_mut().find(|(t, _)| t == ty) {
                        Some(t) => t.1 = t.1.max(v),
                        None => by_type.push((ty.clone(), v)),
                    }
                }
                by_type.sort_by(|x, y| x.0.cmp(&y.0));
                g.engines = by_type;

                let kmt = a.kmt_stats();
                let (src, vs) = match (&a.vendor, &self.nvml, &self.adl) {
                    (Vendor::Nvml(d), Some(n), _) => ("NVML", n.stats(*d)),
                    (Vendor::Adl(i), _, Some(adl)) => ("ADL", adl.stats(*i)),
                    _ => ("", Stats::default()),
                };
                if !src.is_empty() {
                    vs.readings(src, &a.name, &mut vendor_readings);
                }
                // WDDM values not already covered by the vendor library.
                let rest = Stats {
                    temp_c: kmt.temp_c.filter(|_| vs.temp_c.is_none()),
                    fan_rpm: kmt.fan_rpm.filter(|_| vs.fan_rpm.is_none()),
                    mem_mhz: kmt.mem_mhz.filter(|_| vs.mem_mhz.is_none()),
                    ..Default::default()
                };
                rest.readings("WDDM", &a.name, &mut vendor_readings);
                vs.or(kmt).apply(&mut g);
                g
            })
            .collect();
        {
            let mut sh = self.shared.borrow_mut();
            sh.vendor = vendor_readings;
            sh.at = Some(Instant::now());
        }

        for (&(pid, _, _), &v) in procs.iter() {
            let e = per_pid.entry(pid).or_default();
            *e = e.max(v);
        }
        self.names.retain(|pid, _| per_pid.contains_key(pid));
        top.extend(per_pid.iter().map(|(&pid, &v)| (pid, v)).filter(|&(pid, v)| pid != 0 && v >= 0.05));
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        top.truncate(TOP_N);
        snap.top.by_gpu = top
            .iter()
            .map(|&(pid, v)| ProcEntry {
                pid,
                name: self.names.entry(pid).or_insert_with(|| proc_name(pid)).clone(),
                gpu_pct: v.clamp(0.0, 100.0) as f32,
                ..Default::default()
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_names() {
        let s = "pid_1234_luid_0x00000001_0x0000D1B5_phys_0_eng_3_engtype_VideoDecode";
        assert_eq!(parse_luid(s), Some(1 << 32 | 0xD1B5));
        assert_eq!(field(s, "pid_"), Some("1234"));
        assert_eq!(field(s, "_eng_"), Some("3"));
        assert_eq!(parse_luid("luid_0x00000000_0x00012345_phys_0"), Some(0x12345));
    }
}
