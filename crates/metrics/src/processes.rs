use crate::conns::Connections;
use crate::etw::Trace;
use crate::util::{Clock, Every};
use busy_core::{Module, NetRank, ProcEntry, Snapshot, Source, SourceOptions, TOP_N};
use std::collections::HashMap;
use std::ops::Range;
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, STATUS_INFO_LENGTH_MISMATCH};
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
    /// This sample's counters; swapped with `prev` afterwards so both allocations are reused.
    cur: HashMap<(usize, i64), (i64, i64)>,
    rows: Vec<Row>,
    opts: SourceOptions,
    conns: Connections,
    /// Runs while the Network flyout lists processes with `process_network` on.
    trace: Option<Trace>,
    /// Starting it was refused: not again until the opt-in is turned off and on.
    denied: bool,
    /// Starting it failed otherwise (e.g. no free session): retried every 30 s.
    failed: bool,
    retry: Every,
    /// pid -> this sample's network use: bytes/s or connections, as `snap.top.net_rank` says.
    net: HashMap<u32, u64>,
}

impl Default for Processes {
    fn default() -> Self {
        Self {
            buf: vec![0; 1 << 17],
            clock: Clock::default(),
            ncpu: unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) }.max(1) as f64,
            prev: HashMap::new(),
            cur: HashMap::new(),
            rows: Vec::new(),
            opts: SourceOptions::default(),
            conns: Connections::default(),
            trace: None,
            denied: false,
            failed: false,
            retry: Every::default(),
            net: HashMap::new(),
        }
    }
}

struct Row {
    pid: u32,
    /// Image name, in u16 units of `buf` (the kernel points `ImageName.Buffer` into the same buffer).
    name: Range<usize>,
    cpu: f32,
    mem: u64,
    io: f64,
    /// Network use as `net` holds it; 0 unless the Network flyout lists processes.
    net: f64,
}

/// `buf` as UTF-16 units, for image names.
fn words(buf: &[u64]) -> &[u16] {
    // SAFETY: u64 storage is valid, and more than aligned, as four times as many u16s.
    unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u16>(), buf.len() * 4) }
}

/// Where an entry's image name lies in `buf`, in u16 units; empty unless it lies wholly inside.
fn name_range(buf: &[u64], ptr: *const u16, len_bytes: u16) -> Range<usize> {
    let start = (ptr as usize).wrapping_sub(buf.as_ptr() as usize);
    let end = start.saturating_add(len_bytes as usize);
    if ptr.is_null() || !start.is_multiple_of(2) || end > buf.len() * 8 {
        return 0..0;
    }
    start / 2..end / 2
}

impl Processes {
    /// Fills `net` with each process's network use: traffic from the trace where it may run, else connections.
    fn network_use(&mut self) -> NetRank {
        let may_trace = self.opts.process_network && !self.denied && (!self.failed || self.retry.due(30));
        if may_trace && self.trace.is_none() {
            match Trace::start() {
                Ok(t) => (self.trace, self.failed) = (Some(t), false),
                Err(e) if e == ERROR_ACCESS_DENIED => self.denied = true,
                Err(_) => {
                    self.failed = true;
                    self.retry.arm();
                }
            }
        }
        match &mut self.trace {
            Some(t) => {
                t.drain(&mut self.net);
                NetRank::Traffic
            }
            None => {
                self.conns.count(&mut self.net);
                NetRank::Connections
            }
        }
    }

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

    fn configure(&mut self, opts: SourceOptions) {
        // The session is machine-wide and costs a callback per transfer: it runs only while it is shown.
        if !(opts.network_processes && opts.process_network) {
            self.trace = None;
        }
        if !opts.process_network {
            (self.denied, self.failed) = (false, false);
        }
        self.opts = opts;
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        if !self.query() {
            return;
        }
        let dt = self.clock.tick();
        self.net.clear();
        let rank = if self.opts.network_processes { self.network_use() } else { NetRank::Connections };
        self.cur.clear();
        self.rows.clear();
        let base = self.buf.as_ptr().cast::<u8>();
        let end = self.buf.len() * 8;
        let mut off = 0usize;
        while off + size_of::<Spi>() <= end {
            let p = unsafe { &*base.add(off).cast::<Spi>() };
            if p.pid != 0 {
                let cpu = p.user_time + p.kernel_time;
                let io = p.read_xfer + p.write_xfer + p.other_xfer;
                let key = (p.pid, p.create_time);
                let (dcpu, dio) = self.prev.get(&key).map_or((0, 0), |&(c, i)| ((cpu - c).max(0), (io - i).max(0)));
                self.cur.insert(key, (cpu, io));
                self.rows.push(Row {
                    pid: p.pid as u32,
                    name: name_range(&self.buf, p.name_buf, p.name_len),
                    cpu: dt.map_or(0.0, |dt| (dcpu as f64 / (dt * 1e7 * self.ncpu) * 100.0).min(100.0) as f32),
                    mem: if p.ws_private > 0 { p.ws_private as u64 } else { p.ws as u64 },
                    io: dt.map_or(0.0, |dt| dio as f64 / dt),
                    net: self.net.get(&(p.pid as u32)).map_or(0.0, |&n| n as f64),
                });
            }
            if p.next == 0 {
                break;
            }
            off += p.next as usize;
        }
        std::mem::swap(&mut self.prev, &mut self.cur);
        let words = words(&self.buf);
        let rows = &mut self.rows;
        let top = |rows: &mut Vec<Row>, key: &dyn Fn(&Row) -> f64| -> Vec<ProcEntry> {
            rows.sort_unstable_by(|a, b| key(b).total_cmp(&key(a)));
            rows.iter()
                .take(TOP_N)
                .map(|r| ProcEntry {
                    pid: r.pid,
                    name: String::from_utf16_lossy(&words[r.name.clone()]),
                    cpu_pct: r.cpu,
                    mem_bytes: r.mem,
                    io_bps: r.io,
                    gpu_pct: 0.0,
                    net_bps: if rank == NetRank::Traffic { r.net } else { 0.0 },
                    connections: if rank == NetRank::Connections { r.net as u32 } else { 0 },
                })
                .collect()
        };
        snap.top.by_cpu = top(rows, &|r| r.cpu as f64);
        // Task Manager hides the compression store from its process list; it's shown under Memory instead.
        snap.top.by_mem = top(rows, &|r| {
            if words[r.name.clone()].iter().copied().eq("Memory Compression".encode_utf16()) {
                -1.0
            } else {
                r.mem as f64
            }
        });
        snap.top.by_disk = top(rows, &|r| r.io);
        if self.opts.network_processes {
            snap.top.by_net = top(rows, &|r| r.net);
            snap.top.by_net.retain(|p| p.net_bps > 0.0 || p.connections > 0);
            snap.top.net_rank = rank;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::name_range;

    #[test]
    fn name_range_stays_inside_the_buffer() {
        let buf = vec![0u64; 4];
        let at = |bytes: usize| buf.as_ptr().cast::<u8>().wrapping_add(bytes).cast::<u16>();
        assert_eq!(name_range(&buf, at(8), 6), 4..7);
        assert_eq!(name_range(&buf, at(26), 6), 13..16);
        assert_eq!(name_range(&buf, at(28), 6), 0..0, "runs past the end");
        assert_eq!(name_range(&buf, at(3), 2), 0..0, "odd offset");
        assert_eq!(name_range(&buf, std::ptr::null(), 2), 0..0);
        assert_eq!(name_range(&buf, at(8).wrapping_sub(16), 2), 0..0, "before the buffer");
    }
}
