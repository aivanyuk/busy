//! Toggle switch (design `tog()`): "On"/"Off" right-aligned in 24 DIPs, 12 apart from a 40×20 track with a
//! 12-DIP knob 4 in from its edge.

use crate::window::layout::Fonts;
use busy_ui::render::{Align, Canvas, Rect};
use busy_ui::theme::{Theme, alpha};

pub(in crate::window) const SIZE: (f32, f32) = (24.0 + 12.0 + 40.0, 20.0);

/// `enabled` false draws it at 40 % (the autostart toggle until the registry has answered).
pub(in crate::window) fn draw(cv: &Canvas, r: Rect, on: bool, enabled: bool, t: &Theme, f: &Fonts) {
    let a = if enabled { 1.0 } else { 0.4 };
    let s = &busy_ui::i18n::t().settings;
    cv.text(if on { s.on } else { s.off }, &f.body, Rect::new(r.x, r.y, 24.0, r.h), alpha(t.fg, a), Align::Right);
    let track = Rect::new(r.right() - 40.0, r.y + (r.h - 20.0) / 2.0, 40.0, 20.0);
    if on {
        cv.round(track, 10.0, alpha(t.accent, a));
    } else {
        cv.round_outline(track, 10.0, alpha(t.ctl_strong, a));
    }
    let x = track.x + if on { 24.0 } else { 4.0 };
    cv.round(Rect::new(x, track.y + 4.0, 12.0, 12.0), 6.0, alpha(if on { t.on_accent } else { t.knob }, a));
}
