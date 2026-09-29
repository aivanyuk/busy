use super::super::detail::{Chart, Detail, Value, series_span};
use crate::ctx::Ctx;
use crate::fmt;
use crate::select;
use crate::tone::{self, SECOND};
use busy_core::{Module, ModuleCfg, SensorPick};

/// Design `fly('cpu')`: utilization over time with the System share as a second line, the System / User /
/// Idle split, every logical processor, and the busiest processes.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    let Some(c) = &ctx.snap.cpu else { return Detail::waiting(Module::Cpu) };
    let (t, hist) = (ctx.theme, ctx.hist);
    let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
    let mut d = Detail::new(Module::Cpu);
    d.sub = c.name.trim().to_string();
    if c.physical_cores > 0 && c.logical_cores > 0 {
        d.sub = format!("{} · {} cores, {} threads", d.sub, c.physical_cores, c.logical_cores);
    }
    d.big = fmt::pct(c.total);
    d.big_label = "Utilization".into();
    d.chart = Some(Chart {
        lines: vec![(&hist.cpu, color), (&hist.cpu_kernel, second)],
        max: 100.0,
        span: series_span(&hist.cpu),
        max_label: "0–100%".into(),
        value: Value::Pct,
    });
    let system = c.kernel.min(c.total);
    d.legend = vec![
        ("System".into(), fmt::pct(system), second),
        ("User".into(), fmt::pct(c.total - system), color),
        ("Idle".into(), fmt::pct(100.0 - c.total), t.dim),
    ];
    d.cores = c.per_core.iter().map(|&p| (p, t.color(tone::fill(mc, p)))).collect();
    d.stat("Speed", c.freq_mhz.map(fmt::mhz));
    d.stat("Temperature", select::part_temp(ctx.snap, &SensorPick::Cpu).map(|s| fmt::temp(s.value, ctx.cfg.temp_unit)));
    d.stat("Processes", Some(fmt::count(c.processes as u64)));
    d.stat("Threads", Some(fmt::count(c.threads as u64)));
    d.stat("Handles", Some(fmt::count(c.handles as u64)));
    d.stat("Up time", Some(fmt::uptime(c.uptime_secs)));
    d.procs = super::procs(ctx, &ctx.snap.top.by_cpu, |p| fmt::pct1(p.cpu_pct));
    d
}
