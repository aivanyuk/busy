//! One flyout section per module: what it shows and in which order.

use super::painter::Painter;
use crate::render::nice_max;
use crate::tone::{self, SECOND};
use crate::{fmt, select};
use busy_core::{Module, ModuleCfg, ProcEntry, SensorKind};

const TABS: [&str; 4] = ["CPU", "Memory", "Disk", "GPU"];

impl Painter<'_> {
    pub(super) fn section(&mut self, mc: &ModuleCfg) {
        let ctx = self.ctx;
        let (snap, hist, t, unit) = (ctx.snap, ctx.hist, *ctx.theme, ctx.cfg.temp_unit);
        // Design `fly()`: charts in the module color (by load for bars), rates paired with `SECOND`.
        let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
        let fill = |pct| t.color(tone::fill(mc, pct));
        let value = |pct| t.color(tone::value(mc, pct));
        let rate = &|v: f32| fmt::rate(v as f64);
        match mc.module {
            // Drawn by `modules`.
            Module::Cpu | Module::Memory | Module::Gpu => {}
            Module::Network => {
                let Some(n) = &snap.net else { return self.missing("Network") };
                self.header("Network", "", t.fg);
                self.rates(("↓", n.rx_bps, color), ("↑", n.tx_bps, second));
                let max = nice_max(hist.net_rx.max().max(hist.net_tx.max()));
                self.graph(
                    &[(&hist.net_rx, color), (&hist.net_tx, second)],
                    max,
                    56.0,
                    rate,
                    Some(fmt::rate(max as f64)),
                );
                self.kv(&[("Received", fmt::bytes(n.rx_total)), ("Sent", fmt::bytes(n.tx_total))]);
                for i in n.interfaces.iter().filter(|i| i.connected) {
                    self.gap(4.0);
                    self.row(&i.name, &format!("↓ {}   ↑ {}", fmt::rate(i.rx_bps), fmt::rate(i.tx_bps)), t.fg2);
                    let mut info = i.ipv4.join(", ");
                    if i.link_speed_bps > 0 {
                        let speed = match i.link_speed_bps {
                            s if s >= 1_000_000_000 => format!("{} Gbps", s as f64 / 1e9),
                            s => format!("{} Mbps", s / 1_000_000),
                        };
                        info = if info.is_empty() { speed } else { format!("{info}  ·  {speed}") };
                    }
                    if !info.is_empty() {
                        self.sub(&info);
                    }
                }
            }
            Module::Disk => {
                if snap.disks.is_empty() && snap.volumes.is_empty() {
                    return self.missing("Disk");
                }
                let (r, w) = (
                    snap.disks.iter().map(|d| d.read_bps).sum::<f64>(),
                    snap.disks.iter().map(|d| d.write_bps).sum::<f64>(),
                );
                self.header("Disk", "", t.fg);
                self.rates(("R", r, color), ("W", w, second));
                let max = nice_max(hist.disk_r.max().max(hist.disk_w.max()));
                self.graph(
                    &[(&hist.disk_r, color), (&hist.disk_w, second)],
                    max,
                    48.0,
                    rate,
                    Some(fmt::rate(max as f64)),
                );
                for d in &snap.disks {
                    self.row(&d.name, &format!("R {}   W {}", fmt::rate(d.read_bps), fmt::rate(d.write_bps)), t.fg2);
                    self.meter("Active", d.active_pct, fill(d.active_pct));
                }
                if !snap.volumes.is_empty() {
                    self.gap(4.0);
                }
                for v in &snap.volumes {
                    let used = v.total.saturating_sub(v.free);
                    let name = if v.label.is_empty() { v.mount.clone() } else { format!("{} {}", v.mount, v.label) };
                    self.row(&name, &format!("{} free of {}", fmt::bytes(v.free), fmt::bytes(v.total)), t.fg2);
                    let frac = if v.total > 0 { used as f32 / v.total as f32 } else { 0.0 };
                    self.bar(frac, fill(frac * 100.0));
                }
            }
            Module::Sensors => {
                let pinned = select::taskbar_sensor(snap, ctx.cfg);
                let v = pinned.map(|s| fmt::sensor(s.value, s.kind, unit)).unwrap_or_default();
                let vc = pinned.filter(|s| s.kind == SensorKind::Temperature).map_or(t.fg, |s| value(s.value));
                self.header("Sensors", &v, vc);
                if snap.sensors.is_empty() {
                    self.hint(if ctx.cfg.opt_in.third_party_sensors {
                        "No sensors available. Run LibreHardwareMonitor or HWiNFO (with shared memory enabled) for CPU temperatures."
                    } else {
                        // No UI for opt-ins until the Phase 4 settings window (Advanced page).
                        "No sensors available. Reading LibreHardwareMonitor / HWiNFO is off; enable opt_in.third_party_sensors in config.json for CPU temperatures."
                    });
                    return;
                }
                let mut groups: Vec<&str> = Vec::new();
                for s in &snap.sensors {
                    if !groups.contains(&s.hardware.as_str()) {
                        groups.push(&s.hardware);
                    }
                }
                for g in groups {
                    self.group(g);
                    for s in snap.sensors.iter().filter(|s| s.hardware == g) {
                        let c = if s.kind == SensorKind::Temperature { value(s.value) } else { t.fg };
                        self.row_kv(&s.name, &fmt::sensor(s.value, s.kind, unit), c);
                    }
                }
            }
            Module::Battery => {
                let Some(b) = &snap.battery else { return self.missing("Battery") };
                let (fill, value) = tone::battery(mc, b);
                let (c, vc) = (t.color(fill), t.color(value));
                self.header("Battery", &format!("{}{}", fmt::pct(b.percent), if b.charging { " ⚡" } else { "" }), vc);
                self.bar(b.percent / 100.0, c);
                let state = match (b.charging, b.ac_online) {
                    (true, _) => "Charging",
                    (false, true) => "Plugged in",
                    _ => "On battery",
                };
                let mut kv = vec![("State", state.to_string())];
                if let Some(s) = b.secs_remaining {
                    kv.push(("Time left", fmt::duration(s as u64)));
                }
                if let Some(r) = b.rate_mw {
                    kv.push(("Rate", format!("{}{}", if r > 0 { "+" } else { "" }, fmt::watts(r as f32 / 1000.0))));
                }
                if let (Some(full), Some(design)) = (b.full_capacity_mwh, b.design_capacity_mwh.filter(|d| *d > 0)) {
                    kv.push(("Health", fmt::pct(full as f32 * 100.0 / design as f32)));
                    kv.push(("Capacity", format!("{:.1} / {:.1} Wh", full as f32 / 1000.0, design as f32 / 1000.0)));
                }
                if let Some(c) = b.cycle_count {
                    kv.push(("Cycles", c.to_string()));
                }
                self.kv(&kv);
            }
            Module::Processes => {
                self.header("Processes", "", t.fg);
                self.tabs(&TABS);
                let top = &snap.top;
                let (list, val): (&[ProcEntry], fn(&ProcEntry) -> String) = match self.tab {
                    0 => (&top.by_cpu, |p| fmt::pct(p.cpu_pct)),
                    1 => (&top.by_mem, |p| fmt::bytes(p.mem_bytes)),
                    2 => (&top.by_disk, |p| fmt::rate(p.io_bps)),
                    _ => (&top.by_gpu, |p| fmt::pct(p.gpu_pct)),
                };
                if list.is_empty() {
                    self.sub("No data");
                }
                for p in list.iter().take(busy_core::TOP_N) {
                    self.row(&p.name, &val(p), t.fg);
                }
            }
        }
    }
}
