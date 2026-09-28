use crate::pdh::{Counter, NOCAP100, Query};
use crate::util::{perf_info, reg_dword, reg_string};
use busy_core::{CpuInfo, Module, Snapshot, Source};
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessorPerformanceInformation};
use windows::Win32::System::Power::{CallNtPowerInformation, PROCESSOR_POWER_INFORMATION, ProcessorInformation};
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, GetTickCount64, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows::Win32::System::Threading::{ALL_PROCESSOR_GROUPS, GetActiveProcessorCount};
use windows::Win32::System::WindowsProgramming::SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION as Sppi;

const CPU_KEY: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";

struct Load {
    total: f32,
    user: f32,
    kernel: f32,
    per_core: Vec<f32>,
    perf_pct: Option<f64>,
}

pub struct Cpu {
    name: String,
    logical: u32,
    physical: u32,
    base_mhz: Option<u32>,
    pdh: Option<Pdh>,
    prev: Vec<Sppi>,
}

impl Cpu {
    pub fn new() -> Self {
        let logical = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
        let mut s = Self {
            name: reg_string(CPU_KEY, "ProcessorNameString").unwrap_or_else(|| "CPU".into()),
            logical,
            physical: physical_cores(),
            base_mhz: reg_dword(CPU_KEY, "~MHz").filter(|&m| m > 0).or_else(|| power_max_mhz(logical)),
            pdh: Pdh::open(),
            prev: Vec::new(),
        };
        if s.pdh.is_none() {
            nt_times(&mut s.prev);
        }
        s
    }

    fn nt_load(&mut self) -> Option<Load> {
        let mut cur = Vec::new();
        if !nt_times(&mut cur) {
            return None;
        }
        let prev = std::mem::replace(&mut self.prev, cur);
        if prev.len() != self.prev.len() {
            return None;
        }
        // KernelTime includes IdleTime.
        let (mut idle, mut kern, mut user) = (0i64, 0i64, 0i64);
        let per_core = prev
            .iter()
            .zip(&self.prev)
            .map(|(a, b)| {
                let (di, dk, du) = (b.IdleTime - a.IdleTime, b.KernelTime - a.KernelTime, b.UserTime - a.UserTime);
                (idle, kern, user) = (idle + di, kern + dk, user + du);
                pct((dk + du - di) as f64, (dk + du) as f64)
            })
            .collect();
        let all = (kern + user) as f64;
        Some(Load {
            total: pct((kern + user - idle) as f64, all),
            user: pct(user as f64, all),
            kernel: pct((kern - idle) as f64, all),
            per_core,
            perf_pct: None,
        })
    }
}

fn pct(part: f64, whole: f64) -> f32 {
    if whole > 0.0 { (part / whole * 100.0).clamp(0.0, 100.0) as f32 } else { 0.0 }
}

impl Source for Cpu {
    fn module(&self) -> Module {
        Module::Cpu
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        let Some(load) = self.pdh.as_ref().and_then(Pdh::read).or_else(|| self.nt_load()) else { return };
        let pi = perf_info().unwrap_or_default();
        snap.cpu = Some(CpuInfo {
            name: self.name.clone(),
            total: load.total,
            user: load.user,
            kernel: load.kernel,
            per_core: load.per_core,
            freq_mhz: load.perf_pct.zip(self.base_mhz).map(|(p, b)| (p * b as f64 / 100.0).round() as u32),
            logical_cores: self.logical,
            physical_cores: self.physical,
            processes: pi.ProcessCount,
            threads: pi.ThreadCount,
            handles: pi.HandleCount,
            uptime_secs: unsafe { GetTickCount64() } / 1000,
        });
    }
}

struct Pdh {
    q: Query,
    cores: Counter,
    total: Counter,
    kernel: Counter,
    perf: Option<Counter>,
}

impl Pdh {
    fn open() -> Option<Self> {
        let q = Query::open()?;
        let add = |c: &str| q.add(&format!(r"\Processor Information({c}"));
        let s = Self {
            cores: add(r"*)\% Processor Utility")?,
            total: add(r"_Total)\% Processor Utility")?,
            kernel: add(r"_Total)\% Privileged Utility")?,
            perf: add(r"_Total)\% Processor Performance"),
            q,
        };
        s.q.collect().then_some(s)
    }

    fn read(&self) -> Option<Load> {
        if !self.q.collect() {
            return None;
        }
        // Utility counters exceed 100 under turbo: clamp the total like Task Manager, split user/kernel by raw ratio.
        let raw = self.total.value(NOCAP100)?.max(0.0);
        let total = raw.min(100.0);
        let share = if raw > 0.0 { (self.kernel.value(NOCAP100).unwrap_or(0.0) / raw).clamp(0.0, 1.0) } else { 0.0 };
        let (total, kernel) = (total as f32, (total * share) as f32);
        // Instances are "group,index" plus "g,_Total"/"_Total"; the array re-expands as cores come and go.
        let mut cores: Vec<_> = self
            .cores
            .array(0)?
            .into_iter()
            .filter_map(|(n, v)| {
                let (g, i) = n.split_once(',')?;
                Some(((g.parse::<u16>().ok()?, i.parse::<u16>().ok()?), v.clamp(0.0, 100.0) as f32))
            })
            .collect();
        cores.sort_unstable_by_key(|c| c.0);
        Some(Load {
            total,
            user: total - kernel,
            kernel,
            per_core: cores.into_iter().map(|c| c.1).collect(),
            perf_pct: self.perf.and_then(|c| c.value(NOCAP100)),
        })
    }
}

/// Fallback: per-CPU times for the calling thread's processor group.
fn nt_times(buf: &mut Vec<Sppi>) -> bool {
    buf.resize(64, Sppi::default());
    let mut len = 0u32;
    let size = (buf.len() * size_of::<Sppi>()) as u32;
    let ok = unsafe {
        NtQuerySystemInformation(SystemProcessorPerformanceInformation, buf.as_mut_ptr().cast(), size, &mut len)
    }
    .is_ok();
    buf.truncate(if ok { len as usize / size_of::<Sppi>() } else { 0 });
    ok
}

fn physical_cores() -> u32 {
    let mut len = 0u32;
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len) };
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    let p = buf.as_mut_ptr().cast::<u8>();
    if unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, Some(p.cast()), &mut len) }.is_err() {
        return 0;
    }
    let (mut off, mut n) = (0usize, 0u32);
    while off < len as usize {
        // Records are variable-length; read only the Size header.
        let rec = unsafe { p.add(off) }.cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>();
        let size = unsafe { std::ptr::addr_of!((*rec).Size).read_unaligned() } as usize;
        if size == 0 {
            break;
        }
        off += size;
        n += 1;
    }
    n
}

fn power_max_mhz(logical: u32) -> Option<u32> {
    let mut v = vec![PROCESSOR_POWER_INFORMATION::default(); logical.max(1) as usize];
    let size = (v.len() * size_of::<PROCESSOR_POWER_INFORMATION>()) as u32;
    let st = unsafe { CallNtPowerInformation(ProcessorInformation, None, 0, Some(v.as_mut_ptr().cast()), size) };
    st.is_ok().then(|| v[0].MaxMhz).filter(|&m| m > 0)
}
