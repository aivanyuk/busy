//! Toggle switch (design `tog()`): "On"/"Off" right-aligned in 24 DIPs (wider when a language's words are),
//! 12 apart from a 40×20 track with a 12-DIP knob 4 in from its edge.

use crate::window::layout::{Fonts, Label};
use busy_ui::render::{Align, Canvas, Gfx, Rect};
use busy_ui::theme::{Theme, alpha};

/// The track and the gap before it.
const SWITCH_W: f32 = 12.0 + 40.0;
const H: f32 = 20.0;

/// The label's box: the longer of On and Off, at least the design's 24, so the switch doesn't move as it flips.
fn label_w(gfx: &Gfx, f: &Fonts) -> f32 {
    let s = &busy_ui::i18n::t().settings;
    [s.on, s.off].iter().map(|l| f.label_width(gfx, Label::Body, l)).fold(24.0, f32::max)
}

pub(in crate::window) fn size(gfx: &Gfx, f: &Fonts) -> (f32, f32) {
    (label_w(gfx, f) + SWITCH_W, H)
}

/// `r` is as wide as `size`; `enabled` false draws it at 40 % (the autostart toggle until the registry has
/// answered).
pub(in crate::window) fn draw(cv: &Canvas, r: Rect, on: bool, enabled: bool, t: &Theme, f: &Fonts) {
    let a = if enabled { 1.0 } else { 0.4 };
    let s = &busy_ui::i18n::t().settings;
    let label = Rect::new(r.x, r.y, (r.w - SWITCH_W).max(0.0), r.h);
    cv.text(if on { s.on } else { s.off }, &f.body, label, alpha(t.fg, a), Align::Right);
    let track = Rect::new(r.right() - 40.0, r.y + (r.h - H) / 2.0, 40.0, H);
    if on {
        cv.round(track, 10.0, alpha(t.accent, a));
    } else {
        cv.round_outline(track, 10.0, alpha(t.ctl_strong, a));
    }
    let x = track.x + if on { 24.0 } else { 4.0 };
    cv.round(Rect::new(x, track.y + 4.0, 12.0, 12.0), 6.0, alpha(if on { t.on_accent } else { t.knob }, a));
}
