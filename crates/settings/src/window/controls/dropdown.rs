//! Dropdown (design `sel()`): a 32-high button with the chosen option and a chevron, and its popup list (one
//! 32-high option per row, the chosen one with an accent pill).

use super::{face, pill};
use crate::window::choices::Opt;
use crate::window::layout::{Fonts, POP_PAD, Target, View};
use busy_ui::render::{Align, Canvas, Gfx, Rect};
use busy_ui::theme::{Theme, rgba};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

/// Design: min-width 190, padding 0 12, 16 between the value and the chevron. Long names (adapters, sensors)
/// are cut off with an ellipsis past 320.
const MIN_W: f32 = 190.0;
const MAX_W: f32 = 320.0;
const CHEVRON_W: f32 = 9.0;

pub(in crate::window) fn size(gfx: &Gfx, f: &IDWriteTextFormat, opts: &[Opt]) -> (f32, f32) {
    let text = opts.iter().map(|o| gfx.text_width(f, &o.label)).fold(0.0, f32::max);
    ((12.0 + text + 16.0 + CHEVRON_W + 12.0).clamp(MIN_W, MAX_W), 32.0)
}

pub(in crate::window) fn draw(cv: &Canvas, r: Rect, label: &str, hover: bool, t: &Theme, f: &Fonts) {
    face(cv, r, if hover { t.active } else { t.ctl }, t.ctl_bottom, t);
    let text = Rect::new(r.x + 12.0, r.y, r.w - 12.0 - 16.0 - CHEVRON_W - 12.0, r.h);
    cv.text(label, &f.body, text, t.fg, Align::Left);
    chevron(cv, r.right() - 12.0 - CHEVRON_W, r.y + r.h / 2.0, t);
}

/// Design: a 6×6 L of 1.5-DIP strokes rotated 45° and lifted 2, i.e. a down-pointing chevron ~8.5 wide.
fn chevron(cv: &Canvas, x: f32, cy: f32, t: &Theme) {
    let (w, h) = (8.5, 4.25);
    let y = cy - h / 2.0;
    cv.line((x, y), (x + w / 2.0, y + h), 1.5, t.fg2);
    cv.line((x + w / 2.0, y + h), (x + w, y), 1.5, t.fg2);
}

/// The open popup, over everything else.
pub(in crate::window) fn draw_popup(cv: &Canvas, v: &View, t: &Theme, f: &Fonts) {
    let (Some(p), Some((opts, sel))) = (&v.popup, v.options()) else { return };
    // Design: box-shadow 0 8px 16px rgba(0,0,0,.3), approximated by two soft layers.
    cv.round(Rect::new(p.rect.x - 2.0, p.rect.y + 2.0, p.rect.w + 4.0, p.rect.h + 8.0), 12.0, rgba(0, 0.08));
    cv.round(Rect::new(p.rect.x - 1.0, p.rect.y + 1.0, p.rect.w + 2.0, p.rect.h + 4.0), 10.0, rgba(0, 0.12));
    cv.round(p.rect, 8.0, t.pop);
    cv.round_outline(p.rect, 8.0, t.win_line);
    for k in p.first..p.first + p.visible {
        let (Some(r), Some(o)) = (v.option_rect(k), opts.get(k)) else { continue };
        if k == sel || v.hover == Some(Target::Opt(k)) {
            cv.round(r, 4.0, t.hover);
        }
        if k == sel {
            pill(cv, r, 14.0, t);
        }
        cv.text(&o.label, &f.body, Rect::new(r.x + 16.0, r.y, r.w - 16.0 - POP_PAD, r.h), t.fg, Align::Left);
    }
    // More options than fit: a thin thumb on the right edge.
    if p.visible < opts.len() {
        let track = p.rect.h - 2.0 * POP_PAD;
        let thumb = (track * p.visible as f32 / opts.len() as f32).max(16.0);
        let y = p.rect.y + POP_PAD + (track - thumb) * p.first as f32 / (opts.len() - p.visible) as f32;
        cv.round(Rect::new(p.rect.right() - 4.0, y, 2.0, thumb), 1.0, t.fg3);
    }
}
