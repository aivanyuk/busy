//! Measuring and drawing a cell down a vertical taskbar (design `MeterWidget`, `w.vertical`): a column as wide
//! as the taskbar allows, its label over its value over its graph or bar, everything centered.

use super::styles::{Font, Fonts, GRAPH_SPAN, LABEL_H, TRACKING, VALUE_H, max_width};
use super::{Body, Cell};
use crate::render::{Align, Canvas, Gfx, Rect};
use crate::theme::Theme;

/// The design's cell is 56 DIPs wide in its 64-DIP taskbar, 4 DIPs in from either side; Windows' own vertical
/// taskbar is 48 wide, so cells there take its width less the same margins.
const MAX_W: f32 = 56.0;
const MARGIN: f32 = 4.0;
/// Padding 5 above and below; rows 3 apart.
const PAD_Y: f32 = 5.0;
const GAP: f32 = 3.0;
/// Graph values 11 px, Bar values 12 px, line-height 1.15.
const GRAPH_VALUE_H: f32 = 11.0 * 1.15;
const BAR_VALUE_H: f32 = 12.0 * 1.15;
/// A 16-high sparkline; per-core bars 12 high, 2 apart; a single bar is a 4-high horizontal bar.
const GRAPH_H: f32 = 16.0;
const CORES_H: f32 = 12.0;
const CORES_GAP: f32 = 2.0;
const TOTAL_H: f32 = 4.0;
/// Io: two 14-high rows of 11 px text, the key 3 from its value.
const IO_ROW_H: f32 = 14.0;
const IO_KEY_GAP: f32 = 3.0;

/// Width of a column cell in a vertical taskbar `bar_w` DIPs wide.
pub fn column_width(bar_w: f32) -> f32 {
    (bar_w - 2.0 * MARGIN).clamp(0.0, MAX_W)
}

/// Side padding: the design's 4 in a full-width cell, down to 2 in Windows' narrower taskbar.
fn pad_x(w: f32) -> f32 {
    ((w - 48.0) / 2.0).clamp(2.0, 4.0)
}

impl Cell<'_> {
    /// Height of the rows, padding excluded.
    fn rows_h(&self) -> f32 {
        let label = if self.label.is_some() { LABEL_H + GAP } else { 0.0 };
        label
            + match &self.body {
                Body::Text { .. } => VALUE_H,
                // Design: Network's graph has no value, so no value row either.
                Body::Graph { value, .. } if value.is_empty() => GRAPH_H,
                Body::Graph { .. } => GRAPH_VALUE_H + GAP + GRAPH_H,
                Body::Bar { bars, .. } => BAR_VALUE_H + GAP + if bars.len() > 1 { CORES_H } else { TOTAL_H },
                Body::Io { .. } => 2.0 * IO_ROW_H,
            }
    }

    /// Height of the cell in a column, padding included; its width is `column_width`'s, whatever it shows.
    pub fn column_height(&self) -> f32 {
        (self.rows_h() + 2.0 * PAD_Y).ceil()
    }

    /// Draws the cell's content into `r`, its `column_width` × `column_height` rect.
    pub fn draw_column(&self, cv: &Canvas, gfx: &Gfx, f: &Fonts, t: &Theme, r: Rect) {
        let px = pad_x(r.w);
        let (x, w) = (r.x + px, r.w - 2.0 * px);
        let mut y = r.y + (r.h - self.rows_h()) / 2.0;
        let mut row = |h: f32| {
            let at = Rect::new(x, y, w, h);
            y += h + GAP;
            at
        };
        if let Some(l) = &self.label {
            cv.text_tracked(l, &f.small, row(LABEL_H), t.fg3, Align::Center, TRACKING);
        }
        match &self.body {
            Body::Text { value, color, .. } => {
                // A value wider than the column (a fan's "1200 rpm") steps down to 11 px before it is trimmed.
                let font = if f.width(gfx, Font::Value, value) > w { &f.value11 } else { &f.value };
                cv.text(value, font, row(VALUE_H), *color, Align::Center);
            }
            Body::Graph { value, lines, max, .. } => {
                if !value.is_empty() {
                    cv.text(value, &f.value11, row(GRAPH_VALUE_H), t.fg, Align::Center);
                }
                let g = row(GRAPH_H);
                cv.round(g, 2.0, t.track);
                // As across: 1 px clear of the top and bottom edges.
                let inner = g.inset(0.0, 1.0);
                for (i, (s, col)) in lines.iter().enumerate() {
                    cv.graph(inner, s, *max, *col, if i == 0 { 0.3 } else { 0.0 }, 1.2, GRAPH_SPAN);
                }
            }
            Body::Bar { bars, value, color, .. } => {
                cv.text(value, &f.value12, row(BAR_VALUE_H), *color, Align::Center);
                if bars.len() > 1 {
                    let b = row(CORES_H);
                    let n = bars.len() as f32;
                    let bw = (b.w - CORES_GAP * (n - 1.0)) / n;
                    for (i, (frac, col)) in bars.iter().enumerate() {
                        cv.vbar(Rect::new(b.x + i as f32 * (bw + CORES_GAP), b.y, bw, CORES_H), *frac, *col, t.track);
                    }
                } else if let Some((frac, col)) = bars.first() {
                    cv.hbar(row(TOTAL_H), *frac, *col, t.track);
                }
            }
            Body::Io { rows, short, .. } => {
                let keys = max_width(gfx, f, Font::IoKey, rows.iter().map(|r| r.0));
                let values = max_width(gfx, f, Font::Io, short.iter().map(String::as_str));
                let mut both = row(2.0 * IO_ROW_H);
                // Windows' narrow column fits "W 999K" only without the padding: take it rather than trim.
                if keys + IO_KEY_GAP + values > both.w {
                    both = Rect { x: r.x, w: r.w, ..both };
                }
                for (i, ((key, col, _), value)) in rows.iter().zip(short).enumerate() {
                    let r = Rect { y: both.y + i as f32 * IO_ROW_H, h: IO_ROW_H, ..both };
                    cv.text(key, &f.io_key, Rect { w: keys, ..r }, *col, Align::Left);
                    let v = Rect::new(r.x + keys + IO_KEY_GAP, r.y, r.w - keys - IO_KEY_GAP, r.h);
                    cv.text(value, &f.io, v, t.fg, Align::Right);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::column_width;

    #[test]
    fn columns_fit_the_taskbar() {
        // Windows' vertical taskbar, and the design's.
        assert_eq!(column_width(48.0), 40.0);
        assert_eq!(column_width(64.0), 56.0);
        assert_eq!(column_width(96.0), 56.0);
        assert_eq!(column_width(4.0), 0.0);
    }
}
