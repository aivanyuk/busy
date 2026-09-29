use super::super::detail::{BarRow, Chart, Detail, Value, series_span};
use crate::ctx::Ctx;
use crate::render::nice_max;
use crate::tone;
use busy_core::{Module, ModuleCfg, SensorKind, SensorReading};
use busy_ui::{fmt, select};

/// Design `fly('sens')`: the reading the cell shows over time, every temperature as a bar, and the other
/// readings (fans, power, clocks) as stats. No process list.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    let (snap, cfg, t, hist) = (ctx.snap, ctx.cfg, ctx.theme, ctx.hist);
    let mut d = Detail::new(Module::Sensors);
    d.sub = "Temperatures, fans and power".into();
    if snap.sensors.is_empty() {
        d.note = Some(
            if cfg.opt_in.third_party_sensors {
                "No sensors available. Run LibreHardwareMonitor or HWiNFO (with shared memory enabled) for CPU \
             temperatures."
            } else {
                // No UI for opt-ins until the Phase 4 settings window (Advanced page).
                "No sensors available. Reading LibreHardwareMonitor / HWiNFO is off; enable \
             opt_in.third_party_sensors in config.json for CPU temperatures."
            }
            .into(),
        );
        return d;
    }
    let unit = cfg.temp_unit;
    let several = snap.sensors.iter().any(|s| s.hardware != snap.sensors[0].hardware);
    // With readings from several parts, a name alone ("Temperature") is ambiguous.
    let name = |s: &SensorReading| if several { format!("{} · {}", s.hardware, s.name) } else { s.name.clone() };
    if let Some(s) = select::taskbar_sensor(snap, cfg) {
        let is_temp = s.kind == SensorKind::Temperature;
        d.big = fmt::sensor(s.value, s.kind, unit);
        d.big_label = s.name.clone();
        let top = fmt::temp(100.0, unit);
        d.chart = Some(Chart {
            lines: vec![(
                &hist.sensor,
                if is_temp { t.color(tone::fill(mc, s.value)) } else { t.color(tone::module(mc)) },
            )],
            max: if is_temp { 100.0 } else { nice_max(hist.sensor.max()) },
            span: series_span(&hist.sensor),
            max_label: if is_temp { format!("0–{top}") } else { String::new() },
            value: Value::Sensor(s.kind, unit),
        });
    }
    let temps = snap.sensors.iter().filter(|s| s.kind == SensorKind::Temperature);
    d.bars_title = "Temperatures";
    d.bars = temps
        .map(|s| BarRow {
            label: name(s),
            text: fmt::temp(s.value, unit),
            // Load coloring reads °C as percent, like the design.
            frac: s.value / 100.0,
            color: t.color(tone::fill(mc, s.value)),
        })
        .collect();
    for s in snap.sensors.iter().filter(|s| s.kind != SensorKind::Temperature) {
        d.stat(&name(s), Some(fmt::sensor(s.value, s.kind, unit)));
    }
    d
}
