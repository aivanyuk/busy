use super::super::detail::{Chart, Detail, Value, series_span};
use crate::ctx::Ctx;
use crate::fmt;
use crate::tone::{self, SECOND};
use busy_core::{Module, ModuleCfg};

/// Design `fly('mem')`: memory in use over time, the composition of physical memory (in use, modified,
/// standby, free), commit and kernel pools, and the processes using the most.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    let Some(m) = ctx.snap.memory.as_ref().filter(|m| m.total > 0) else { return Detail::waiting(Module::Memory) };
    let (t, hist) = (ctx.theme, ctx.hist);
    let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
    let mut d = Detail::new(Module::Memory);
    d.sub = match m.hardware_reserved {
        Some(r) => format!("{} installed", fmt::bytes(m.total + r)),
        None => format!("{} usable", fmt::bytes(m.total)),
    };
    d.big = fmt::bytes(m.used);
    d.big_label = format!("of {} in use", fmt::bytes(m.total));
    d.chart = Some(Chart {
        lines: vec![(&hist.mem, color)],
        max: 100.0,
        span: series_span(&hist.mem),
        max_label: "0–100%".into(),
        value: Value::Pct,
    });
    // Task Manager's composition when the page lists were read; else just used and available.
    let parts = match (m.in_use(), m.modified, m.standby, m.free) {
        (Some(in_use), Some(modified), Some(standby), Some(free)) => vec![
            ("In use", in_use, color),
            ("Modified", modified, second),
            ("Standby", standby, t.standby),
            ("Free", free, t.dim),
        ],
        _ => vec![("In use", m.used, color), ("Available", m.available, t.dim)],
    };
    d.seg = parts.iter().map(|&(_, b, c)| (b as f32, c)).collect();
    d.legend = parts.iter().map(|&(l, b, c)| (l.into(), fmt::bytes(b), c)).collect();
    d.stat("Committed", Some(format!("{} / {}", fmt::bytes(m.commit_used), fmt::bytes(m.commit_limit))));
    d.stat("Compressed", m.compressed.map(fmt::bytes));
    d.stat("Paged pool", m.paged_pool.map(fmt::bytes));
    d.stat("Non-paged pool", m.nonpaged_pool.map(fmt::bytes));
    d.stat("Hardware reserved", m.hardware_reserved.map(fmt::bytes));
    if m.standby.is_none() {
        d.stat("Cached", m.cached.map(fmt::bytes));
    }
    d.procs = super::procs(ctx, &ctx.snap.top.by_mem, |p| fmt::bytes(p.mem_bytes));
    d
}
