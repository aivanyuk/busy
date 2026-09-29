//! Measuring and drawing a cell in its style, to the design's `MeterWidget` geometry (sizes in DIPs).

use super::cells::{Body, Cell};
use crate::render::{Align, Canvas, Gfx, Rect};
use crate::theme::Theme;
use std::cell::RefCell;
use std::collections::HashMap;
use windows::Win32::Graphics::DirectWrite::{DWRITE_FONT_WEIGHT_BOLD, IDWriteTextFormat};
use windows::core::Result;

/// Cell height; cells are centered in the taskbar.
pub(super) const CELL_H: f32 = 40.0;
/// Horizontal padding inside a cell.
const PAD_X: f32 = 8.0;
/// Labels: 9 px, letter-spacing .06em, line-height 1.15.
const TRACKING: f32 = 9.0 * 0.06;
const LABEL_H: f32 = 9.0 * 1.15;
/// Values: 13 px, line-height 1.15; right-aligned in at least 32 px (Text) or left in 30 (Bar).
const VALUE_H: f32 = 13.0 * 1.15;
const TEXT_MIN_W: f32 = 32.0;
const BAR_TEXT_MIN_W: f32 = 30.0;
/// Graph: 40×20 sparkline, 2 px under its label row.
const GRAPH_W: f32 = 40.0;
const GRAPH_H: f32 = 20.0;
/// Samples on the sparkline: ~2 px each; the whole history would be unreadably dense at this size.
const GRAPH_SPAN: usize = 20;
/// Bars: 26 px high, 1 px apart, 6 px from the text.
const BAR_H: f32 = 26.0;
/// IO: two 15 px rows of 11 px text, key 5 px from a value right-aligned in at least 60 px.
const IO_ROW_H: f32 = 15.0;
const IO_MIN_W: f32 = 60.0;

pub(super) struct Fonts {
    /// Labels and the Graph row: 9 px semibold.
    small: IDWriteTextFormat,
    /// Text and Bar values: 13 px semibold.
    value: IDWriteTextFormat,
    /// IO values: 11 px regular.
    io: IDWriteTextFormat,
    /// IO keys: 11 px bold.
    io_key: IDWriteTextFormat,
    /// Widths of strings without digits (labels, keys), which are constant for a given config.
    widths: RefCell<HashMap<(u8, String), f32>>,
}

/// Which format a cached width was measured with.
#[derive(Clone, Copy)]
enum Font {
    Small,
    Value,
    Io,
    IoKey,
}

impl Fonts {
    pub(super) fn new(gfx: &Gfx) -> Result<Self> {
        Ok(Self {
            small: gfx.format(9.0, true)?,
            value: gfx.format(13.0, true)?,
            io: gfx.format(11.0, false)?,
            io_key: gfx.format_weight(11.0, DWRITE_FONT_WEIGHT_BOLD)?,
            widths: RefCell::new(HashMap::new()),
        })
    }

    fn format(&self, f: Font) -> &IDWriteTextFormat {
        match f {
            Font::Small => &self.small,
            Font::Value => &self.value,
            Font::Io => &self.io,
            Font::IoKey => &self.io_key,
        }
    }

    /// Width of `s`; labels (tracked) and other strings without digits are measured once.
    fn width(&self, gfx: &Gfx, f: Font, s: &str) -> f32 {
        let tracking = if matches!(f, Font::Small) { TRACKING } else { 0.0 };
        let measure = || gfx.text_width_tracked(self.format(f), s, tracking);
        if s.bytes().any(|b| b.is_ascii_digit()) {
            return measure();
        }
        let mut cache = self.widths.borrow_mut();
        // Bounded: a config has a handful of labels; a sensor rename could otherwise grow it forever.
        if cache.len() > 64 {
            cache.clear();
        }
        *cache.entry((f as u8, s.to_owned())).or_insert_with(measure)
    }
}

fn max_width<'s>(gfx: &Gfx, f: &Fonts, font: Font, ss: impl IntoIterator<Item = &'s str>) -> f32 {
    ss.into_iter().map(|s| f.width(gfx, font, s)).fold(0.0, f32::max)
}

impl Cell<'_> {
    fn label_w(&self, gfx: &Gfx, f: &Fonts) -> f32 {
        self.label.as_deref().map_or(0.0, |l| f.width(gfx, Font::Small, l))
    }

    /// Total width, padding included.
    pub(super) fn width(&self, gfx: &Gfx, f: &Fonts) -> f32 {
        let content = match &self.body {
            Body::Text { value, .. } => self.label_w(gfx, f).max(f.width(gfx, Font::Value, value)).max(TEXT_MIN_W),
            // The label row (label, value) is hidden with the label, as in the design.
            Body::Graph { value, .. } => match &self.label {
                Some(_) => GRAPH_W.max(self.label_w(gfx, f) + 4.0 + f.width(gfx, Font::Small, value)),
                None => GRAPH_W,
            },
            Body::Bar { bars, bar_w, value, .. } => {
                let text = self.label_w(gfx, f).max(f.width(gfx, Font::Value, value)).max(BAR_TEXT_MIN_W);
                bars_w(bars.len(), *bar_w) + 6.0 + text
            }
            Body::Io { rows } => {
                let keys = max_width(gfx, f, Font::IoKey, rows.iter().map(|r| r.0));
                let values = max_width(gfx, f, Font::Io, rows.iter().map(|r| r.2.as_str())).max(IO_MIN_W);
                keys + 5.0 + values
            }
        };
        (content + 2.0 * PAD_X).ceil()
    }

    /// Draws the cell's content into `r`, the cell's full `CELL_H`-high rect.
    pub(super) fn draw(&self, cv: &Canvas, gfx: &Gfx, f: &Fonts, t: &Theme, r: Rect) {
        let c = Rect::new(r.x + PAD_X, r.y, r.w - 2.0 * PAD_X, r.h);
        let label = |x: f32, y: f32, w: f32, align| {
            if let Some(l) = &self.label {
                cv.text_tracked(l, &f.small, Rect::new(x, y, w, LABEL_H), t.fg3, align, TRACKING);
            }
        };
        let label_h = if self.label.is_some() { LABEL_H } else { 0.0 };
        match &self.body {
            Body::Text { value, color } => {
                let y = c.y + (c.h - label_h - VALUE_H) / 2.0;
                label(c.x, y, c.w, Align::Right);
                cv.text(value, &f.value, Rect::new(c.x, y + label_h, c.w, VALUE_H), *color, Align::Right);
            }
            Body::Graph { value, lines, max } => {
                let row = if self.label.is_some() { LABEL_H + 2.0 } else { 0.0 };
                let y = c.y + (c.h - row - GRAPH_H) / 2.0;
                if self.label.is_some() {
                    label(c.x, y, c.w, Align::Left);
                    let head = Rect::new(c.x, y, c.w, LABEL_H);
                    cv.text_tracked(value, &f.small, head, t.fg, Align::Right, TRACKING);
                }
                let g = Rect::new(c.x, y + row, GRAPH_W, GRAPH_H);
                cv.round(g, 2.0, t.track);
                // Design: 1 px clear of the top and bottom edges.
                let inner = g.inset(0.0, 1.0);
                for (i, (s, col)) in lines.iter().enumerate() {
                    cv.graph(inner, s, *max, *col, if i == 0 { 0.3 } else { 0.0 }, GRAPH_SPAN);
                }
            }
            Body::Bar { bars, bar_w, value, color } => {
                let y = c.y + (c.h - BAR_H) / 2.0;
                for (i, (frac, col)) in bars.iter().enumerate() {
                    let b = Rect::new(c.x + i as f32 * (bar_w + 1.0), y, *bar_w, BAR_H);
                    cv.vbar(b, *frac, *col, t.track);
                }
                let dx = bars_w(bars.len(), *bar_w) + 6.0;
                let (x, w) = (c.x + dx, c.w - dx);
                let y = c.y + (c.h - label_h - VALUE_H) / 2.0;
                label(x, y, w, Align::Left);
                cv.text(value, &f.value, Rect::new(x, y + label_h, w, VALUE_H), *color, Align::Left);
            }
            Body::Io { rows } => {
                let keys = max_width(gfx, f, Font::IoKey, rows.iter().map(|r| r.0));
                let y0 = c.y + (c.h - 2.0 * IO_ROW_H) / 2.0;
                for (i, (key, col, value)) in rows.iter().enumerate() {
                    let y = y0 + i as f32 * IO_ROW_H;
                    cv.text(key, &f.io_key, Rect::new(c.x, y, keys, IO_ROW_H), *col, Align::Left);
                    let v = Rect::new(c.x + keys + 5.0, y, c.w - keys - 5.0, IO_ROW_H);
                    cv.text(value, &f.io, v, t.fg, Align::Right);
                }
            }
        }
    }
}

fn bars_w(n: usize, bar_w: f32) -> f32 {
    n as f32 * (bar_w + 1.0) - 1.0
}
