//! A standard button (design `.btn`, as Settings' "Run setup" and setup's Skip): 32 high, padding 0 20, a
//! control face with `--active` on hover, the label centered.

use super::face;
use crate::window::layout::Fonts;
use busy_ui::render::{Align, Canvas, Gfx, Rect};
use busy_ui::theme::Theme;

pub(in crate::window) const H: f32 = 32.0;

pub(in crate::window) fn size(gfx: &Gfx, f: &Fonts, label: &str) -> (f32, f32) {
    (gfx.text_width(&f.body, label) + 40.0, H)
}

pub(in crate::window) fn draw(cv: &Canvas, r: Rect, label: &str, hover: bool, t: &Theme, f: &Fonts) {
    face(cv, r, if hover { t.active } else { t.ctl }, t.ctl_bottom, t);
    cv.text(label, &f.body, r, t.fg, Align::Center);
}
