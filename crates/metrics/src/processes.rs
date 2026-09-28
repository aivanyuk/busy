use crate::util::Clock;
use busy_core::{Module, ProcEntry, Snapshot, Source, TOP_N};
use std::collections::HashMap;
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
use windows::Win32::Foundation::STATUS_INFO_LENGTH_MISMATCH;
use windows::Win32::System::Threading::{ALL_PROCESSOR_GROUPS, GetActiveProcessorCount};

/// Full SYSTEM_PROCESS_INFORMATION (the `windows` crate only exposes the documented subset).
#[repr(C)]
struct Spi {
    next: u32,
    threads: u32,
    ws_private: i64,
    hard_faults: u32,
    threads_hwm: u32,
    cycle_time: u64,
    create_time: i64,
    user_time: i64,
    kernel_time: i64,
    name_len: u16,
    name_max: u16,
    name_buf: *const u16,
    base_priority: i32,
    pid: usize,
    parent_pid: usize,
    handles: u32,
    session: u32,
    key: usize,
    peak_vsize: usize,
    vsize: usize,
    page_faults: u32,
    peak_ws: usize,
    ws: usize,
    quota: [usize; 4],
    pagefile: usize,
    peak_pagefile: usize,
    private_pages: usize,
    ops: [i64; 3],
    read_xfer: i64,
    write_xfer: i64,
    other_xfer: i64,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<Spi>() == 256
        && std::mem::offset_of!(Spi, name_len) == 56
        && std::mem::offset_of!(Spi, pid) == 80
        && std::mem::offset_of!(Spi, ws) == 144
        && std::mem::offset_of!(Spi, read_xfer) == 232
);

pub struct Processes {
    buf: Vec<u64>,
    clock: Clock,
    ncpu: f64,
    /// (pid, create time) -> (cpu 100ns, io bytes)
    prev: HashMap<(usize, i64), (i64, i64)>,
}

impl Default for Processes {
    fn default() -> Self {
        Self {
            buf: vec![0; 1 << 17],
            clock: Clock::default(),
            ncpu: unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) }.max(1) as f64,
            prev: HashMap::new(),
        }
    }
}

struct Row<'a> {
    pid: u32,
    name: &'a [u16],
    cpu: f32,
    mem: u64,
    io: f64,
}

impl Processes {
    fn query(&mut self) -> bool {
        for _ in 0..4 {
            let mut len = 0u32;
            let size = (self.buf.len() * 8) as u32;
            let st = unsafe {
                NtQuerySystemInformation(SystemProcessInformation, self.buf.as_mut_ptr().cast(), size, &mut len)
            };
            if st == STATUS_INFO_LENGTH_MISMATCH {
                self.buf.resize((len as usize + (64 << 10)).div_ceil(8).max(self.buf.len() * 2), 0);
                continue;
            }
            return st.is_ok();
        }
        false
    }
}

impl Source for Processes {
    fn module(&self) -> Module {
        Module::Processes
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        if !self.query() {
            return;
        }
        let dt = self.clock.tick();
        let mut prev = std::mem::take(&mut self.prev);
        let mut rows = Vec::with_capacity(prev.len() + 16);
        let base = self.buf.as_ptr().cast::<u8>();
        let end = self.buf.len() * 8;
        let mut off = 0usize;
        while off + size_of::<Spi>() <= end {
            let p = unsafe { &*base.add(off).cast::<Spi>() };
            if p.pid != 0 {
                let cpu = p.user_time + p.kernel_time;
                let io = p.read_xfer + p.write_xfer + p.other_xfer;
                let key = (p.pid, p.create_time);
                let (dcpu, dio) = prev.remove(&key).map_or((0, 0), |(c, i)| ((cpu - c).max(0), (io - i).max(0)));
                self.prev.insert(key, (cpu, io));
                let name = if p.name_buf.is_null() {
                    &[][..]
                } else {
                    unsafe { std::slice::from_raw_parts(p.name_buf, p.name_len as usize / 2) }
                };
                rows.push(Row {
                    pid: p.pid as u32,
                    name,
                    cpu: dt.map_or(0.0, |dt| (dcpu as f64 / (dt * 1e7 * self.ncpu) * 100.0).min(100.0) as f32),
                    mem: if p.ws_private > 0 { p.ws_private as u64 } else { p.ws as u64 },
                    io: dt.map_or(0.0, |dt| dio as f64 / dt),
                });
            }
            if p.next == 0 {
                break;
            }
            off += p.next as usize;
        }
        let top = |rows: &mut Vec<Row>, key: fn(&Row) -> f64| -> Vec<ProcEntry> {
            rows.sort_unstable_by(|a, b| key(b).total_cmp(&key(a)));
            rows.iter()
                .take(TOP_N)
                .map(|r| ProcEntry {
                    pid: r.pid,
                    name: String::from_utf16_lossy(r.name),
                    cpu_pct: r.cpu,
                    mem_bytes: r.mem,
                    io_bps: r.io,
                    gpu_pct: 0.0,
                })
                .collect()
        };
        snap.top.by_cpu = top(&mut rows, |r| r.cpu as f64);
        // Task Manager hides the compression store from its process list; it's shown under Memory instead.
        snap.top.by_mem = top(&mut rows, |r| {
            if r.name.iter().copied().eq("Memory Compression".encode_utf16()) { -1.0 } else { r.mem as f64 }
        });
        snap.top.by_disk = top(&mut rows, |r| r.io);
    }
}
