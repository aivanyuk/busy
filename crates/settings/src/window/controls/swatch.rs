//! Color swatches (design `sw()`): one 30-DIP circle per palette color, 4 apart, a 20-DIP dot inside; the
//! chosen one has a 2-DIP `--fg` ring.

use busy_core::PALETTE_LEN;
use busy_ui::render::{Canvas, Rect};
use busy_ui::theme::Theme;

const D: f32 = 30.0;
const GAP: f32 = 4.0;
const DOT: f32 = 20.0;

pub(in crate::window) const SIZE: (f32, f32) = (PALETTE_LEN as f32 * (D + GAP) - GAP, D);

pub(in crate::window) fn swatch(r: Rect, i: usize) -> Rect {
    Rect::new(r.x + i as f32 * (D + GAP), r.y, D, D)
}

pub(in crate::window) fn hit(r: Rect, x: f32, y: f32) -> Option<usize> {
    (0..PALETTE_LEN as usize).find(|&i| swatch(r, i).contains(x, y))
}

pub(in crate::window) fn draw(cv: &Canvas, r: Rect, sel: u8, t: &Theme) {
    for (i, &c) in t.pal.iter().enumerate() {
        let s = swatch(r, i);
        if i == sel as usize {
            cv.round_stroke(s, D / 2.0, 2.0, t.fg);
        }
        let o = (D - DOT) / 2.0;
        cv.round(Rect::new(s.x + o, s.y + o, DOT, DOT), DOT / 2.0, c);
    }
}
