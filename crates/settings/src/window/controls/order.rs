//! The General page's "Taskbar order" card (design `orderItems`): an intro line, then one 48-high row per
//! module with its color, name, Shown/Hidden and 32×32 ↑/↓ buttons (dimmed at the ends).

use crate::window::layout::{CARD_PAD_L, Fonts, LINE_12, Target};
use busy_core::Module;
use busy_ui::render::{Align, Canvas, Rect};
use busy_ui::theme::{Color, Theme, alpha};

const INTRO_H: f32 = 12.0 + LINE_12 + 10.0;
const ROW_H: f32 = 48.0;
const BTN: f32 = 32.0;
const STATUS_W: f32 = 48.0;

pub(in crate::window) fn height(n: usize) -> f32 {
    INTRO_H + n as f32 * ROW_H
}

fn row(r: Rect, i: usize) -> Rect {
    Rect::new(r.x, r.y + INTRO_H + i as f32 * ROW_H, r.w, ROW_H)
}

/// (↑, ↓) of row `i`: right-aligned with padding 12, 4 apart.
fn buttons(r: Rect, i: usize) -> (Rect, Rect) {
    let rr = row(r, i);
    let y = rr.y + (ROW_H - BTN) / 2.0;
    let down = Rect::new(rr.right() - 12.0 - BTN, y, BTN, BTN);
    (Rect::new(down.x - 4.0 - BTN, y, BTN, BTN), down)
}

/// Row `i`'s ↑ (`up`) or ↓ button.
pub(in crate::window) fn button(r: Rect, i: usize, up: bool) -> Rect {
    let (u, d) = buttons(r, i);
    if up { u } else { d }
}

/// The ↑ of the first row and the ↓ of the last do nothing, so they are not targets.
pub(in crate::window) fn hit(r: Rect, n: usize, x: f32, y: f32) -> Option<Target> {
    (0..n).find_map(|i| {
        let (up, down) = buttons(r, i);
        if up.contains(x, y) && i > 0 {
            Some(Target::Up(i))
        } else if down.contains(x, y) && i + 1 < n {
            Some(Target::Down(i))
        } else {
            None
        }
    })
}

/// The intro line (design `orderNote`): a vertical taskbar stacks the cells top to bottom.
fn note(vertical: bool) -> &'static str {
    if vertical { "Widgets appear top to bottom in this order." } else { "Widgets appear left to right in this order." }
}

/// `color(m)` is a module's palette color; `hover` the hovered button; `vertical` whether the taskbar is.
#[allow(clippy::too_many_arguments)] // One call site, in `paint`; the arguments are what the card needs to draw.
pub(in crate::window) fn draw(
    cv: &Canvas,
    r: Rect,
    list: &[(Module, bool)],
    color: impl Fn(Module) -> Color,
    hover: Option<Target>,
    vertical: bool,
    t: &Theme,
    f: &Fonts,
) {
    cv.round(r, 4.0, t.card);
    cv.round_outline(r, 4.0, t.card_line);
    let intro = Rect::new(r.x + CARD_PAD_L, r.y + 12.0, r.w - CARD_PAD_L - 16.0, LINE_12);
    cv.text(note(vertical), &f.small, intro, t.fg2, Align::Left);
    let n = list.len();
    for (i, &(m, shown)) in list.iter().enumerate() {
        let rr = row(r, i);
        cv.fill(Rect::new(rr.x + 1.0, rr.y, rr.w - 2.0, 1.0), t.line);
        cv.round(Rect::new(rr.x + CARD_PAD_L, rr.y + ROW_H / 2.0 - 5.0, 10.0, 10.0), 3.0, color(m));
        let (up, down) = buttons(r, i);
        let name_x = rr.x + CARD_PAD_L + 10.0 + 14.0;
        let status_x = up.x - 14.0 - STATUS_W;
        cv.text(
            busy_ui::i18n::t().common.module(m),
            &f.body,
            Rect::new(name_x, rr.y, status_x - 14.0 - name_x, ROW_H),
            t.fg,
            Align::Left,
        );
        let status = if shown { "Shown" } else { "Hidden" };
        cv.text(status, &f.small, Rect::new(status_x, rr.y, STATUS_W, ROW_H), t.fg3, Align::Left);
        let arrows = [(up, "\u{2191}", i > 0, Target::Up(i)), (down, "\u{2193}", i + 1 < n, Target::Down(i))];
        for (b, glyph, enabled, target) in arrows {
            if enabled && hover == Some(target) {
                cv.round(b, 4.0, t.hover);
            }
            cv.text(glyph, &f.body, b, alpha(t.fg, if enabled { 1.0 } else { 0.3 }), Align::Center);
        }
    }
}
