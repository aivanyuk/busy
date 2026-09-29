//! The custom title bar (design: 40 high, app glyph, title, 46×40 caption buttons) and the non-client
//! handling behind it: the client area reaches the top edge, while Windows keeps the resize borders, the
//! shadow and the rounded corners of a normal frame.

use super::controls::nav::glyph;
use super::layout::{Fonts, TITLE_H, Target};
use busy_ui::render::{Align, Canvas, Rect};
use busy_ui::theme::{Theme, rgb};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use windows::Win32::UI::WindowsAndMessaging::*;

const BTN_W: f32 = 46.0;
/// Close-button hover (design: hard-coded `#C42B1C` with white glyph, as Windows).
const CLOSE_HOT: u32 = 0xC42B1C;

/// The caption button under `x` in a window `w` DIPs wide, right to left: Close, Max, Min.
pub(super) fn button_at(w: f32, x: f32) -> Option<Target> {
    match ((w - x) / BTN_W).floor() as i32 {
        0 if x < w => Some(Target::Close),
        1 => Some(Target::Max),
        2 => Some(Target::Min),
        _ => None,
    }
}

fn button_rect(w: f32, t: Target) -> Rect {
    let i = match t {
        Target::Close => 1.0,
        Target::Max => 2.0,
        _ => 3.0,
    };
    Rect::new(w - i * BTN_W, 0.0, BTN_W, TITLE_H)
}

pub(super) fn draw(cv: &Canvas, w: f32, hover: Option<Target>, maximized: bool, t: &Theme, f: &Fonts) {
    // Glyph: 3-wide bars 2 apart, 14 high, centered in the bar.
    glyph(cv, 16.0, (TITLE_H + 14.0) / 2.0, (3.0, 2.0), [6.0, 14.0, 9.0], t);
    cv.text("busy Settings", &f.caption, Rect::new(16.0 + 13.0 + 12.0, 0.0, 200.0, TITLE_H), t.fg, Align::Left);
    for b in [Target::Min, Target::Max, Target::Close] {
        let r = button_rect(w, b);
        let hot = hover == Some(b);
        let ink = if hot && b == Target::Close { rgb(0xFFFFFF) } else { t.fg };
        if hot {
            cv.fill(r, if b == Target::Close { rgb(CLOSE_HOT) } else { t.hover });
        }
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        match b {
            Target::Min => cv.fill(Rect::new(cx - 5.0, cy, 10.0, 1.0), ink),
            Target::Max if maximized => {
                // Restore: two overlapping boxes.
                cv.round_outline(Rect::new(cx - 5.0, cy - 3.0, 8.0, 8.0), 1.5, ink);
                cv.line((cx - 3.0, cy - 5.0), (cx + 3.5, cy - 5.0), 1.0, ink);
                cv.line((cx + 5.0, cy - 3.5), (cx + 5.0, cy + 3.0), 1.0, ink);
            }
            Target::Max => cv.round_outline(Rect::new(cx - 5.0, cy - 5.0, 10.0, 10.0), 2.0, ink),
            _ => {
                cv.line((cx - 5.0, cy - 5.0), (cx + 5.0, cy + 5.0), 1.0, ink);
                cv.line((cx + 5.0, cy - 5.0), (cx - 5.0, cy + 5.0), 1.0, ink);
            }
        }
    }
}

/// Height of the sizing border Windows would give the top edge (and that a maximized window hangs off screen).
fn border(dpi: u32) -> i32 {
    // SAFETY: plain metric queries.
    unsafe { GetSystemMetricsForDpi(SM_CYFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi) }
}

/// `WM_NCCALCSIZE` with `wParam` TRUE: the default frame on the left, right and bottom, none on top, so the
/// client (and our title bar) starts at the window's top edge; a maximized window keeps its content on screen.
pub(super) fn calc_size(hwnd: HWND, w: WPARAM, l: LPARAM, dpi: u32) -> LRESULT {
    // SAFETY: for WM_NCCALCSIZE with wParam TRUE, lParam points to an NCCALCSIZE_PARAMS valid for the message.
    unsafe {
        let p = l.0 as *mut NCCALCSIZE_PARAMS;
        let top = (*p).rgrc[0].top;
        let r = DefWindowProcW(hwnd, WM_NCCALCSIZE, w, l);
        (*p).rgrc[0].top = top + if IsZoomed(hwnd).as_bool() { border(dpi) } else { 0 };
        r
    }
}

/// `WM_NCHITTEST`: the default for the side and bottom borders; the top border, the title bar (caption, so it
/// drags and double-click maximizes) and the caption buttons (client, so we draw and click them) are ours.
pub(super) fn hit_test(hwnd: HWND, l: LPARAM, dpi: u32, width: f32) -> LRESULT {
    // SAFETY: DefWindowProcW with the message's own parameters; `pt` is a valid in/out pointer.
    unsafe {
        let r = DefWindowProcW(hwnd, WM_NCHITTEST, WPARAM(0), l);
        if r.0 as u32 != HTCLIENT {
            return r;
        }
        let mut pt = POINT { x: (l.0 & 0xFFFF) as i16 as i32, y: ((l.0 >> 16) & 0xFFFF) as i16 as i32 };
        let _ = ScreenToClient(hwnd, &mut pt);
        if !IsZoomed(hwnd).as_bool() && pt.y < border(dpi) {
            return LRESULT(HTTOP as isize);
        }
        let s = dpi as f32 / 96.0;
        let (x, y) = (pt.x as f32 / s, pt.y as f32 / s);
        if y < TITLE_H && button_at(width, x).is_none() {
            return LRESULT(HTCAPTION as isize);
        }
        LRESULT(HTCLIENT as isize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_buttons_right_to_left() {
        assert_eq!(button_at(1000.0, 999.0), Some(Target::Close));
        assert_eq!(button_at(1000.0, 955.0), Some(Target::Close));
        assert_eq!(button_at(1000.0, 953.0), Some(Target::Max));
        assert_eq!(button_at(1000.0, 870.0), Some(Target::Min));
        assert_eq!(button_at(1000.0, 861.0), None);
        assert_eq!(button_at(1000.0, 1000.0), None);
    }
}
