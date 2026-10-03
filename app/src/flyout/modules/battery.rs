use super::super::detail::{Chart, Detail, Value, series_span};
use busy_core::{Module, ModuleCfg};
use busy_ui::ctx::Ctx;
use busy_ui::fmt;
use busy_ui::i18n::{self, fill};
use busy_ui::tone;

/// Design `fly('bat')`: charge over time, time left, and the battery's state and health. The per-app "Power
/// usage" list needs the SRUM opt-in and is left out.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    let Some(b) = &ctx.snap.battery else { return Detail::waiting(Module::Battery) };
    let (t, hist) = (ctx.theme, ctx.hist);
    let mut d = Detail::new(Module::Battery);
    let f = &i18n::t().flyout;
    let wh = |mwh: u32| format!("{} Wh", fmt::decimal(f64::from(mwh) / 1000.0, 1));
    d.sub = match b.design_capacity_mwh.filter(|&c| c > 0) {
        Some(c) => fill(f.bat_internal_design, &[&wh(c)]),
        None => f.bat_internal.into(),
    };
    let state = match (b.charging, b.ac_online) {
        (true, _) => f.bat_charging,
        (false, true) => f.bat_plugged_in,
        _ => f.bat_on_battery,
    };
    d.big = fmt::pct(b.percent);
    d.big_label = match b.secs_remaining {
        Some(s) => fill(f.bat_remaining, &[&fmt::hours_minutes_long(s)]),
        None => state.into(),
    };
    d.chart = Some(Chart {
        lines: vec![(&hist.battery, t.color(tone::module(mc)))],
        max: 100.0,
        span: series_span(&hist.battery),
        max_label: "0–100%".into(),
        value: Value::Pct,
    });
    d.stat(f.bat_state, Some(state.into()));
    let rate = b.rate_mw.filter(|&r| r != 0).map(|r| fmt::watts(r.unsigned_abs() as f32 / 1000.0));
    d.stat(if b.charging { f.bat_charge_rate } else { f.bat_power_draw }, rate);
    let design = b.design_capacity_mwh.filter(|&c| c > 0);
    d.stat(f.bat_health, b.full_capacity_mwh.zip(design).map(|(f, c)| fmt::pct(f as f32 * 100.0 / c as f32)));
    d.stat(f.bat_cycle_count, b.cycle_count.map(|c| fmt::count(c as u64)));
    d.stat(f.bat_design_capacity, design.map(wh));
    d.stat(f.bat_full_charge, b.full_capacity_mwh.map(wh));
    d
}
