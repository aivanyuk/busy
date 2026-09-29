//! The flyout's layout primitives: a vertical cursor that measures (no canvas) or measures and draws.

use crate::ctx::Ctx;
use crate::fmt;
use crate::history::Series;
use crate::render::{Align, Canvas, Rect};
use crate::theme::{Color, Theme, alpha};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

pub(super) struct Fonts {
    pub(super) title: IDWriteTextFormat,
    pub(super) body: IDWriteTextFormat,
    pub(super) bold: IDWriteTextFormat,
    pub(super) small: IDWriteTextFormat,
    pub(super) hint: IDWriteTextFormat,
}

pub(super) struct Painter<'a> {
    pub(super) cv: Option<&'a Canvas<'a>>,
    pub(super) ctx: &'a Ctx<'a>,
    pub(super) f: &'a Fonts,
    pub(super) x: f32,
    pub(super) w: f32,
    pub(super) y: f32,
    pub(super) mouse: Option<(f32, f32)>,
    pub(super) hits: Vec<(Rect, usize)>,
    pub(super) tab: usize,
}

pub(super) type Fmt<'a> = &'a dyn Fn(f32) -> String;

impl Painter<'_> {
    pub(super) fn t(&self) -> &Theme {
        self.ctx.theme
    }

    pub(super) fn text(&self, s: &str, f: &IDWriteTextFormat, r: Rect, c: Color, a: Align) {
        if let Some(cv) = self.cv {
            cv.text(s, f, r, c, a);
        }
    }

    pub(super) fn header(&mut self, title: &str, value: &str, c: Color) {
        let r = Rect::new(self.x, self.y, self.w, 22.0);
        self.text(title, &self.f.title, r, self.t().text, Align::Left);
        self.text(value, &self.f.title, r, c, Align::Right);
        self.y += 24.0;
    }

    pub(super) fn sub(&mut self, s: &str) {
        self.text(s, &self.f.small, Rect::new(self.x, self.y, self.w, 16.0), self.t().secondary, Align::Left);
        self.y += 18.0;
    }

    pub(super) fn group(&mut self, s: &str) {
        self.y += 4.0;
        self.text(s, &self.f.bold, Rect::new(self.x, self.y, self.w, 18.0), self.t().text, Align::Left);
        self.y += 19.0;
    }

    pub(super) fn gap(&mut self, h: f32) {
        self.y += h;
    }

    pub(super) fn separator(&mut self) {
        self.y += 12.0;
        if let Some(cv) = self.cv {
            cv.fill(Rect::new(self.x, self.y, self.w, 1.0), self.t().separator);
        }
        self.y += 13.0;
    }

    /// Two-column label/value grid.
    pub(super) fn kv(&mut self, items: &[(&str, String)]) {
        let cw = (self.w - 20.0) / 2.0;
        for (i, (k, v)) in items.iter().enumerate() {
            let r = Rect::new(self.x + (i % 2) as f32 * (cw + 20.0), self.y, cw, 19.0);
            self.text(k, &self.f.body, r, self.t().secondary, Align::Left);
            self.text(v, &self.f.body, r, self.t().text, Align::Right);
            if i % 2 == 1 || i + 1 == items.len() {
                self.y += 19.0;
            }
        }
    }

    pub(super) fn row(&mut self, left: &str, right: &str, rc: Color) {
        let r = Rect::new(self.x, self.y, self.w, 20.0);
        let rw = self.ctx.gfx.text_width(&self.f.body, right);
        self.text(left, &self.f.body, Rect { w: (self.w - rw - 8.0).max(0.0), ..r }, self.t().text, Align::Left);
        self.text(right, &self.f.body, r, rc, Align::Right);
        self.y += 20.0;
    }

    pub(super) fn bar(&mut self, frac: f32, c: Color) {
        if let Some(cv) = self.cv {
            cv.hbar(Rect::new(self.x, self.y + 1.0, self.w, 5.0), frac, c, self.t().track);
        }
        self.y += 12.0;
    }

    pub(super) fn hint(&mut self, s: &str) {
        let (_, h) = self.ctx.gfx.metrics(&self.f.hint, s, self.w);
        self.text(s, &self.f.hint, Rect::new(self.x, self.y, self.w, h), self.t().secondary, Align::Left);
        self.y += h + 4.0;
    }

    /// History graph with optional hover readout. `max_label` is drawn in the top-right corner.
    pub(super) fn graph(&mut self, series: &[(&Series, Color)], max: f32, h: f32, fv: Fmt, max_label: Option<String>) {
        let r = Rect::new(self.x, self.y, self.w, h);
        self.y += h + 8.0;
        let Some(cv) = self.cv else { return };
        let t = self.t();
        cv.round(r, 4.0, t.track);
        for f in [0.25, 0.5, 0.75] {
            let y = (r.y + r.h * f).round();
            cv.hline(r.x + 4.0, r.right() - 4.0, y, alpha(t.separator, 0.7));
        }
        let inner = r.inset(1.0, 2.0);
        for (s, c) in series {
            cv.graph(inner, s, max, *c, if series.len() > 1 { 0.16 } else { 0.28 }, s.cap());
        }
        if let Some(l) = max_label {
            self.text(&l, &self.f.small, Rect::new(r.x + 6.0, r.y + 2.0, r.w - 12.0, 14.0), t.tertiary, Align::Right);
        }
        let Some((mx, my)) = self.mouse.filter(|&(x, y)| r.contains(x, y)) else { return };
        let Some((s0, _)) = series.first() else { return };
        let step = inner.w / (s0.cap().max(2) - 1) as f32;
        let k = ((inner.right() - mx) / step).round().max(0.0) as usize;
        if k >= s0.len() {
            return;
        }
        let x = inner.right() - k as f32 * step;
        cv.vline(x, r.y + 2.0, r.bottom() - 2.0, t.secondary);
        let secs = k as u64 * self.ctx.cfg.interval_ms as u64 / 1000;
        let mut label = series
            .iter()
            .filter(|(s, _)| k < s.len())
            .map(|(s, _)| fv(s.get(s.len() - 1 - k)))
            .collect::<Vec<_>>()
            .join("  ");
        if secs > 0 {
            label = format!("{label}  · {} ago", fmt::duration(secs));
        }
        let lw = self.ctx.gfx.text_width(&self.f.small, &label) + 12.0;
        let lx = if mx > r.x + r.w / 2.0 { x - lw - 4.0 } else { x + 4.0 };
        let lr = Rect::new(lx.clamp(r.x, r.right() - lw), my.clamp(r.y + 2.0, r.bottom() - 20.0) - 9.0, lw, 18.0);
        cv.round(lr, 4.0, if t.dark { alpha(t.background, 0.92) } else { alpha(crate::theme::rgb(0xFFFFFF), 0.95) });
        self.text(&label, &self.f.small, lr, t.text, Align::Center);
    }

    pub(super) fn cores(&mut self, v: &[f32]) {
        if v.is_empty() {
            return;
        }
        let n = v.len() as f32;
        let gap = if v.len() > 32 { 1.0 } else { 2.0 };
        let bw = ((self.w - gap * (n - 1.0)) / n).min(16.0);
        if let Some(cv) = self.cv {
            let t = self.t();
            for (i, &p) in v.iter().enumerate() {
                cv.vbar(
                    Rect::new(self.x + i as f32 * (bw + gap), self.y, bw, 22.0),
                    p / 100.0,
                    t.level(t.accent, p),
                    t.track,
                );
            }
        }
        self.y += 30.0;
    }

    pub(super) fn tabs(&mut self, names: &[&str]) {
        let r = Rect::new(self.x, self.y, self.w, 24.0);
        let tw = r.w / names.len() as f32;
        let t = *self.t();
        if let Some(cv) = self.cv {
            cv.round(r, 5.0, t.track);
        }
        for (i, name) in names.iter().enumerate() {
            let tr = Rect::new(r.x + i as f32 * tw, r.y, tw, r.h);
            if let Some(cv) = self.cv {
                if i == self.tab {
                    cv.round(tr.inset(2.0, 2.0), 4.0, alpha(t.accent, 0.25));
                } else if self.mouse.is_some_and(|(x, y)| tr.contains(x, y)) {
                    cv.round(tr.inset(2.0, 2.0), 4.0, t.hover);
                }
            }
            let f = if i == self.tab { &self.f.bold } else { &self.f.body };
            self.text(name, f, tr, if i == self.tab { t.text } else { t.secondary }, Align::Center);
            self.hits.push((tr, i));
        }
        self.y += 30.0;
    }

    pub(super) fn meter(&mut self, label: &str, p: f32, c: Color) {
        let r = Rect::new(self.x, self.y, self.w, 18.0);
        let t = *self.t();
        self.text(label, &self.f.small, Rect { w: 60.0, ..r }, t.secondary, Align::Left);
        self.text(&fmt::pct(p), &self.f.small, r, t.secondary, Align::Right);
        if let Some(cv) = self.cv {
            cv.hbar(Rect::new(self.x + 60.0, self.y + 6.5, self.w - 100.0, 5.0), p / 100.0, c, t.track);
        }
        self.y += 20.0;
    }

    pub(super) fn row_kv(&mut self, left: &str, right: &str, rc: Color) {
        let r = Rect::new(self.x, self.y, self.w, 19.0);
        self.text(left, &self.f.body, Rect { w: r.w - 80.0, ..r }, self.t().secondary, Align::Left);
        self.text(right, &self.f.body, r, rc, Align::Right);
        self.y += 19.0;
    }

    /// Two colored rate readouts side by side (download/upload, read/write).
    pub(super) fn rates(&mut self, a: (&str, f64, Color), b: (&str, f64, Color)) {
        let half = self.w / 2.0;
        for (i, (p, v, c)) in [a, b].into_iter().enumerate() {
            let x = self.x + i as f32 * half;
            self.text(p, &self.f.bold, Rect::new(x, self.y, 14.0, 20.0), c, Align::Left);
            self.text(
                &fmt::rate(v),
                &self.f.bold,
                Rect::new(x + 14.0, self.y, half - 14.0, 20.0),
                self.t().text,
                Align::Left,
            );
        }
        self.y += 24.0;
    }

    pub(super) fn missing(&mut self, title: &str) {
        self.header(title, "", self.t().text);
        self.sub("Waiting for data…");
    }
}
