//! Lays out and paints a `Detail` as the design's detailed flyout (`isDetailed`), 360 DIPs wide. Without a
//! canvas it only measures, so the window can be sized before the first paint.

use super::detail::{Chart, Detail};
use crate::fmt;
use crate::render::{Align, Canvas, Gfx, Rect};
use crate::theme::{Color, Theme};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;
use windows::core::Result;

pub(super) const WIDTH: f32 = 360.0;
/// Side padding of every block.
const PAD: f32 = 16.0;
/// Content width.
const W: f32 = WIDTH - 2.0 * PAD;
/// Line boxes of the design's type sizes.
const LINE_11: f32 = 14.0;
const LINE_12: f32 = 16.0;
const LINE_13: f32 = 17.0;
const LINE_14: f32 = 19.0;
const LINE_24: f32 = 30.0;
/// Gap between the header block's items (design `gap: 12px`).
const GAP: f32 = 12.0;
const CHART_H: f32 = 86.0;

pub(super) struct Fonts {
    /// 14 px semibold.
    title: IDWriteTextFormat,
    /// 24 px semibold.
    big: IDWriteTextFormat,
    /// 13 px: stat values.
    stat: IDWriteTextFormat,
    /// 12 px regular and semibold.
    body: IDWriteTextFormat,
    bold: IDWriteTextFormat,
    /// 11 px: captions, stat keys, the chart readout.
    small: IDWriteTextFormat,
    /// 12 px, wrapping: notes.
    note: IDWriteTextFormat,
}

impl Fonts {
    pub(super) fn new(gfx: &Gfx) -> Result<Self> {
        Ok(Self {
            title: gfx.format(14.0, true)?,
            big: gfx.format(24.0, true)?,
            stat: gfx.format(13.0, false)?,
            body: gfx.format(12.0, false)?,
            bold: gfx.format(12.0, true)?,
            small: gfx.format(11.0, false)?,
            note: gfx.wrapping(12.0)?,
        })
    }
}

/// A history chart as laid out, for mapping the pointer to a sample without painting.
#[derive(Clone, Copy)]
pub(super) struct GraphHit {
    pub(super) r: Rect,
    inner: Rect,
    cap: usize,
    len: usize,
}

impl GraphHit {
    pub(super) fn new(r: Rect, inner: Rect, cap: usize, len: usize) -> Self {
        Self { r, inner, cap, len }
    }

    /// X of the sample `k` back from the newest one.
    pub(super) fn x_of(&self, k: usize) -> f32 {
        self.inner.right() - k as f32 * self.inner.w / (self.cap.max(2) - 1) as f32
    }

    /// Samples back from the newest one to the sample nearest `x`, if that sample exists.
    pub(super) fn sample_at(&self, x: f32) -> Option<usize> {
        let step = self.inner.w / (self.cap.max(2) - 1) as f32;
        let k = ((self.inner.right() - x) / step).round().max(0.0) as usize;
        (k < self.len).then_some(k)
    }
}

/// What a layout pass produced.
pub(super) struct Drawn {
    pub(super) height: f32,
    pub(super) graphs: Vec<GraphHit>,
}

struct Pen<'a> {
    cv: Option<&'a Canvas<'a>>,
    gfx: &'a Gfx,
    t: &'a Theme,
    f: &'a Fonts,
    mouse: Option<(f32, f32)>,
    y: f32,
    graphs: Vec<GraphHit>,
}

/// Lays out `d` from the top (scrolled by `scroll`), painting when `cv` is given.
pub(super) fn draw(
    d: &Detail,
    cv: Option<&Canvas>,
    gfx: &Gfx,
    t: &Theme,
    f: &Fonts,
    scroll: f32,
    mouse: Option<(f32, f32)>,
) -> Drawn {
    let mut p = Pen { cv, gfx, t, f, mouse, y: PAD - scroll, graphs: Vec::new() };
    p.header(d);
    if let Some(n) = &d.note {
        p.gap();
        p.note(n);
    }
    if let Some(c) = &d.chart {
        p.gap();
        p.chart(c);
    }
    if !d.seg.is_empty() {
        p.gap();
        p.seg(&d.seg);
    }
    if !d.legend.is_empty() {
        p.gap();
        p.legend(&d.legend);
    }
    if !d.cores.is_empty() {
        p.gap();
        p.cores(&d.cores);
    }
    p.y += 14.0;
    if !d.stats.is_empty() {
        p.stats(&d.stats);
    }
    if !d.bars.is_empty() {
        p.bars(d);
    }
    if !d.procs.is_empty() {
        p.procs(d);
    }
    Drawn { height: p.y + scroll, graphs: p.graphs }
}

impl Pen<'_> {
    fn text(&self, s: &str, f: &IDWriteTextFormat, r: Rect, c: Color, a: Align) {
        if let Some(cv) = self.cv {
            cv.text(s, f, r, c, a);
        }
    }

    fn gap(&mut self) {
        self.y += GAP;
    }

    /// Title and hardware line on the left, the big value and its label on the right.
    fn header(&mut self, d: &Detail) {
        let t = self.t;
        let big_w = if d.big.is_empty() { 0.0 } else { self.gfx.text_width(&self.f.big, &d.big) };
        let label_w = if d.big_label.is_empty() { 0.0 } else { self.gfx.text_width(&self.f.body, &d.big_label) };
        let right_w = big_w.max(label_w);
        let left_w = (W - right_w - GAP).max(0.0);
        let y = self.y;
        self.text(d.title, &self.f.title, Rect::new(PAD, y, left_w, LINE_14), t.fg, Align::Left);
        if !d.sub.is_empty() {
            self.text(&d.sub, &self.f.body, Rect::new(PAD, y + LINE_14 + 2.0, left_w, LINE_12), t.fg2, Align::Left);
        }
        let right = Rect::new(PAD + W - right_w, y, right_w, LINE_24);
        self.text(&d.big, &self.f.big, right, t.fg, Align::Right);
        self.text(&d.big_label, &self.f.body, Rect { y: y + LINE_24, h: LINE_12, ..right }, t.fg3, Align::Right);
        let left_h = if d.sub.is_empty() { LINE_14 } else { LINE_14 + 2.0 + LINE_12 };
        let right_h = if d.big.is_empty() { 0.0 } else { LINE_24 + if d.big_label.is_empty() { 0.0 } else { LINE_12 } };
        self.y += left_h.max(right_h);
    }

    fn note(&mut self, s: &str) {
        let (_, h) = self.gfx.metrics(&self.f.note, s, W);
        self.text(s, &self.f.note, Rect::new(PAD, self.y, W, h), self.t.fg2, Align::Left);
        self.y += h;
    }

    /// 86-DIP chart on `well` with dashed grid lines at 25/50/75 %, the span and scale below it, and the
    /// hovered sample's readout.
    fn chart(&mut self, c: &Chart) {
        let t = self.t;
        let r = Rect::new(PAD, self.y, W, CHART_H);
        let inner = r.inset(1.0, 1.0);
        let hit = c.lines.first().map(|(s, _)| GraphHit::new(r, inner, s.cap(), s.len()));
        self.graphs.extend(hit);
        if let Some(cv) = self.cv {
            cv.round(r, 4.0, t.well);
            cv.round_outline(r, 4.0, t.line);
            cv.clip(inner);
            for frac in [0.25, 0.5, 0.75] {
                cv.dashed_hline(inner.x, inner.right(), (inner.y + inner.h * frac).round(), t.grid);
            }
            for (i, (s, col)) in c.lines.iter().enumerate() {
                cv.graph(inner, s, c.max, *col, if i == 0 { 0.22 } else { 0.0 }, 1.5, s.cap());
            }
            cv.unclip();
            self.readout(c, r, hit);
        }
        self.y += CHART_H + 4.0;
        let row = Rect::new(PAD, self.y, W, LINE_11);
        self.text(&c.span, &self.f.small, row, t.fg3, Align::Left);
        self.text(&c.max_label, &self.f.small, row, t.fg3, Align::Right);
        self.y += LINE_11;
    }

    /// Vertical line on the sample under the pointer, with its value(s) and age.
    fn readout(&self, c: &Chart, r: Rect, hit: Option<GraphHit>) {
        let (Some(cv), t) = (self.cv, self.t) else { return };
        let Some((mx, my)) = self.mouse.filter(|&(x, y)| r.contains(x, y)) else { return };
        let Some((hit, k)) = hit.and_then(|h| Some((h, h.sample_at(mx)?))) else { return };
        let x = hit.x_of(k);
        cv.vline(x, r.y + 2.0, r.bottom() - 2.0, t.fg2);
        let secs = k as u64 * c.lines.first().map_or(0, |(s, _)| s.interval_ms()) as u64 / 1000;
        let mut label = c
            .lines
            .iter()
            .filter(|(s, _)| k < s.len())
            .map(|(s, _)| c.value.format(s.get(s.len() - 1 - k)))
            .collect::<Vec<_>>()
            .join("  ");
        if secs > 0 {
            label = format!("{label}  · {} ago", fmt::duration(secs));
        }
        let lw = self.gfx.text_width(&self.f.small, &label) + 12.0;
        // Placed from the sample, not the pointer: the readout repaints only when the sample changes.
        let lx = if x > r.x + r.w / 2.0 { x - lw - 4.0 } else { x + 4.0 };
        let lr = Rect::new(lx.clamp(r.x, r.right() - lw), my.clamp(r.y + 2.0, r.bottom() - 20.0) - 9.0, lw, 18.0);
        cv.round(lr, 4.0, Color { a: 0.95, ..t.fly });
        cv.text(&label, &self.f.small, lr, t.fg, Align::Center);
    }

    /// 8-DIP composition bar, segments 2 DIPs apart, rounded at the ends.
    fn seg(&mut self, seg: &[(f32, Color)]) {
        let total: f32 = seg.iter().map(|s| s.0.max(0.0)).sum();
        if let Some(cv) = self.cv.filter(|_| total > 0.0) {
            let shown: Vec<_> = seg.iter().filter(|s| s.0 > 0.0).collect();
            let room = W - 2.0 * shown.len().saturating_sub(1) as f32;
            let mut x = PAD;
            for (i, (share, col)) in shown.iter().enumerate() {
                let w = room * share / total;
                let r = Rect::new(x, self.y, w, 8.0);
                cv.round(r, (w / 2.0).min(4.0), *col);
                // Square the inner ends: only the bar's outer corners are rounded.
                if i > 0 {
                    cv.fill(Rect { w: (w / 2.0).min(4.0), ..r }, *col);
                }
                if i + 1 < shown.len() {
                    let sq = (w / 2.0).min(4.0);
                    cv.fill(Rect { x: r.right() - sq, w: sq, ..r }, *col);
                }
                x += w + 2.0;
            }
        }
        self.y += 8.0;
    }

    /// Swatch, label and value per item, wrapping; 16 DIPs between items, 6 between rows.
    fn legend(&mut self, items: &[(String, String, Color)]) {
        let (mut x, mut y) = (PAD, self.y);
        for (label, value, col) in items {
            let (lw, vw) = (self.gfx.text_width(&self.f.body, label), self.gfx.text_width(&self.f.bold, value));
            let w = 8.0 + 6.0 + lw + 6.0 + vw;
            if x > PAD && x + w > PAD + W {
                (x, y) = (PAD, y + LINE_12 + 6.0);
            }
            if let Some(cv) = self.cv {
                cv.round(Rect::new(x, y + 4.0, 8.0, 8.0), 2.0, *col);
            }
            self.text(label, &self.f.body, Rect::new(x + 14.0, y, lw, LINE_12), self.t.fg2, Align::Left);
            self.text(value, &self.f.bold, Rect::new(x + 20.0 + lw, y, vw, LINE_12), self.t.fg, Align::Left);
            x += w + 16.0;
        }
        self.y = y + LINE_12;
    }

    /// "Logical processors" over a 16-column grid of bars, 36 DIPs high in all.
    fn cores(&mut self, cores: &[(f32, Color)]) {
        self.text("Logical processors", &self.f.small, Rect::new(PAD, self.y, W, LINE_11), self.t.fg3, Align::Left);
        self.y += LINE_11 + 6.0;
        let rows = cores.len().div_ceil(16);
        let cw = (W - 3.0 * 15.0) / 16.0;
        let rh = (36.0 - 3.0 * (rows - 1) as f32) / rows as f32;
        if let Some(cv) = self.cv {
            for (i, (pct, col)) in cores.iter().enumerate() {
                let (c, r) = ((i % 16) as f32, (i / 16) as f32);
                cv.vbar(
                    Rect::new(PAD + c * (cw + 3.0), self.y + r * (rh + 3.0), cw, rh),
                    pct / 100.0,
                    *col,
                    self.t.track,
                );
            }
        }
        self.y += 36.0;
    }

    /// A block under a full-width `line`, padded 12 at the top.
    fn block(&mut self) {
        if let Some(cv) = self.cv {
            cv.fill(Rect::new(0.0, self.y, WIDTH, 1.0), self.t.line);
        }
        self.y += 1.0 + 12.0;
    }

    /// Two columns of key (11 px, `fg3`) over value (13 px).
    fn stats(&mut self, stats: &[(String, String)]) {
        self.block();
        let cw = (W - 16.0) / 2.0;
        let row_h = LINE_11 + 1.0 + LINE_13;
        for (i, (k, v)) in stats.iter().enumerate() {
            let (x, y) = (PAD + (i % 2) as f32 * (cw + 16.0), self.y + (i / 2) as f32 * (row_h + 10.0));
            self.text(k, &self.f.small, Rect::new(x, y, cw, LINE_11), self.t.fg3, Align::Left);
            self.text(v, &self.f.stat, Rect::new(x, y + LINE_11 + 1.0, cw, LINE_13), self.t.fg, Align::Left);
        }
        let rows = stats.len().div_ceil(2);
        self.y += rows as f32 * row_h + rows.saturating_sub(1) as f32 * 10.0 + 12.0;
    }

    /// Titled list of label/text rows over 4-DIP bars, 10 DIPs apart.
    fn bars(&mut self, d: &Detail) {
        self.block();
        let t = self.t;
        self.text(d.bars_title, &self.f.bold, Rect::new(PAD, self.y, W, LINE_12), t.fg, Align::Left);
        self.y += LINE_12;
        for b in &d.bars {
            self.y += 10.0;
            let row = Rect::new(PAD, self.y, W, LINE_12);
            let tw = self.gfx.text_width(&self.f.body, &b.text);
            self.text(&b.label, &self.f.body, Rect { w: (W - tw - 8.0).max(0.0), ..row }, t.fg, Align::Left);
            self.text(&b.text, &self.f.body, row, t.fg2, Align::Right);
            self.y += LINE_12 + 4.0;
            if let Some(cv) = self.cv {
                cv.hbar(Rect::new(PAD, self.y, W, 4.0), b.frac, b.color, t.track);
            }
            self.y += 4.0;
        }
        self.y += 12.0;
    }

    /// Titled process rows: icon tile, name, value.
    fn procs(&mut self, d: &Detail) {
        self.block();
        let t = self.t;
        self.text(d.procs_title, &self.f.bold, Rect::new(PAD, self.y, W, LINE_12), t.fg, Align::Left);
        self.y += LINE_12 + 4.0;
        for (name, value) in &d.procs {
            self.y += 2.0;
            if let Some(cv) = self.cv {
                cv.round(Rect::new(PAD, self.y + 5.0, 16.0, 16.0), 4.0, t.tile);
            }
            let vw = self.gfx.text_width(&self.f.body, value);
            let row = Rect::new(PAD + 26.0, self.y, W - 26.0, 26.0);
            self.text(name, &self.f.body, Rect { w: (row.w - vw - 10.0).max(0.0), ..row }, t.fg, Align::Left);
            self.text(value, &self.f.body, row, t.fg2, Align::Right);
            self.y += 26.0;
        }
        self.y += 10.0;
    }
}

#[cfg(test)]
mod tests {
    use super::GraphHit;
    use crate::render::Rect;

    #[test]
    fn sample_at_counts_back_from_the_newest_sample() {
        let r = Rect::new(0.0, 0.0, 102.0, 40.0);
        // One DIP per sample: inner is 100 wide and spans 101 samples.
        let g = GraphHit { r, inner: r.inset(1.0, 2.0), cap: 101, len: 50 };
        assert_eq!(g.sample_at(101.0), Some(0));
        assert_eq!(g.sample_at(105.0), Some(0));
        assert_eq!(g.sample_at(90.4), Some(11));
        assert_eq!(g.sample_at(52.0), Some(49));
        assert_eq!(g.sample_at(51.0), None);
    }
}
