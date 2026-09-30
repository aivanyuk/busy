//! A module page's preview card (design `pw`): "Preview" and a note over a 48-high taskbar strip with the
//! module's cell, drawn by the same code as the real taskbar, and the clock. A module that isn't on the
//! taskbar is shown at 40 %, with a note saying how to show it.

use crate::window::clock;
use crate::window::layout::{Fonts, LINE_12};
use busy_core::{Config, Module, Snapshot};
use busy_ui::cell;
use busy_ui::ctx::Ctx;
use busy_ui::history::History;
use busy_ui::render::{Align, Canvas, Gfx, Rect};
use busy_ui::theme::{Color, Theme, alpha};

/// Padding 14 16 16, the header line, 10 apart from the 48-high strip.
const PAD_T: f32 = 14.0;
const PAD_X: f32 = 16.0;
const STRIP_H: f32 = 48.0;
pub(in crate::window) const HEIGHT: f32 = PAD_T + LINE_12 + 10.0 + STRIP_H + 16.0;

pub(in crate::window) struct Data<'a> {
    pub(in crate::window) snap: &'a Snapshot,
    pub(in crate::window) hist: &'a History,
}

#[allow(clippy::too_many_arguments)] // One call site, in `paint`; the arguments are what a card needs to draw.
pub(in crate::window) fn draw(
    cv: &Canvas,
    r: Rect,
    m: Module,
    cfg: &Config,
    data: Option<&Data>,
    (gfx, cell_fonts): (&Gfx, &cell::Fonts),
    t: &Theme,
    f: &Fonts,
) {
    cv.round(r, 4.0, t.card);
    cv.round_outline(r, 4.0, t.card_line);
    let Some(mc) = cfg.module(m) else { return };
    let head = Rect::new(r.x + PAD_X, r.y + PAD_T, r.w - 2.0 * PAD_X, LINE_12);
    cv.text("Preview", &f.small, head, t.fg2, Align::Left);
    let note = if mc.taskbar { "Live" } else { "Hidden \u{2014} turn on \u{201c}Show on taskbar\u{201d}" };
    cv.text(note, &f.small, head, t.fg2, Align::Right);

    let strip = Rect::new(r.x + PAD_X, head.bottom() + 10.0, r.w - 2.0 * PAD_X, STRIP_H);
    cv.round(strip, 4.0, t.tb);
    cv.round_outline(strip, 4.0, t.tb_line);
    // Clock: 12 px on 16-DIP lines, right-aligned, padding 0 8, 8 from the strip's edge and from the cell.
    let (time, date) = clock::now();
    let cw = gfx.text_width(&f.small, &time).max(gfx.text_width(&f.small, &date)) + 16.0;
    let clock = Rect::new(strip.right() - 8.0 - cw, strip.y + (STRIP_H - 2.0 * LINE_12) / 2.0, cw - 8.0, LINE_12);
    cv.text(&time, &f.small, clock, t.fg, Align::Right);
    cv.text(&date, &f.small, Rect { y: clock.bottom(), ..clock }, t.fg, Align::Right);

    let Some(d) = data else { return };
    let ctx = Ctx { cfg, snap: d.snap, hist: d.hist, theme: t, gfx };
    let Some(c) = cell::cell(&ctx, mc) else { return };
    let w = c.width(gfx, cell_fonts);
    let at = Rect::new(clock.x - 8.0 - 8.0 - w, strip.y + (STRIP_H - cell::CELL_H) / 2.0, w, cell::CELL_H);
    c.draw(cv, gfx, cell_fonts, t, at);
    if !mc.taskbar {
        // Design `opacity: .4`: the strip's own color (`--tb` over the card) at 60 % over the cell.
        cv.fill(at, alpha(over(t.tb, t.card), 0.6));
    }
}

/// `top` composited over opaque `bottom`.
fn over(top: Color, bottom: Color) -> Color {
    let mix = |a: f32, b: f32| a * top.a + b * (1.0 - top.a);
    Color { r: mix(top.r, bottom.r), g: mix(top.g, bottom.g), b: mix(top.b, bottom.b), a: 1.0 }
}
