//! What the widget shows: one cell per enabled module, measured and drawn in its configured style.

use crate::ctx::Ctx;
use crate::history::Series;
use crate::render::{Align, Canvas, Gfx, Rect, nice_max};
use crate::theme::Color;
use crate::tone::{self, SECOND};
use crate::{fmt, select};
use busy_core::{CellStyle, Module};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

const GRAPH_W: f32 = 54.0;
const BAR_W: f32 = 4.0;

pub(super) struct Fonts {
    pub(super) label: IDWriteTextFormat,
    pub(super) value: IDWriteTextFormat,
    pub(super) pair: IDWriteTextFormat,
    pub(super) tiny: IDWriteTextFormat,
}

enum Val {
    One(String),
    /// Two stacked (prefix, prefix color, text) rows, e.g. upload/download.
    Two([(&'static str, Color, String); 2]),
}

pub(super) struct Cell<'a> {
    style: CellStyle,
    label: String,
    val: Val,
    /// Color of a `Val::One` value (design `valueColor`).
    value_color: Color,
    /// Widest strings the value can take, so the cell doesn't jitter.
    worst: &'static [&'static str],
    short: String,
    series: Vec<(&'a Series, Color)>,
    max: f32,
    bars: Vec<(f32, Color)>,
}

const PCT: &[&str] = &["100%"];
const RATES: &[&str] = &["99.9 MB/s", "999 MB/s", "99.9 KB/s", "999 KB/s"];

pub(super) fn cells<'a>(ctx: &Ctx<'a>) -> Vec<Cell<'a>> {
    let (snap, hist, t) = (ctx.snap, ctx.hist, ctx.theme);
    let mut out = Vec::new();
    for mc in ctx.cfg.modules.iter().filter(|m| m.taskbar) {
        // Design `widget()`: graphs and bars in the module color (or by load), values in `fg` (or by load);
        // rates pair the module color (download, read) with `SECOND` (upload, write).
        let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
        let fill = |pct| t.color(tone::fill(mc, pct));
        let value = |pct| t.color(tone::value(mc, pct));
        let base = |label: &str, val, worst, short: String| Cell {
            style: mc.style,
            label: label.into(),
            val,
            value_color: t.fg,
            worst,
            short,
            series: Vec::new(),
            max: 100.0,
            bars: Vec::new(),
        };
        let cell = match mc.module {
            Module::Cpu => snap.cpu.as_ref().map(|c| Cell {
                series: vec![(&hist.cpu, fill(c.total))],
                bars: vec![(c.total / 100.0, fill(c.total))],
                value_color: value(c.total),
                ..base("CPU", Val::One(fmt::pct(c.total)), PCT, fmt::pct(c.total))
            }),
            Module::Memory => snap.memory.as_ref().filter(|m| m.total > 0).map(|m| {
                let p = m.used as f32 * 100.0 / m.total as f32;
                Cell {
                    series: vec![(&hist.mem, fill(p))],
                    bars: vec![(p / 100.0, fill(p))],
                    value_color: value(p),
                    ..base("MEM", Val::One(fmt::pct(p)), PCT, fmt::pct(p))
                }
            }),
            Module::Gpu => {
                snap.gpus.iter().enumerate().max_by(|a, b| a.1.util_pct.total_cmp(&b.1.util_pct)).map(|(i, g)| Cell {
                    series: hist.gpus.get(i).map(|s| vec![(s, fill(g.util_pct))]).unwrap_or_default(),
                    bars: snap.gpus.iter().take(2).map(|g| (g.util_pct / 100.0, fill(g.util_pct))).collect(),
                    value_color: value(g.util_pct),
                    ..base("GPU", Val::One(fmt::pct(g.util_pct)), PCT, fmt::pct(g.util_pct))
                })
            }
            Module::Network => snap.net.as_ref().map(|n| {
                let max = nice_max(hist.net_rx.max().max(hist.net_tx.max()));
                Cell {
                    series: vec![(&hist.net_rx, color), (&hist.net_tx, second)],
                    max,
                    bars: vec![(n.rx_bps as f32 / max, color), (n.tx_bps as f32 / max, second)],
                    ..base(
                        "NET",
                        Val::Two([("↑", second, fmt::rate(n.tx_bps)), ("↓", color, fmt::rate(n.rx_bps))]),
                        RATES,
                        format!("↓{}", fmt::rate_short(n.rx_bps)),
                    )
                }
            }),
            Module::Disk => (!snap.disks.is_empty()).then(|| {
                let (r, w) = (
                    snap.disks.iter().map(|d| d.read_bps).sum::<f64>(),
                    snap.disks.iter().map(|d| d.write_bps).sum::<f64>(),
                );
                let max = nice_max(hist.disk_r.max().max(hist.disk_w.max()));
                Cell {
                    series: vec![(&hist.disk_r, color), (&hist.disk_w, second)],
                    max,
                    bars: vec![(r as f32 / max, color), (w as f32 / max, second)],
                    ..base(
                        "DISK",
                        Val::Two([("R", color, fmt::rate(r)), ("W", second, fmt::rate(w))]),
                        RATES,
                        fmt::rate_short(r + w),
                    )
                }
            }),
            Module::Battery => snap.battery.as_ref().map(|b| {
                let v = format!("{}{}", fmt::pct(b.percent), if b.charging { "⚡" } else { "" });
                let (fill, value) = tone::battery(mc, b);
                let c = t.color(fill);
                Cell {
                    series: vec![(&hist.battery, c)],
                    bars: vec![(b.percent / 100.0, c)],
                    value_color: t.color(value),
                    ..base("BAT", Val::One(v.clone()), &["100%⚡"], v)
                }
            }),
            Module::Sensors => select::taskbar_sensor(snap, ctx.cfg).map(|s| {
                let v = fmt::sensor(s.value, s.kind, ctx.cfg.temp_unit);
                let is_temp = s.kind == busy_core::SensorKind::Temperature;
                let max = if is_temp { 100.0 } else { nice_max(hist.sensor.max()) };
                let label =
                    if is_temp { "TEMP".into() } else { s.name.chars().take(6).collect::<String>().to_uppercase() };
                // Load coloring reads a temperature in °C as percent, like the design.
                let (c, vc) = if is_temp { (fill(s.value), value(s.value)) } else { (color, t.fg) };
                Cell {
                    series: vec![(&hist.sensor, c)],
                    max,
                    bars: vec![(s.value / max, c)],
                    value_color: vc,
                    ..base(&label, Val::One(v.clone()), &["100°C", "212°F", "8888 rpm", "888.8 W"], v)
                }
            }),
            // Flyout-only: `Config::normalize` never leaves it on the taskbar.
            Module::Processes => None,
        };
        out.extend(cell);
    }
    out
}

impl Cell<'_> {
    fn text_width(&self, gfx: &Gfx, f: &Fonts) -> f32 {
        let worst = |fmt: &IDWriteTextFormat| self.worst.iter().map(|s| gfx.text_width(fmt, s)).fold(0.0, f32::max);
        match &self.val {
            Val::One(v) => gfx.text_width(&f.label, &self.label).max(worst(&f.value)).max(gfx.text_width(&f.value, v)),
            Val::Two(rows) => {
                let prefix = rows.iter().map(|r| gfx.text_width(&f.pair, r.0)).fold(0.0, f32::max);
                prefix
                    + 3.0
                    + worst(&f.pair).max(rows.iter().map(|r| gfx.text_width(&f.pair, &r.2)).fold(0.0, f32::max))
            }
        }
    }

    fn bars_width(&self) -> f32 {
        self.bars.len() as f32 * (BAR_W + 2.0) - 2.0
    }

    pub(super) fn width(&self, gfx: &Gfx, f: &Fonts) -> f32 {
        match self.style {
            CellStyle::Text | CellStyle::Io => self.text_width(gfx, f),
            CellStyle::Graph => {
                GRAPH_W.max(gfx.text_width(&f.label, &self.label) + gfx.text_width(&f.tiny, &self.short) + 6.0)
            }
            CellStyle::Bar => self.bars_width() + 6.0 + self.text_width(gfx, f),
        }
        .ceil()
    }

    fn draw_text(&self, cv: &Canvas, f: &Fonts, ctx: &Ctx, r: Rect) {
        let t = ctx.theme;
        let y0 = (r.h - 30.0) / 2.0;
        match &self.val {
            Val::One(v) => {
                cv.text(&self.label, &f.label, Rect::new(r.x, y0, r.w, 13.0), t.fg3, Align::Left);
                cv.text(v, &f.value, Rect::new(r.x, y0 + 12.0, r.w, 18.0), self.value_color, Align::Left);
            }
            Val::Two(rows) => {
                let pw = rows.iter().map(|row| ctx.gfx.text_width(&f.pair, row.0)).fold(0.0, f32::max);
                for (i, (prefix, col, text)) in rows.iter().enumerate() {
                    let y = y0 + i as f32 * 15.0;
                    cv.text(prefix, &f.pair, Rect::new(r.x, y, pw, 15.0), *col, Align::Left);
                    cv.text(text, &f.pair, Rect::new(r.x + pw + 3.0, y, r.w - pw - 3.0, 15.0), t.fg, Align::Left);
                }
            }
        }
    }

    pub(super) fn draw(&self, cv: &Canvas, f: &Fonts, ctx: &Ctx, r: Rect) {
        let t = ctx.theme;
        match self.style {
            CellStyle::Text | CellStyle::Io => self.draw_text(cv, f, ctx, r),
            CellStyle::Graph => {
                let y0 = (r.h - 32.0) / 2.0;
                cv.text(&self.label, &f.label, Rect::new(r.x, y0, r.w, 13.0), t.fg3, Align::Left);
                cv.text(&self.short, &f.tiny, Rect::new(r.x, y0, r.w, 13.0), self.value_color, Align::Right);
                let g = Rect::new(r.x, y0 + 15.0, r.w, 17.0);
                cv.round(g, 3.0, t.track);
                let inner = g.inset(1.0, 1.5);
                for (s, c) in &self.series {
                    // ~2 DIPs per sample: the full history would be unreadably dense at this size.
                    cv.graph(
                        inner,
                        s,
                        self.max,
                        *c,
                        if self.series.len() > 1 { 0.18 } else { 0.3 },
                        (inner.w / 2.0) as usize,
                    );
                }
            }
            CellStyle::Bar => {
                let bh = 30.0;
                let y0 = (r.h - bh) / 2.0;
                for (i, (frac, c)) in self.bars.iter().enumerate() {
                    cv.vbar(Rect::new(r.x + i as f32 * (BAR_W + 2.0), y0, BAR_W, bh), *frac, *c, t.track);
                }
                let dx = self.bars_width() + 6.0;
                self.draw_text(cv, f, ctx, Rect::new(r.x + dx, r.y, r.w - dx, r.h));
            }
        }
    }
}
