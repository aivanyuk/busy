//! Finding our place in explorer's taskbar: its windows, and the free span between task buttons and tray.

use busy_core::{Anchor, Config};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::MapWindowPoints;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, GetWindowRect};
use windows::core::{PCWSTR, w};

pub(super) fn find_tray() -> Option<HWND> {
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None).ok() }
}

fn child(parent: HWND, class: PCWSTR) -> Option<HWND> {
    unsafe { FindWindowExW(Some(parent), None, class, None).ok() }
}

/// Window rect of `h` in `parent`'s client coordinates.
fn rect_in(h: HWND, parent: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    unsafe {
        GetWindowRect(h, &mut r).ok()?;
        let mut pts = [POINT { x: r.left, y: r.top }, POINT { x: r.right, y: r.bottom }];
        MapWindowPoints(None, Some(parent), &mut pts);
        Some(RECT { left: pts[0].x, top: pts[0].y, right: pts[1].x, bottom: pts[1].y })
    }
}

/// Where the widget may go along the taskbar, in taskbar client pixels: (anchor edge, room). The taskbar runs
/// along x, or along y when `vertical` (on the left or right edge), and so does everything here: for
/// `NearTray` the edge is the widget's right (bottom) side, for `Left` its left (top) side; `room` is the
/// length available beside the task buttons.
pub(super) fn slot(tray: HWND, cfg: &Config, scale: f32, client: &RECT, vertical: bool) -> (i32, i32) {
    let px = |dip: f32| (dip * scale).round() as i32;
    let span = |r: RECT| if vertical { (r.top, r.bottom) } else { (r.left, r.right) };
    let end = span(*client).1;
    let rect = |class| child(tray, class).and_then(|h| rect_in(h, tray)).map(span);
    // TrayNotifyWnd is kept in sync with the XAML notification area on Win11 (verified on 26200, and on 26300
    // with a vertical taskbar, where its top is the "show hidden icons" chevron's).
    let tray_left = rect(w!("TrayNotifyWnd")).map_or(end, |r| r.0);
    // The (hidden) legacy Start window still tracks the XAML Start button. With centered icons the
    // button group is symmetric around the taskbar center, which gives its right end; ReBarWindow32
    // (the legacy task list) is not kept in sync, so it is only a fallback for left alignment.
    let start = rect(w!("Start"));
    let left_aligned = start.is_none_or(|r| r.0 < px(40.0));
    let (tasks_left, tasks_right) = match start {
        Some(s) if !left_aligned => (s.0, end - s.0),
        _ => (0, rect(w!("ReBarWindow32")).map_or(end / 2, |r| r.1)),
    };
    let off = px(cfg.offset_px as f32);
    let gap = px(8.0);
    match cfg.anchor {
        Anchor::NearTray => {
            let edge = tray_left - off;
            (edge, edge - tasks_right - gap)
        }
        Anchor::Left if left_aligned => {
            let edge = tasks_right + gap + off;
            (edge, tray_left - gap - edge)
        }
        Anchor::Left => (off, tasks_left - gap - off),
    }
}
