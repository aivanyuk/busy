//! What the widget shows: one cell per enabled module, with its label, value and colors (design `widget()`).
//! How a cell is measured and drawn in its style is `styles.rs`.

use crate::ctx::Ctx;
use crate::history::Series;
use crate::render::nice_max;
use crate::theme::Color;
use crate::tone::{self, SECOND};
use crate::{fmt, select};
use busy_core::{CellStyle, CpuBar, Module, SensorKind};

pub(super) struct Cell<'a> {
    pub(super) module: Module,
    /// Caption above the value (design `M.short`); `None` when the module's `show_label` is off.
    pub(super) label: Option<String>,
    pub(super) body: Body<'a>,
}

/// A cell's content in its style (design `MeterWidget`).
pub(super) enum Body<'a> {
    Text {
        value: String,
        color: Color,
    },
    /// Sparkline under a label/value row. The first series is filled (area .3), the others are lines only.
    Graph {
        value: String,
        lines: Vec<(&'a Series, Color)>,
        max: f32,
    },
    /// Vertical bars (fill 0..=1) next to the label and value.
    Bar {
        bars: Vec<(f32, Color)>,
        /// Width of each bar: 3 DIPs for per-core bars, 6 for a single bar.
        bar_w: f32,
        value: String,
        color: Color,
    },
    /// Two rows of (key, key color, value), e.g. ↑ upload / ↓ download or R / W.
    Io {
        rows: [(&'static str, Color, String); 2],
    },
}

/// What a cell's pixels depend on, owned so the widget can compare it with the last drawn frame.
#[derive(PartialEq)]
pub(super) struct Key {
    module: Module,
    label: Option<String>,
    /// Every drawn string with its color.
    texts: Vec<(String, Color)>,
    /// Bar fill in 1/256 steps (under half a device pixel of the 26-DIP bar up to 400 % scale) and color.
    bars: Vec<(u8, Color)>,
    /// Graph cells: (samples pushed, samples held, color) per series; the samples themselves never change.
    series: Vec<(u64, usize, Color)>,
    /// Graph scale, or bar width.
    size: f32,
}

impl Cell<'_> {
    pub(super) fn key(&self) -> Key {
        let mut key = Key {
            module: self.module,
            label: self.label.clone(),
            texts: Vec::new(),
            bars: Vec::new(),
            series: Vec::new(),
            size: 0.0,
        };
        match &self.body {
            Body::Text { value, color } => key.texts.push((value.clone(), *color)),
            Body::Graph { value, lines, max } => {
                key.texts.push((value.clone(), Color::default()));
                key.series = lines.iter().map(|(s, c)| (s.pushed(), s.len(), *c)).collect();
                key.size = *max;
            }
            Body::Bar { bars, bar_w, value, color } => {
                key.texts.push((value.clone(), *color));
                key.bars = bars.iter().map(|(f, c)| ((f.clamp(0.0, 1.0) * 255.0).round() as u8, *c)).collect();
                key.size = *bar_w;
            }
            Body::Io { rows } => key.texts = rows.iter().map(|(k, c, v)| (format!("{k} {v}"), *c)).collect(),
        }
        key
    }
}

/// One cell per module with a taskbar cell that has data, in config order.
pub(super) fn cells<'a>(ctx: &Ctx<'a>) -> Vec<Cell<'a>> {
    let (snap, hist, t) = (ctx.snap, ctx.hist, ctx.theme);
    let mut out = Vec::new();
    for mc in ctx.cfg.modules.iter().filter(|m| m.taskbar) {
        // Design `widget()`: graphs and bars in the module color (or by load), values in `fg` (or by load);
        // rates pair the module color (download, read) with `SECOND` (upload, write).
        let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
        let fill = |pct| t.color(tone::fill(mc, pct));
        let value = |pct| t.color(tone::value(mc, pct));
        // A percentage in the cell's style (CPU, memory, GPU, battery), with its (fill, value) colors.
        let pct_body = |pct: f32, series: Option<&'a Series>, (fill, value): (Color, Color)| {
            let v = fmt::pct(pct);
            match mc.style {
                CellStyle::Graph => {
                    Body::Graph { value: v, lines: series.map(|s| vec![(s, fill)]).unwrap_or_default(), max: 100.0 }
                }
                CellStyle::Bar => Body::Bar { bars: vec![(pct / 100.0, fill)], bar_w: 6.0, value: v, color: value },
                _ => Body::Text { value: v, color: value },
            }
        };
        let opts = &ctx.cfg.options;
        let body = match mc.module {
            Module::Cpu => snap.cpu.as_ref().map(|c| match mc.style {
                // Design: the "Each core" bars take the load colors per bar; the value stays the total.
                CellStyle::Bar if opts.cpu.bar == CpuBar::Cores && !c.per_core.is_empty() => Body::Bar {
                    bars: select::core_bars(&c.per_core).into_iter().map(|p| (p / 100.0, fill(p))).collect(),
                    bar_w: 3.0,
                    value: fmt::pct(c.total),
                    color: value(c.total),
                },
                _ => pct_body(c.total, Some(&hist.cpu), (fill(c.total), value(c.total))),
            }),
            Module::Memory => snap.memory.as_ref().filter(|m| m.total > 0).map(|m| {
                let p = m.used as f32 * 100.0 / m.total as f32;
                pct_body(p, Some(&hist.mem), (fill(p), value(p)))
            }),
            Module::Gpu => {
                let busiest = snap.gpus.iter().enumerate().max_by(|a, b| a.1.util_pct.total_cmp(&b.1.util_pct));
                busiest.map(|(i, g)| pct_body(g.util_pct, hist.gpus.get(i), (fill(g.util_pct), value(g.util_pct))))
            }
            Module::Battery => snap.battery.as_ref().map(|b| {
                let (f, v) = tone::battery(mc, b);
                pct_body(b.percent, Some(&hist.battery), (t.color(f), t.color(v)))
            }),
            Module::Network => select::net_rates(snap, ctx.cfg).map(|(rx, tx)| match mc.style {
                // Design: the Graph style has no value; the two rates are the two lines.
                CellStyle::Graph => Body::Graph {
                    value: String::new(),
                    lines: vec![(&hist.net_rx, color), (&hist.net_tx, second)],
                    max: nice_max(hist.net_rx.max().max(hist.net_tx.max())),
                },
                _ => {
                    let rate = |bps| fmt::rate_in(bps, opts.network.units);
                    Body::Io { rows: [("↑", second, rate(tx)), ("↓", color, rate(rx))] }
                }
            }),
            // Design: Io shows the read/write rates of all disks, Text and Bar how full the chosen drive is.
            Module::Disk => match mc.style {
                CellStyle::Io => (!snap.disks.is_empty()).then(|| {
                    let r = snap.disks.iter().map(|d| d.read_bps).sum::<f64>();
                    let w = snap.disks.iter().map(|d| d.write_bps).sum::<f64>();
                    Body::Io { rows: [("R", color, fmt::rate(r)), ("W", second, fmt::rate(w))] }
                }),
                _ => select::disk_volume(snap, ctx.cfg).filter(|v| v.total > 0).map(|v| {
                    let p = v.total.saturating_sub(v.free) as f32 * 100.0 / v.total as f32;
                    pct_body(p, None, (fill(p), value(p)))
                }),
            },
            Module::Sensors => select::taskbar_sensor(snap, ctx.cfg).map(|s| {
                let v = fmt::sensor(s.value, s.kind, ctx.cfg.temp_unit);
                // Load coloring reads a temperature in °C as percent, like the design.
                let is_temp = s.kind == SensorKind::Temperature;
                let (c, vc) = if is_temp { (fill(s.value), value(s.value)) } else { (color, t.fg) };
                match mc.style {
                    CellStyle::Graph => Body::Graph {
                        value: v,
                        lines: vec![(&hist.sensor, c)],
                        max: if is_temp { 100.0 } else { nice_max(hist.sensor.max()) },
                    },
                    _ => Body::Text { value: v, color: vc },
                }
            }),
            // Flyout-only: `Config::normalize` never leaves it on the taskbar.
            Module::Processes => None,
        };
        let Some(body) = body else { continue };
        let label = mc.show_label.then(|| label(mc.module, mc.style, ctx));
        out.push(Cell { module: mc.module, label, body });
    }
    out
}

/// Design `MODS.short`, except: Disk Text/Bar name their drive ("C:"), Battery shows the time left (`h:mm`)
/// on battery with `show_remaining`, and a sensor pick that isn't a temperature is named by its reading.
fn label(m: Module, style: CellStyle, ctx: &Ctx) -> String {
    let opts = &ctx.cfg.options;
    match m {
        Module::Cpu => "CPU".into(),
        Module::Memory => "RAM".into(),
        Module::Gpu => "GPU".into(),
        Module::Network => "NET".into(),
        Module::Disk => match select::disk_volume(ctx.snap, ctx.cfg) {
            Some(v) if style != CellStyle::Io => v.mount.clone(),
            _ => "DISK".into(),
        },
        Module::Battery => match ctx.snap.battery.as_ref().and_then(|b| b.secs_remaining) {
            Some(secs) if opts.battery.show_remaining => fmt::hours_minutes(secs),
            _ => "BAT".into(),
        },
        Module::Sensors => match select::taskbar_sensor(ctx.snap, ctx.cfg) {
            Some(s) if s.kind != SensorKind::Temperature => s.name.chars().take(6).collect::<String>().to_uppercase(),
            _ => "TEMP".into(),
        },
        Module::Processes => String::new(),
    }
}
