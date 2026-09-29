//! One flyout section per module: what it shows and in which order.

use super::painter::Painter;
use crate::tone;
use crate::{fmt, select};
use busy_core::{Module, ModuleCfg, ProcEntry, SensorKind};

const TABS: [&str; 4] = ["CPU", "Memory", "Disk", "GPU"];

impl Painter<'_> {
    pub(super) fn section(&mut self, mc: &ModuleCfg) {
        let ctx = self.ctx;
        let (snap, t, unit) = (ctx.snap, *ctx.theme, ctx.cfg.temp_unit);
        let value = |pct| t.color(tone::value(mc, pct));
        match mc.module {
            // Drawn by `modules`.
            Module::Cpu | Module::Memory | Module::Gpu | Module::Network | Module::Disk => {}
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
