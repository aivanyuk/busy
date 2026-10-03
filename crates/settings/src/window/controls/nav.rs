//! The nav column (design `navItems`): app header, search box, and one 36-high item per page with its color
//! dot, name and On/Off status; the selected one gets `--hover` and an accent pill.

use super::{face, pill};
use crate::window::layout::{Fonts, HEADER_Y, LINE_12, LINE_14, Label};
use busy_ui::render::{Align, Canvas, Gfx, Rect};
use busy_ui::theme::{Color, Theme};

/// Three `--link` bars of `heights`, `w` wide and `gap` apart, bottoms on `base` (the app glyph).
pub(in crate::window) fn glyph(cv: &Canvas, x: f32, base: f32, (w, gap): (f32, f32), heights: [f32; 3], t: &Theme) {
    for (i, h) in heights.into_iter().enumerate() {
        cv.round(Rect::new(x + i as f32 * (w + gap), base - h, w, h), 1.0, t.link);
    }
}

pub(in crate::window) fn header(cv: &Canvas, readings: &str, t: &Theme, f: &Fonts) {
    let tile = Rect::new(16.0 + 4.0, HEADER_Y + 8.0, 48.0, 48.0);
    cv.round(tile, 8.0, t.card);
    cv.round_outline(tile, 8.0, t.card_line);
    // 5-wide bars 3 apart (21 wide, centered), 13 up from the tile's bottom.
    glyph(cv, tile.x + (48.0 - 21.0) / 2.0, tile.bottom() - 13.0, (5.0, 3.0), [10.0, 22.0, 15.0], t);
    let x = tile.right() + 12.0;
    let y = tile.y + (48.0 - (LINE_14 + 2.0 + LINE_12)) / 2.0;
    cv.text("busy", &f.strong, Rect::new(x, y, 170.0, LINE_14), t.fg, Align::Left);
    cv.text(readings, &f.small, Rect::new(x, y + LINE_14 + 2.0, 170.0, LINE_12), t.fg2, Align::Left);
}

/// The search box as the design draws it: placeholder (or the query) and a circle glyph, bottom edge
/// `--ctl-strong`; with the focus, a caret and WinUI's 2-DIP accent bottom edge.
pub(in crate::window) fn search(cv: &Canvas, r: Rect, query: &str, focused: bool, (gfx, t): (&Gfx, &Theme), f: &Fonts) {
    face(cv, r, t.ctl, t.ctl_strong, t);
    let text = Rect::new(r.x + 12.0, r.y, r.w - 40.0, r.h);
    if query.is_empty() {
        cv.text(busy_ui::i18n::t().settings.find, &f.body, text, t.fg3, Align::Left);
    } else {
        cv.text(query, &f.body, text, t.fg, Align::Left);
    }
    if focused {
        cv.fill(Rect::new(r.x + 2.0, r.bottom() - 2.0, r.w - 4.0, 2.0), t.accent);
        let x = (text.x + if query.is_empty() { 0.0 } else { gfx.text_width(&f.body, query) }).min(text.right());
        cv.fill(Rect::new(x, r.y + 8.0, 1.0, r.h - 16.0), t.fg);
    }
    let icon = Rect::new(r.right() - 12.0 - 11.0, r.y + (r.h - 11.0) / 2.0, 11.0, 11.0);
    cv.round_stroke(icon, 5.5, 1.5, t.fg3);
}

/// The items' status column: the longer of On and Off, at least the design's 32.
pub(in crate::window) fn status_w(gfx: &Gfx, f: &Fonts) -> f32 {
    let s = &busy_ui::i18n::t().settings;
    [s.on, s.off].iter().map(|l| f.label_width(gfx, Label::Small, l)).fold(32.0, f32::max)
}

pub(in crate::window) struct Item<'a> {
    pub(in crate::window) label: &'a str,
    pub(in crate::window) dot: Color,
    /// "On"/"Off"; empty for General.
    pub(in crate::window) status: &'a str,
    /// The status column's width (`status_w`).
    pub(in crate::window) status_w: f32,
    pub(in crate::window) selected: bool,
    pub(in crate::window) hover: bool,
}

pub(in crate::window) fn item(cv: &Canvas, r: Rect, it: &Item, t: &Theme, f: &Fonts) {
    if it.selected || it.hover {
        cv.round(r, 4.0, t.hover);
    }
    if it.selected {
        pill(cv, r, 18.0, t);
    }
    cv.round(Rect::new(r.x + 16.0, r.y + r.h / 2.0 - 5.0, 10.0, 10.0), 3.0, it.dot);
    let sw = if it.status.is_empty() { 0.0 } else { it.status_w };
    let x = r.x + 16.0 + 10.0 + 14.0;
    cv.text(it.label, &f.body, Rect::new(x, r.y, r.right() - 12.0 - sw - x, r.h), t.fg, Align::Left);
    cv.text(it.status, &f.small, Rect::new(r.right() - 12.0 - sw, r.y, sw, r.h), t.fg3, Align::Right);
}
