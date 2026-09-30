//! The design's controls, one file per kind: each knows its size, draws itself and says which of its parts is
//! under the pointer. None of them changes state: input on a control becomes a `model::Edit` in `input.rs`.

pub(super) mod button;
pub(super) mod dropdown;
pub(super) mod nav;
pub(super) mod order;
pub(super) mod preview;
pub(super) mod segmented;
pub(super) mod swatch;
pub(super) mod toggle;

use busy_ui::render::{Canvas, Rect};
use busy_ui::theme::{Color, Theme};

/// A control face (design `--ctl` with a `--ctl-line` border and a darker `bottom` edge), radius 4.
pub(super) fn face(cv: &Canvas, r: Rect, fill: Color, bottom: Color, t: &Theme) {
    cv.round(r, 4.0, fill);
    cv.round_outline(r, 4.0, t.ctl_line);
    cv.fill(Rect::new(r.x + 4.0, r.bottom() - 1.0, r.w - 8.0, 1.0), bottom);
}

/// The accent pill left of a selected nav item or option: 3 wide, radius 2, vertically centered.
pub(super) fn pill(cv: &Canvas, r: Rect, h: f32, t: &Theme) {
    cv.round(Rect::new(r.x, r.y + (r.h - h) / 2.0, 3.0, h), 1.5, t.accent);
}
