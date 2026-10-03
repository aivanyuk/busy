use super::super::detail::{Chart, Detail, Value, series_span};
use busy_core::{Module, ModuleCfg};
use busy_ui::ctx::Ctx;
use busy_ui::fmt;
use busy_ui::i18n::{self, fill};
use busy_ui::tone::{self, SECOND};

/// Design `fly('mem')`: memory in use over time, the composition of physical memory (in use, modified,
/// standby, free), commit and kernel pools, and the processes using the most.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    let Some(m) = ctx.snap.memory.as_ref().filter(|m| m.total > 0) else { return Detail::waiting(Module::Memory) };
    let (t, hist) = (ctx.theme, ctx.hist);
    let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
    let mut d = Detail::new(Module::Memory);
    let f = &i18n::t().flyout;
    d.sub = match m.hardware_reserved {
        Some(r) => fill(f.mem_installed, &[&fmt::bytes(m.total + r)]),
        None => fill(f.mem_usable, &[&fmt::bytes(m.total)]),
    };
    d.big = fmt::bytes(m.used);
    d.big_label = fill(f.mem_of_in_use, &[&fmt::bytes(m.total)]);
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
            (f.mem_in_use, in_use, color),
            (f.mem_modified, modified, second),
            (f.mem_standby, standby, t.standby),
            (f.mem_free, free, t.dim),
        ],
        _ => vec![(f.mem_in_use, m.used, color), (f.mem_available, m.available, t.dim)],
    };
    d.seg = parts.iter().map(|&(_, b, c)| (b as f32, c)).collect();
    d.legend = parts.iter().map(|&(l, b, c)| (l.into(), fmt::bytes(b), c)).collect();
    d.stat(f.mem_committed, Some(format!("{} / {}", fmt::bytes(m.commit_used), fmt::bytes(m.commit_limit))));
    d.stat(f.mem_compressed, m.compressed.map(fmt::bytes));
    d.stat(f.mem_paged_pool, m.paged_pool.map(fmt::bytes));
    d.stat(f.mem_nonpaged_pool, m.nonpaged_pool.map(fmt::bytes));
    d.stat(f.mem_hardware_reserved, m.hardware_reserved.map(fmt::bytes));
    if m.standby.is_none() {
        d.stat(f.mem_cached, m.cached.map(fmt::bytes));
    }
    d.procs = super::procs(ctx, &ctx.snap.top.by_mem, |p| fmt::bytes(p.mem_bytes));
    d
}
