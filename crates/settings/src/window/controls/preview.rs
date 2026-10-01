//! A module page's preview card (design `pw`): "Preview" and a note over a piece of taskbar with the module's
//! cell, drawn by the same code as the real taskbar, and the clock: a 48-high strip, or down a vertical
//! taskbar a 64-wide column on its side of the card. A module that isn't on the taskbar is shown at 40 %, with
//! a note saying how to show it.

use crate::window::clock;
use crate::window::layout::{Fonts, LINE_12};
use busy_core::{Config, Module};
use busy_ui::cell::{self, Cell};
use busy_ui::ctx::Ctx;
use busy_ui::history::History;
use busy_ui::render::{Align, Canvas, Gfx, Rect};
use busy_ui::theme::{Color, Theme, alpha};
use busy_win::Edge;

/// Padding 14 16 16, the header line, 10 apart from the 48-high strip.
const PAD_T: f32 = 14.0;
const PAD_X: f32 = 16.0;
const PAD_B: f32 = 16.0;
const HEAD_H: f32 = PAD_T + LINE_12 + 10.0;
const STRIP_H: f32 = 48.0;
/// Card height across a horizontal taskbar.
pub(in crate::window) const HEIGHT: f32 = HEAD_H + STRIP_H + PAD_B;
/// The vertical column (design `isVertical`): 64 wide, a 1-DIP border and padding 6 0 around the cell, a
/// 24 × 1 divider and the clock (padding 2 0 around two 16-DIP lines), 6 apart.
const COL_W: f32 = 64.0;
const COL_PAD: f32 = 1.0 + 6.0;
const COL_GAP: f32 = 6.0;
const DIVIDER_W: f32 = 24.0;
const CLOCK_H: f32 = 2.0 + 2.0 * LINE_12 + 2.0;
/// The column's room for a cell not measured yet (no readings, or a module without them).
const NO_CELL_H: f32 = cell::CELL_H;

/// The card's height on a taskbar at `edge`; down a vertical one it follows the cell's `column_height`.
pub(in crate::window) fn height(edge: Edge, cell_h: Option<f32>) -> f32 {
    if !edge.is_vertical() {
        return HEIGHT;
    }
    HEAD_H + 2.0 * COL_PAD + cell_h.unwrap_or(NO_CELL_H) + 2.0 * COL_GAP + 1.0 + CLOCK_H + PAD_B
}

pub(in crate::window) struct Data<'a> {
    pub(in crate::window) snap: &'a busy_core::Snapshot,
    pub(in crate::window) hist: &'a History,
}

impl Data<'_> {
    /// `m`'s cell as it reads now; `None` while its module has no data.
    fn cell<'a>(&'a self, m: Module, cfg: &'a Config, t: &'a Theme, gfx: &'a Gfx) -> Option<Cell<'a>> {
        let ctx = Ctx { cfg, snap: self.snap, hist: self.hist, theme: t, gfx };
        cell::cell(&ctx, cfg.module(m)?)
    }
}

/// The height of `m`'s cell in a vertical taskbar's column, which the card's height follows.
pub(in crate::window) fn column_height(d: &Data, m: Module, cfg: &Config, t: &Theme, gfx: &Gfx) -> Option<f32> {
    d.cell(m, cfg, t, gfx).map(|c| c.column_height())
}

#[allow(clippy::too_many_arguments)] // One call site, in `paint`; the arguments are what a card needs to draw.
pub(in crate::window) fn draw(
    cv: &Canvas,
    r: Rect,
    m: Module,
    cfg: &Config,
    data: Option<&Data>,
    (gfx, cell_fonts): (&Gfx, &cell::Fonts),
    edge: Edge,
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

    let area = Rect::new(r.x + PAD_X, r.y + HEAD_H, r.w - 2.0 * PAD_X, r.h - HEAD_H - PAD_B);
    let c = data.and_then(|d| d.cell(m, cfg, t, gfx));
    let at = if edge.is_vertical() {
        column(cv, area, c.as_ref(), edge == Edge::Right, (gfx, cell_fonts), t, f)
    } else {
        strip(cv, area, c.as_ref(), (gfx, cell_fonts), t, f)
    };
    if let Some(at) = at
        && !mc.taskbar
    {
        // Design `opacity: .4`: the strip's own color (`--tb` over the card) at 60 % over the cell.
        cv.fill(at, alpha(over(t.tb, t.card), 0.6));
    }
}

/// Across a horizontal taskbar: the cell, then the clock at the strip's right end. Returns where the cell went.
fn strip(
    cv: &Canvas,
    area: Rect,
    c: Option<&Cell>,
    (gfx, cf): (&Gfx, &cell::Fonts),
    t: &Theme,
    f: &Fonts,
) -> Option<Rect> {
    let strip = Rect { h: STRIP_H, ..area };
    cv.round(strip, 4.0, t.tb);
    cv.round_outline(strip, 4.0, t.tb_line);
    // Clock: 12 px on 16-DIP lines, right-aligned, padding 0 8, 8 from the strip's edge and from the cell.
    let (time, date) = clock::now();
    let cw = gfx.text_width(&f.small, &time).max(gfx.text_width(&f.small, &date)) + 16.0;
    let clock = Rect::new(strip.right() - 8.0 - cw, strip.y + (STRIP_H - 2.0 * LINE_12) / 2.0, cw - 8.0, LINE_12);
    cv.text(&time, &f.small, clock, t.fg, Align::Right);
    cv.text(&date, &f.small, Rect { y: clock.bottom(), ..clock }, t.fg, Align::Right);

    let c = c?;
    let w = c.width(gfx, cf);
    let at = Rect::new(clock.x - 8.0 - 8.0 - w, strip.y + (STRIP_H - cell::CELL_H) / 2.0, w, cell::CELL_H);
    c.draw(cv, gfx, cf, t, at);
    Some(at)
}

/// Down a vertical taskbar (design `isVertical`): a column at the card's left, or its right for a taskbar on
/// the right (design `pvJustify`), with the cell, a divider, and the time over the short date. Returns where
/// the cell went.
fn column(
    cv: &Canvas,
    area: Rect,
    c: Option<&Cell>,
    right: bool,
    (gfx, cf): (&Gfx, &cell::Fonts),
    t: &Theme,
    f: &Fonts,
) -> Option<Rect> {
    let col = Rect::new(if right { area.right() - COL_W } else { area.x }, area.y, COL_W, area.h);
    cv.round(col, 4.0, t.tb);
    cv.round_outline(col, 4.0, t.tb_line);
    let (time, date) = clock::now_short();
    let clock = Rect::new(col.x, col.bottom() - COL_PAD - CLOCK_H + 2.0, COL_W, LINE_12);
    cv.text(&time, &f.small, clock, t.fg, Align::Center);
    cv.text(&date, &f.small, Rect { y: clock.bottom(), ..clock }, t.fg, Align::Center);
    let divider = Rect::new(col.x + (COL_W - DIVIDER_W) / 2.0, clock.y - 2.0 - COL_GAP - 1.0, DIVIDER_W, 1.0);
    cv.fill(divider, t.line);

    let c = c?;
    let (w, h) = (cell::column_width(COL_W), c.column_height());
    // The card was laid out for this height; centered in what is there, should a reading have changed it since.
    let room = divider.y - COL_GAP - (col.y + COL_PAD);
    let at = Rect::new(col.x + (COL_W - w) / 2.0, col.y + COL_PAD + (room - h) / 2.0, w, h);
    c.draw_column(cv, gfx, cf, t, at);
    Some(at)
}

/// `top` composited over opaque `bottom`.
fn over(top: Color, bottom: Color) -> Color {
    let mix = |a: f32, b: f32| a * top.a + b * (1.0 - top.a);
    Color { r: mix(top.r, bottom.r), g: mix(top.g, bottom.g), b: mix(top.b, bottom.b), a: 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_fits_its_taskbar() {
        assert_eq!(height(Edge::Bottom, Some(80.0)), HEIGHT);
        assert_eq!(height(Edge::Top, None), HEIGHT);
        // Header 40, border and padding 7 + 7, the cell, the divider 6 + 1 + 6 apart, the clock 36, padding 16.
        assert_eq!(height(Edge::Right, Some(39.0)), 40.0 + 14.0 + 39.0 + 13.0 + 36.0 + 16.0);
        assert_eq!(height(Edge::Left, None), height(Edge::Right, Some(cell::CELL_H)));
    }
}
