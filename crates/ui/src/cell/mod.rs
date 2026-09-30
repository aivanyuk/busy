//! A module's taskbar cell: its label, value and colors (design `widget()`), shared by the taskbar widget and
//! the settings window's preview. How a cell is measured and drawn in its style is `styles.rs`.

mod styles;

pub use styles::{CELL_H, Fonts};

use crate::ctx::Ctx;
use crate::history::Series;
use crate::render::nice_max;
use crate::theme::Color;
use crate::tone::{self, SECOND};
use crate::{fmt, select};
use busy_core::{CellStyle, Config, CpuBar, Module, ModuleCfg, SensorKind, Snapshot};

pub struct Cell<'a> {
    pub module: Module,
    /// Caption above the value (design `M.short`); `None` when the module's `show_label` is off.
    label: Option<String>,
    body: Body<'a>,
}

/// A cell's content in its style (design `MeterWidget`).
enum Body<'a> {
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
pub struct Key {
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
    /// Tooltip text (design `title`): "<Module>: <value>", both rates for Io, the name alone without a value.
    pub fn tip(&self) -> String {
        let name = self.module.label();
        match &self.body {
            Body::Text { value, .. } | Body::Graph { value, .. } | Body::Bar { value, .. } if !value.is_empty() => {
                format!("{name}: {value}")
            }
            Body::Io { rows: [(k1, _, v1), (k2, _, v2)] } => format!("{name}: {k1} {v1}  {k2} {v2}"),
            _ => name.into(),
        }
    }

    pub fn key(&self) -> Key {
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
pub fn cells<'a>(ctx: &Ctx<'a>) -> Vec<Cell<'a>> {
    ctx.cfg.modules.iter().filter(|m| m.taskbar).filter_map(|mc| cell(ctx, mc)).collect()
}

/// `mc`'s cell, whether or not it is on the taskbar; `None` while its module has no data.
pub fn cell<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Option<Cell<'a>> {
    let (snap, hist, t) = (ctx.snap, ctx.hist, ctx.theme);
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
        Module::Gpu => select::busiest_gpu(snap)
            .map(|(i, g)| pct_body(g.util_pct, hist.gpus.get(i), (fill(g.util_pct), value(g.util_pct)))),
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
    let label = mc.show_label.then(|| label(mc.module, mc.style, ctx));
    Some(Cell { module: mc.module, label, body: body? })
}

/// A module's reading as one short value, whatever its cell's style (design onboarding: `w.value || w.v2`):
/// the percentage (Disk: how full its drive is), Network's download rate, the taskbar sensor's reading.
/// `None` without data, and for Processes, which has no reading of its own.
pub fn sample(snap: &Snapshot, cfg: &Config, m: Module) -> Option<String> {
    let pct = |used: u64, total: u64| (total > 0).then(|| fmt::pct(used as f32 * 100.0 / total as f32));
    match m {
        Module::Cpu => snap.cpu.as_ref().map(|c| fmt::pct(c.total)),
        Module::Memory => snap.memory.as_ref().and_then(|mem| pct(mem.used, mem.total)),
        Module::Gpu => select::busiest_gpu(snap).map(|(_, g)| fmt::pct(g.util_pct)),
        Module::Battery => snap.battery.as_ref().map(|b| fmt::pct(b.percent)),
        Module::Network => select::net_rates(snap, cfg).map(|(rx, _)| fmt::rate_in(rx, cfg.options.network.units)),
        Module::Disk => select::disk_volume(snap, cfg).and_then(|v| pct(v.total.saturating_sub(v.free), v.total)),
        Module::Sensors => select::taskbar_sensor(snap, cfg).map(|s| fmt::sensor(s.value, s.kind, cfg.temp_unit)),
        Module::Processes => None,
    }
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

#[cfg(test)]
mod tests {
    use super::{Body, Cell, sample};
    use crate::theme::Color;
    use busy_core::{Config, CpuInfo, MemInfo, Module, Snapshot};

    fn cell(module: Module, body: Body<'static>) -> Cell<'static> {
        Cell { module, label: None, body }
    }

    #[test]
    fn samples_are_one_value_whatever_the_style() {
        let cfg = Config::default();
        let snap = Snapshot {
            cpu: Some(CpuInfo { total: 42.0, ..CpuInfo::default() }),
            memory: Some(MemInfo { total: 8, used: 2, ..MemInfo::default() }),
            ..Snapshot::default()
        };
        assert_eq!(sample(&snap, &cfg, Module::Cpu).as_deref(), Some("42%"));
        assert_eq!(sample(&snap, &cfg, Module::Memory).as_deref(), Some("25%"));
        // No data yet, or no reading of its own.
        assert_eq!(sample(&snap, &cfg, Module::Battery), None);
        assert_eq!(sample(&snap, &cfg, Module::Processes), None);
        let empty = Snapshot { memory: Some(MemInfo::default()), ..Snapshot::default() };
        assert_eq!(sample(&empty, &cfg, Module::Memory), None);
    }

    #[test]
    fn tips_name_the_module_and_its_reading() {
        let c = Color::default();
        let text = cell(Module::Memory, Body::Text { value: "35%".into(), color: c });
        assert_eq!(text.tip(), "Memory: 35%");
        let io = cell(Module::Network, Body::Io { rows: [("↑", c, "24 KB/s".into()), ("↓", c, "3 MB/s".into())] });
        assert_eq!(io.tip(), "Network: ↑ 24 KB/s  ↓ 3 MB/s");
        // Design: a Network graph has no value, so the tip is the name alone.
        let graph = cell(Module::Network, Body::Graph { value: String::new(), lines: Vec::new(), max: 1.0 });
        assert_eq!(graph.tip(), "Network");
    }
}
