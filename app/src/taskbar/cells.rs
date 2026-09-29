//! What the widget shows: one cell per enabled module, measured and drawn in its configured style.

use crate::ctx::Ctx;
use crate::history::Series;
use crate::render::{Align, Canvas, Gfx, Rect, nice_max};
use crate::theme::{Color, rgba};
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

#[derive(Clone, PartialEq)]
enum Val {
    One(String),
    /// Two stacked (prefix, prefix color, text) rows, e.g. upload/download.
    Two([(&'static str, Color, String); 2]),
}

pub(super) struct Cell<'a> {
    style: CellStyle,
    label: String,
    val: Val,
    /// Widest strings the value can take, so the cell doesn't jitter.
    worst: &'static [&'static str],
    short: String,
    series: Vec<(&'a Series, Color)>,
    max: f32,
    bars: Vec<(f32, Color)>,
}

/// What a cell's pixels depend on, owned so the widget can compare it with the last drawn frame.
#[derive(PartialEq)]
pub(super) struct Key {
    style: CellStyle,
    label: String,
    val: Option<Val>,
    short: String,
    /// Graph cells: (samples pushed, samples held, color) per series; the samples themselves never change.
    series: Vec<(u64, usize, Color)>,
    max: f32,
    /// Bar cells: fill in 1/256 steps, under half a device pixel of the 30-DIP bar up to 400 % scale.
    bars: Vec<(u8, Color)>,
}

const PCT: &[&str] = &["100%"];
const RATES: &[&str] = &["99.9 MB/s", "999 MB/s", "99.9 KB/s", "999 KB/s"];

pub(super) fn cells<'a>(ctx: &Ctx<'a>) -> Vec<Cell<'a>> {
    let (snap, hist, t) = (ctx.snap, ctx.hist, ctx.theme);
    let mut out = Vec::new();
    for mc in ctx.cfg.modules.iter().filter(|m| m.taskbar) {
        let base = |label: &str, val, worst, short: String| Cell {
            style: mc.style,
            label: label.into(),
            val,
            worst,
            short,
            series: Vec::new(),
            max: 100.0,
            bars: Vec::new(),
        };
        let cell = match mc.module {
            Module::Cpu => snap.cpu.as_ref().map(|c| Cell {
                series: vec![(&hist.cpu, t.accent)],
                bars: vec![(c.total / 100.0, t.level(t.accent, c.total))],
                ..base("CPU", Val::One(fmt::pct(c.total)), PCT, fmt::pct(c.total))
            }),
            Module::Memory => snap.memory.as_ref().filter(|m| m.total > 0).map(|m| {
                let p = m.used as f32 * 100.0 / m.total as f32;
                Cell {
                    series: vec![(&hist.mem, t.mem)],
                    bars: vec![(p / 100.0, t.level(t.mem, p))],
                    ..base("MEM", Val::One(fmt::pct(p)), PCT, fmt::pct(p))
                }
            }),
            Module::Gpu => {
                snap.gpus.iter().enumerate().max_by(|a, b| a.1.util_pct.total_cmp(&b.1.util_pct)).map(|(i, g)| Cell {
                    series: hist.gpus.get(i).map(|s| vec![(s, t.gpu)]).unwrap_or_default(),
                    bars: snap.gpus.iter().take(2).map(|g| (g.util_pct / 100.0, t.level(t.gpu, g.util_pct))).collect(),
                    ..base("GPU", Val::One(fmt::pct(g.util_pct)), PCT, fmt::pct(g.util_pct))
                })
            }
            Module::Network => snap.net.as_ref().map(|n| {
                let max = nice_max(hist.net_rx.max().max(hist.net_tx.max()));
                Cell {
                    series: vec![(&hist.net_rx, t.rx), (&hist.net_tx, t.tx)],
                    max,
                    bars: vec![(n.rx_bps as f32 / max, t.rx), (n.tx_bps as f32 / max, t.tx)],
                    ..base(
                        "NET",
                        Val::Two([("↑", t.tx, fmt::rate(n.tx_bps)), ("↓", t.rx, fmt::rate(n.rx_bps))]),
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
                    series: vec![(&hist.disk_r, t.rx), (&hist.disk_w, t.tx)],
                    max,
                    bars: vec![(r as f32 / max, t.rx), (w as f32 / max, t.tx)],
                    ..base(
                        "DISK",
                        Val::Two([("R", t.rx, fmt::rate(r)), ("W", t.tx, fmt::rate(w))]),
                        RATES,
                        fmt::rate_short(r + w),
                    )
                }
            }),
            Module::Battery => snap.battery.as_ref().map(|b| {
                let v = format!("{}{}", fmt::pct(b.percent), if b.charging { "⚡" } else { "" });
                let c = if b.percent < 20.0 && !b.charging { t.crit } else { t.battery };
                Cell {
                    series: vec![(&hist.battery, c)],
                    bars: vec![(b.percent / 100.0, c)],
                    ..base("BAT", Val::One(v.clone()), &["100%⚡"], v)
                }
            }),
            Module::Sensors => select::pinned_sensor(snap, ctx.cfg).map(|s| {
                let v = fmt::sensor(s.value, s.kind, ctx.cfg.temp_unit);
                let is_temp = s.kind == busy_core::SensorKind::Temperature;
                let max = if is_temp { 100.0 } else { nice_max(hist.sensor.max()) };
                let label =
                    if is_temp { "TEMP".into() } else { s.name.chars().take(6).collect::<String>().to_uppercase() };
                let col = if is_temp { t.temp(s.value) } else { t.text };
                let bar_col = if is_temp && col == t.text { t.accent } else { col };
                Cell {
                    series: vec![(&hist.sensor, bar_col)],
                    max,
                    bars: vec![(s.value / max, bar_col)],
                    ..base(&label, Val::One(v.clone()), &["100°C", "212°F", "8888 rpm", "888.8 W"], v)
                }
            }),
            Module::Processes => snap.top.by_cpu.first().map(|p| {
                let name: String = p.name.trim_end_matches(".exe").chars().take(12).collect();
                Cell {
                    bars: vec![(p.cpu_pct / 100.0, t.accent)],
                    ..base(&name, Val::One(fmt::pct(p.cpu_pct)), &["100%", "WWWWWWWW"], fmt::pct(p.cpu_pct))
                }
            }),
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

    /// Only what the cell's style draws goes in, so e.g. a Text cell ignores its history.
    pub(super) fn key(&self) -> Key {
        let graph = self.style == CellStyle::Graph;
        let bar = self.style == CellStyle::Bar;
        Key {
            style: self.style,
            label: self.label.clone(),
            val: (!graph).then(|| self.val.clone()),
            short: if graph { self.short.clone() } else { String::new() },
            series: if graph {
                self.series.iter().map(|(s, c)| (s.pushed(), s.len(), *c)).collect()
            } else {
                Vec::new()
            },
            max: if graph { self.max } else { 0.0 },
            bars: if bar {
                self.bars.iter().map(|(f, c)| ((f.clamp(0.0, 1.0) * 255.0).round() as u8, *c)).collect()
            } else {
                Vec::new()
            },
        }
    }

    fn bars_width(&self) -> f32 {
        self.bars.len() as f32 * (BAR_W + 2.0) - 2.0
    }

    pub(super) fn width(&self, gfx: &Gfx, f: &Fonts) -> f32 {
        match self.style {
            CellStyle::Text => self.text_width(gfx, f),
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
                cv.text(&self.label, &f.label, Rect::new(r.x, y0, r.w, 13.0), t.secondary, Align::Left);
                cv.text(v, &f.value, Rect::new(r.x, y0 + 12.0, r.w, 18.0), t.text, Align::Left);
            }
            Val::Two(rows) => {
                let pw = rows.iter().map(|row| ctx.gfx.text_width(&f.pair, row.0)).fold(0.0, f32::max);
                for (i, (prefix, col, text)) in rows.iter().enumerate() {
                    let y = y0 + i as f32 * 15.0;
                    cv.text(prefix, &f.pair, Rect::new(r.x, y, pw, 15.0), *col, Align::Left);
                    cv.text(text, &f.pair, Rect::new(r.x + pw + 3.0, y, r.w - pw - 3.0, 15.0), t.text, Align::Left);
                }
            }
        }
    }

    pub(super) fn draw(&self, cv: &Canvas, f: &Fonts, ctx: &Ctx, r: Rect) {
        let t = ctx.theme;
        match self.style {
            CellStyle::Text => self.draw_text(cv, f, ctx, r),
            CellStyle::Graph => {
                let y0 = (r.h - 32.0) / 2.0;
                cv.text(&self.label, &f.label, Rect::new(r.x, y0, r.w, 13.0), t.secondary, Align::Left);
                cv.text(&self.short, &f.tiny, Rect::new(r.x, y0, r.w, 13.0), t.text, Align::Right);
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
                    cv.vbar(Rect::new(r.x + i as f32 * (BAR_W + 2.0), y0, BAR_W, bh), *frac, *c, rgba(0x808080, 0.28));
                }
                let dx = self.bars_width() + 6.0;
                self.draw_text(cv, f, ctx, Rect::new(r.x + dx, r.y, r.w - dx, r.h));
            }
        }
    }
}
