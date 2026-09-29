//! The flyout's layout primitives: a vertical cursor that measures (no canvas) or measures and draws.

use super::draw::GraphHit;
use crate::ctx::Ctx;
use crate::render::{Align, Canvas, Rect};
use crate::theme::{Color, Theme};
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
    pub(super) graphs: Vec<GraphHit>,
    pub(super) tab: usize,
}

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
        self.text(title, &self.f.title, r, self.t().fg, Align::Left);
        self.text(value, &self.f.title, r, c, Align::Right);
        self.y += 24.0;
    }

    pub(super) fn sub(&mut self, s: &str) {
        self.text(s, &self.f.small, Rect::new(self.x, self.y, self.w, 16.0), self.t().fg2, Align::Left);
        self.y += 18.0;
    }

    pub(super) fn group(&mut self, s: &str) {
        self.y += 4.0;
        self.text(s, &self.f.bold, Rect::new(self.x, self.y, self.w, 18.0), self.t().fg, Align::Left);
        self.y += 19.0;
    }

    /// Two-column label/value grid.
    pub(super) fn kv(&mut self, items: &[(&str, String)]) {
        let cw = (self.w - 20.0) / 2.0;
        for (i, (k, v)) in items.iter().enumerate() {
            let r = Rect::new(self.x + (i % 2) as f32 * (cw + 20.0), self.y, cw, 19.0);
            self.text(k, &self.f.body, r, self.t().fg2, Align::Left);
            self.text(v, &self.f.body, r, self.t().fg, Align::Right);
            if i % 2 == 1 || i + 1 == items.len() {
                self.y += 19.0;
            }
        }
    }

    pub(super) fn row(&mut self, left: &str, right: &str, rc: Color) {
        let r = Rect::new(self.x, self.y, self.w, 20.0);
        let rw = self.ctx.gfx.text_width(&self.f.body, right);
        self.text(left, &self.f.body, Rect { w: (self.w - rw - 8.0).max(0.0), ..r }, self.t().fg, Align::Left);
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
        self.text(s, &self.f.hint, Rect::new(self.x, self.y, self.w, h), self.t().fg2, Align::Left);
        self.y += h + 4.0;
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
                    cv.round(tr.inset(2.0, 2.0), 4.0, t.accent);
                } else if self.mouse.is_some_and(|(x, y)| tr.contains(x, y)) {
                    cv.round(tr.inset(2.0, 2.0), 4.0, t.hover);
                }
            }
            let f = if i == self.tab { &self.f.bold } else { &self.f.body };
            self.text(name, f, tr, if i == self.tab { t.on_accent } else { t.fg2 }, Align::Center);
            self.hits.push((tr, i));
        }
        self.y += 30.0;
    }

    pub(super) fn row_kv(&mut self, left: &str, right: &str, rc: Color) {
        let r = Rect::new(self.x, self.y, self.w, 19.0);
        self.text(left, &self.f.body, Rect { w: r.w - 80.0, ..r }, self.t().fg2, Align::Left);
        self.text(right, &self.f.body, r, rc, Align::Right);
        self.y += 19.0;
    }

    pub(super) fn missing(&mut self, title: &str) {
        self.header(title, "", self.t().fg);
        self.sub("Waiting for data…");
    }
}
