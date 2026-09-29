//! Rolling metric history kept on the UI thread.

use crate::select;
use busy_core::{Config, Module, Snapshot};
use std::collections::VecDeque;

/// Which modules a snapshot refreshed, indexed by `Module::index`.
pub type Fresh = [bool; Module::ALL.len()];

#[derive(Default)]
pub struct Series {
    buf: VecDeque<f32>,
    cap: usize,
    /// Samples ever pushed: a change counter for redraw checks.
    pushed: u64,
    /// Time between samples, for reading a sample's age.
    interval_ms: u32,
}

impl Series {
    pub fn new(cap: usize, interval_ms: u32) -> Self {
        Self { buf: VecDeque::with_capacity(cap), cap, pushed: 0, interval_ms }
    }

    pub fn push(&mut self, v: f32) {
        if self.buf.len() >= self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(if v.is_finite() { v } else { 0.0 });
        self.pushed += 1;
    }

    /// Keeps the newest `cap` samples; a new interval drops them all, since their ages would be misread.
    pub fn resize(&mut self, cap: usize, interval_ms: u32) {
        if interval_ms != self.interval_ms {
            self.buf.clear();
            self.interval_ms = interval_ms;
        }
        self.cap = cap;
        while self.buf.len() > cap {
            self.buf.pop_front();
        }
    }

    pub fn cap(&self) -> usize {
        self.cap
    }
    pub fn pushed(&self) -> u64 {
        self.pushed
    }
    pub fn interval_ms(&self) -> u32 {
        self.interval_ms
    }
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
    pub fn get(&self, i: usize) -> f32 {
        self.buf[i]
    }
    pub fn max(&self) -> f32 {
        self.buf.iter().copied().fold(0.0, f32::max)
    }
}

pub struct History {
    /// (capacity, interval) of GPU series, for GPUs that appear later.
    gpu_size: (usize, u32),
    pub cpu: Series,
    /// Kernel ("System") share of CPU time, the CPU flyout's second line.
    pub cpu_kernel: Series,
    pub mem: Series,
    pub gpus: Vec<Series>,
    pub net_rx: Series,
    pub net_tx: Series,
    pub disk_r: Series,
    pub disk_w: Series,
    pub battery: Series,
    pub sensor: Series,
}

pub fn capacity(history_secs: u32, interval_ms: u32) -> usize {
    (history_secs as usize * 1000 / interval_ms.max(1) as usize).max(2)
}

/// (capacity, interval) of a module's series: `history_secs` of samples at the module's own interval.
fn size(cfg: &Config, m: Module) -> (usize, u32) {
    let interval = cfg.module_interval_ms(m);
    (capacity(cfg.history_secs, interval), interval)
}

impl History {
    pub fn new(cfg: &Config) -> Self {
        let s = |m| {
            let (cap, interval) = size(cfg, m);
            Series::new(cap, interval)
        };
        Self {
            gpu_size: size(cfg, Module::Gpu),
            cpu: s(Module::Cpu),
            cpu_kernel: s(Module::Cpu),
            mem: s(Module::Memory),
            gpus: Vec::new(),
            net_rx: s(Module::Network),
            net_tx: s(Module::Network),
            disk_r: s(Module::Disk),
            disk_w: s(Module::Disk),
            battery: s(Module::Battery),
            sensor: s(Module::Sensors),
        }
    }

    /// Follows a config change of `history_secs` or of an update interval.
    pub fn resize(&mut self, cfg: &Config) {
        self.gpu_size = size(cfg, Module::Gpu);
        let series = [
            (&mut self.cpu, Module::Cpu),
            (&mut self.cpu_kernel, Module::Cpu),
            (&mut self.mem, Module::Memory),
            (&mut self.net_rx, Module::Network),
            (&mut self.net_tx, Module::Network),
            (&mut self.disk_r, Module::Disk),
            (&mut self.disk_w, Module::Disk),
            (&mut self.battery, Module::Battery),
            (&mut self.sensor, Module::Sensors),
        ];
        for (s, m) in series {
            let (cap, interval) = size(cfg, m);
            s.resize(cap, interval);
        }
        let (cap, interval) = self.gpu_size;
        self.gpus.iter_mut().for_each(|s| s.resize(cap, interval));
    }

    /// Appends one sample per metric of each module `fresh` marks as just sampled, so a module sampled less
    /// often than others isn't repeated. Network follows the chosen interface and Sensors the chosen reading, as
    /// the taskbar cells do.
    pub fn push(&mut self, snap: &Snapshot, fresh: &Fresh, cfg: &Config) {
        let is = |m: Module| fresh[m.index()];
        if let Some(c) = snap.cpu.as_ref().filter(|_| is(Module::Cpu)) {
            self.cpu.push(c.total);
            self.cpu_kernel.push(c.kernel.min(c.total));
        }
        if let Some(m) = snap.memory.as_ref().filter(|m| m.total > 0 && is(Module::Memory)) {
            self.mem.push(m.used as f32 * 100.0 / m.total as f32);
        }
        if !snap.gpus.is_empty() && is(Module::Gpu) {
            let (cap, interval) = self.gpu_size;
            self.gpus.resize_with(snap.gpus.len(), || Series::new(cap, interval));
            for (s, g) in self.gpus.iter_mut().zip(&snap.gpus) {
                s.push(g.util_pct);
            }
        }
        if let Some((rx, tx)) = select::net_rates(snap, cfg).filter(|_| is(Module::Network)) {
            self.net_rx.push(rx as f32);
            self.net_tx.push(tx as f32);
        }
        if !snap.disks.is_empty() && is(Module::Disk) {
            self.disk_r.push(snap.disks.iter().map(|d| d.read_bps).sum::<f64>() as f32);
            self.disk_w.push(snap.disks.iter().map(|d| d.write_bps).sum::<f64>() as f32);
        }
        if let Some(b) = snap.battery.as_ref().filter(|_| is(Module::Battery)) {
            self.battery.push(b.percent);
        }
        if let Some(s) = select::taskbar_sensor(snap, cfg).filter(|_| is(Module::Sensors)) {
            self.sensor.push(s.value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling() {
        let mut s = Series::new(3, 1000);
        (0..5).for_each(|i| s.push(i as f32));
        assert_eq!((s.len(), s.get(0), s.get(2)), (3, 2.0, 4.0));
        s.resize(2, 1000);
        assert_eq!((s.len(), s.get(0), s.max()), (2, 3.0, 4.0));
        assert_eq!(s.pushed(), 5);
        s.resize(4, 2000);
        assert_eq!((s.len(), s.cap(), s.interval_ms()), (0, 4, 2000));
        assert_eq!(capacity(120, 1000), 120);
        assert_eq!(capacity(60, 500), 120);
    }

    #[test]
    fn each_module_keeps_its_own_pace() {
        let mut cfg = Config { interval_ms: 1000, history_secs: 60, ..Config::default() };
        let set_mem = |cfg: &mut Config, s| {
            cfg.modules.iter_mut().filter(|m| m.module == Module::Memory).for_each(|m| m.interval_s = s);
        };
        set_mem(&mut cfg, Some(5));
        let mut h = History::new(&cfg);
        assert_eq!((h.cpu.cap(), h.mem.cap(), h.mem.interval_ms()), (60, 12, 5000));
        let snap = Snapshot {
            cpu: Some(busy_core::CpuInfo { total: 50.0, ..Default::default() }),
            memory: Some(busy_core::MemInfo { total: 100, used: 40, ..Default::default() }),
            ..Snapshot::default()
        };
        let mut fresh = [false; Module::ALL.len()];
        fresh[Module::Cpu.index()] = true;
        h.push(&snap, &fresh, &cfg);
        assert_eq!((h.cpu.len(), h.mem.len()), (1, 0));
        fresh[Module::Memory.index()] = true;
        h.push(&snap, &fresh, &cfg);
        assert_eq!((h.cpu.len(), h.mem.len(), h.mem.get(0)), (2, 1, 40.0));
        set_mem(&mut cfg, None);
        h.resize(&cfg);
        assert_eq!((h.cpu.len(), h.mem.len(), h.mem.cap()), (2, 0, 60));
    }
}
