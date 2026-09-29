use super::super::detail::{BarRow, Chart, Detail, Value, series_span};
use crate::ctx::Ctx;
use crate::render::nice_max;
use crate::tone::{self, SECOND};
use crate::{fmt, select};
use busy_core::{Module, ModuleCfg, RateUnit, SensorPick};

/// Design `fly('disk')`: active time, read/write rates over time, every volume's fill, response time and
/// temperature, and the processes doing the most I/O.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    let snap = ctx.snap;
    if snap.disks.is_empty() && snap.volumes.is_empty() {
        return Detail::waiting(Module::Disk);
    }
    let (t, hist) = (ctx.theme, ctx.hist);
    let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
    let mut d = Detail::new(Module::Disk);
    if let Some(first) = snap.disks.first() {
        d.sub = match snap.disks.len() {
            1 => first.name.clone(),
            n => format!("{} and {} more", first.name, n - 1),
        };
    }
    let active = snap.disks.iter().map(|d| d.active_pct).fold(0.0, f32::max);
    d.big = fmt::pct(active);
    d.big_label = "Active time".into();
    let peak = hist.disk_r.max().max(hist.disk_w.max());
    d.chart = Some(Chart {
        lines: vec![(&hist.disk_r, color), (&hist.disk_w, second)],
        max: nice_max(peak),
        span: series_span(&hist.disk_r),
        max_label: format!("Peak {}", fmt::rate(peak as f64)),
        value: Value::Rate(RateUnit::Bytes),
    });
    let (r, w) = (snap.disks.iter().map(|d| d.read_bps).sum(), snap.disks.iter().map(|d| d.write_bps).sum());
    d.legend = vec![("Read".into(), fmt::rate(r), color), ("Write".into(), fmt::rate(w), second)];
    d.bars_title = "Volumes";
    d.bars = snap
        .volumes
        .iter()
        .filter(|v| v.total > 0)
        .map(|v| {
            let pct = v.total.saturating_sub(v.free) as f32 * 100.0 / v.total as f32;
            BarRow {
                label: if v.label.is_empty() { v.mount.clone() } else { format!("{} ({})", v.label, v.mount) },
                text: format!("{} free of {}", fmt::bytes(v.free), fmt::bytes(v.total)),
                frac: pct / 100.0,
                color: t.color(tone::fill(mc, pct)),
            }
        })
        .collect();
    // The slowest disk: a response time is worth showing when something is slow.
    let response = snap.disks.iter().filter_map(|d| d.avg_response_ms).reduce(f32::max);
    d.stat("Avg. response", response.map(|ms| format!("{ms:.1} ms")));
    d.stat("Temperature", select::part_temp(snap, &SensorPick::Storage).map(|s| fmt::temp(s.value, ctx.cfg.temp_unit)));
    let total = |f: fn(&busy_core::DiskInfo) -> Option<u64>| snap.disks.iter().filter_map(f).reduce(|a, b| a + b);
    d.stat("Read since start", total(|d| d.read_total).map(fmt::bytes));
    d.stat("Written since start", total(|d| d.written_total).map(fmt::bytes));
    d.procs = super::procs(ctx, &snap.top.by_disk, |p| fmt::rate(p.io_bps));
    d
}
