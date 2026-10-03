use super::super::detail::{Chart, Detail, Value, series_span};
use busy_core::{Module, ModuleCfg, SensorPick};
use busy_ui::ctx::Ctx;
use busy_ui::fmt;
use busy_ui::i18n::{self, fill, plural};
use busy_ui::select;
use busy_ui::tone::{self, SECOND};

/// Design `fly('cpu')`: utilization over time with the System share as a second line, the System / User /
/// Idle split, every logical processor, and the busiest processes.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    let Some(c) = &ctx.snap.cpu else { return Detail::waiting(Module::Cpu) };
    let (t, hist) = (ctx.theme, ctx.hist);
    let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
    let mut d = Detail::new(Module::Cpu);
    let f = &i18n::t().flyout;
    d.sub = c.name.trim().to_string();
    if c.physical_cores > 0 && c.logical_cores > 0 {
        let (cores, threads) = (plural(&f.cores, c.physical_cores as u64), plural(&f.threads, c.logical_cores as u64));
        d.sub = fill(f.cpu_sub, &[&d.sub, &cores, &threads]);
    }
    d.big = fmt::pct(c.total);
    d.big_label = f.utilization.into();
    d.chart = Some(Chart {
        lines: vec![(&hist.cpu, color), (&hist.cpu_kernel, second)],
        max: 100.0,
        span: series_span(&hist.cpu),
        max_label: "0–100%".into(),
        value: Value::Pct,
    });
    let system = c.kernel.min(c.total);
    d.legend = vec![
        (f.cpu_system.into(), fmt::pct(system), second),
        (f.cpu_user.into(), fmt::pct(c.total - system), color),
        (f.cpu_idle.into(), fmt::pct(100.0 - c.total), t.dim),
    ];
    d.cores = c.per_core.iter().map(|&p| (p, t.color(tone::fill(mc, p)))).collect();
    d.stat(f.speed, c.freq_mhz.map(fmt::mhz));
    d.stat(f.temperature, select::part_temp(ctx.snap, &SensorPick::Cpu).map(|s| fmt::temp(s.value, ctx.cfg.temp_unit)));
    d.stat(f.process_count, Some(fmt::count(c.processes as u64)));
    d.stat(f.thread_count, Some(fmt::count(c.threads as u64)));
    d.stat(f.handle_count, Some(fmt::count(c.handles as u64)));
    d.stat(f.up_time, Some(fmt::uptime(c.uptime_secs)));
    d.procs = super::procs(ctx, &ctx.snap.top.by_cpu, |p| fmt::pct1(p.cpu_pct));
    d
}
