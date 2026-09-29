use super::super::detail::{Chart, Detail, Value, series_span};
use crate::ctx::Ctx;
use crate::tone::{self, SECOND};
use crate::{fmt, select};
use busy_core::{Module, ModuleCfg};

/// Design `fly('gpu')`: the GPU the taskbar cell shows (the busiest), its utilization over time, the busiest
/// engines, memory, sensors, driver and DirectX level, and the processes using it.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    let Some((i, g)) = select::busiest_gpu(ctx.snap) else { return Detail::waiting(Module::Gpu) };
    let (t, hist, unit) = (ctx.theme, ctx.hist, ctx.cfg.temp_unit);
    let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
    let mut d = Detail::new(Module::Gpu);
    d.sub = g.name.clone();
    if ctx.snap.gpus.len() > 1 {
        d.sub = format!("{} · busiest of {}", d.sub, ctx.snap.gpus.len());
    }
    d.big = fmt::pct(g.util_pct);
    d.big_label = "Utilization".into();
    d.chart = hist.gpus.get(i).map(|s| Chart {
        lines: vec![(s, color)],
        max: 100.0,
        span: series_span(s),
        max_label: "0–100%".into(),
        value: Value::Pct,
    });
    d.legend = select::busy_engines(&g.engines)
        .into_iter()
        .zip([color, second, t.dim])
        .map(|((name, pct), c)| (name.to_string(), fmt::pct(pct), c))
        .collect();
    let of = |used, total| format!("{} / {}", fmt::bytes(used), fmt::bytes(total));
    d.stat("Dedicated memory", (g.vram_total > 0).then(|| of(g.vram_used, g.vram_total)));
    d.stat("Shared memory", (g.shared_used > 0).then(|| fmt::bytes(g.shared_used)));
    d.stat("Temperature", g.temp_c.map(|v| fmt::temp(v, unit)));
    d.stat("Hot spot", g.hotspot_c.map(|v| fmt::temp(v, unit)));
    d.stat("Power", g.power_w.map(fmt::watts));
    d.stat("Fan", g.fan_rpm.map(|r| format!("{} rpm", fmt::count(r as u64))).or(g.fan_pct.map(fmt::pct)));
    d.stat("Core clock", g.core_clock_mhz.map(fmt::mhz));
    d.stat("Memory clock", g.mem_clock_mhz.map(fmt::mhz));
    d.stat("Driver", g.driver_version.clone());
    // The DirectX version that exposes the level: 12 from feature level 11_0 up, else 11.
    d.stat(
        "DirectX",
        g.feature_level.map(|(major, minor)| format!("{} (FL {major}_{minor})", if major >= 11 { 12 } else { 11 })),
    );
    d.procs = super::procs(ctx, &ctx.snap.top.by_gpu, |p| fmt::pct1(p.gpu_pct));
    d
}
