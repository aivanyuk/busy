//! Segmented control (design `seg()`): side-by-side 28-high segments sized to their 13 px labels (padding
//! 0 14) in a `--ctl` box with padding 2 and 2 between; the chosen one is `--accent` with `--on-accent` text.

use crate::window::choices::Opt;
use crate::window::layout::Fonts;
use busy_ui::render::{Align, Canvas, Gfx, Rect};
use busy_ui::theme::Theme;

const SEG_H: f32 = 28.0;
const PAD: f32 = 2.0;
/// Outer border.
const LINE: f32 = 1.0;

fn widths(gfx: &Gfx, f: &Fonts, opts: &[Opt]) -> Vec<f32> {
    opts.iter().map(|o| (f.seg_width(gfx, &o.label) + 28.0).ceil()).collect()
}

pub(in crate::window) fn size(gfx: &Gfx, f: &Fonts, opts: &[Opt]) -> (f32, f32) {
    let w: f32 = widths(gfx, f, opts).iter().sum();
    let gaps = opts.len().saturating_sub(1) as f32 * PAD;
    (w + gaps + 2.0 * (PAD + LINE), SEG_H + 2.0 * (PAD + LINE))
}

/// Each segment's rect within the control at `r`.
pub(in crate::window) fn segments(gfx: &Gfx, f: &Fonts, opts: &[Opt], r: Rect) -> Vec<Rect> {
    let mut x = r.x + PAD + LINE;
    widths(gfx, f, opts)
        .into_iter()
        .map(|w| {
            let s = Rect::new(x, r.y + PAD + LINE, w, SEG_H);
            x += w + PAD;
            s
        })
        .collect()
}

pub(in crate::window) fn draw(cv: &Canvas, gfx: &Gfx, r: Rect, opts: &[Opt], sel: usize, t: &Theme, f: &Fonts) {
    cv.round(r, 6.0, t.ctl);
    cv.round_outline(r, 6.0, t.ctl_line);
    for (k, (s, o)) in segments(gfx, f, opts, r).into_iter().zip(opts).enumerate() {
        let on = k == sel;
        if on {
            cv.round(s, 4.0, t.accent);
        }
        cv.text(&o.label, &f.seg, s, if on { t.on_accent } else { t.fg }, Align::Center);
    }
}
