//! Rolling metric history kept on the UI thread.

use busy_core::Snapshot;
use std::collections::VecDeque;

#[derive(Default)]
pub struct Series {
    buf: VecDeque<f32>,
    cap: usize,
    /// Samples ever pushed: a change counter for redraw checks.
    pushed: u64,
}

impl Series {
    pub fn new(cap: usize) -> Self {
        Self { buf: VecDeque::with_capacity(cap), cap, pushed: 0 }
    }

    pub fn push(&mut self, v: f32) {
        if self.buf.len() >= self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(if v.is_finite() { v } else { 0.0 });
        self.pushed += 1;
    }

    pub fn set_cap(&mut self, cap: usize) {
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
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    pub fn get(&self, i: usize) -> f32 {
        self.buf[i]
    }
    pub fn max(&self) -> f32 {
        self.buf.iter().copied().fold(0.0, f32::max)
    }
}

pub struct History {
    cap: usize,
    pub cpu: Series,
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

impl History {
    pub fn new(cap: usize) -> Self {
        let s = || Series::new(cap);
        Self {
            cap,
            cpu: s(),
            mem: s(),
            gpus: Vec::new(),
            net_rx: s(),
            net_tx: s(),
            disk_r: s(),
            disk_w: s(),
            battery: s(),
            sensor: s(),
        }
    }

    pub fn set_capacity(&mut self, cap: usize) {
        self.cap = cap;
        let fixed = [
            &mut self.cpu,
            &mut self.mem,
            &mut self.net_rx,
            &mut self.net_tx,
            &mut self.disk_r,
            &mut self.disk_w,
            &mut self.battery,
            &mut self.sensor,
        ];
        fixed.into_iter().chain(self.gpus.iter_mut()).for_each(|s| s.set_cap(cap));
    }

    /// Appends one sample per metric present in `snap`. `sensor` is the resolved pinned-sensor value.
    pub fn push(&mut self, snap: &Snapshot, sensor: Option<f32>) {
        if let Some(c) = &snap.cpu {
            self.cpu.push(c.total);
        }
        if let Some(m) = snap.memory.as_ref().filter(|m| m.total > 0) {
            self.mem.push(m.used as f32 * 100.0 / m.total as f32);
        }
        if !snap.gpus.is_empty() {
            self.gpus.resize_with(snap.gpus.len(), || Series::new(self.cap));
            for (s, g) in self.gpus.iter_mut().zip(&snap.gpus) {
                s.push(g.util_pct);
            }
        }
        if let Some(n) = &snap.net {
            self.net_rx.push(n.rx_bps as f32);
            self.net_tx.push(n.tx_bps as f32);
        }
        if !snap.disks.is_empty() {
            self.disk_r.push(snap.disks.iter().map(|d| d.read_bps).sum::<f64>() as f32);
            self.disk_w.push(snap.disks.iter().map(|d| d.write_bps).sum::<f64>() as f32);
        }
        if let Some(b) = &snap.battery {
            self.battery.push(b.percent);
        }
        if let Some(v) = sensor {
            self.sensor.push(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling() {
        let mut s = Series::new(3);
        (0..5).for_each(|i| s.push(i as f32));
        assert_eq!((s.len(), s.get(0), s.get(2)), (3, 2.0, 4.0));
        s.set_cap(2);
        assert_eq!((s.len(), s.get(0), s.max()), (2, 3.0, 4.0));
        assert_eq!(s.pushed(), 5);
        assert_eq!(capacity(120, 1000), 120);
        assert_eq!(capacity(60, 500), 120);
    }
}
