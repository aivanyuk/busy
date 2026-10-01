//! Which screen edge the taskbar is on (Windows 11 26H2 lets it stand on the left or right), read from window
//! rects only: no message reaches explorer, so a hung explorer can't stall the caller.

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect};
use windows::core::w;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Edge {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

impl Edge {
    /// On the left or right: the taskbar is a column, and what it holds stacks top to bottom.
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// The edge of `monitor` that `bar`, a taskbar's rect, runs along: a taskbar taller than wide stands on the
/// left or right, and then the nearer side decides which.
pub fn edge_of(bar: RECT, monitor: RECT) -> Edge {
    let vertical = bar.bottom - bar.top > bar.right - bar.left;
    match vertical {
        true if bar.left - monitor.left <= monitor.right - bar.right => Edge::Left,
        true => Edge::Right,
        false if bar.top - monitor.top < monitor.bottom - bar.bottom => Edge::Top,
        false => Edge::Bottom,
    }
}

/// The edge of the taskbar window `tray` (`Shell_TrayWnd`); `Bottom` if it is gone.
pub fn window_edge(tray: HWND) -> Edge {
    let mut bar = RECT::default();
    let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: plain queries on a window handle that may be stale (they fail then); `bar` and `mi` are valid
    // out-pointers with `cbSize` set.
    let ok = unsafe {
        GetWindowRect(tray, &mut bar).is_ok()
            && GetMonitorInfoW(MonitorFromWindow(tray, MONITOR_DEFAULTTOPRIMARY), &mut mi).as_bool()
    };
    if ok { edge_of(bar, mi.rcMonitor) } else { Edge::default() }
}

/// The primary taskbar's edge; `Bottom` while explorer isn't running.
pub fn taskbar_edge() -> Edge {
    // SAFETY: looks a window up by class; no pointers are passed.
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.map_or(Edge::default(), window_edge)
}

#[cfg(test)]
mod tests {
    use super::{Edge, edge_of};
    use windows::Win32::Foundation::RECT;

    const MONITOR: RECT = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };

    fn bar(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT { left, top, right, bottom }
    }

    #[test]
    fn edges_follow_the_taskbar_rect() {
        assert_eq!(edge_of(bar(0, 1032, 1920, 1080), MONITOR), Edge::Bottom);
        assert_eq!(edge_of(bar(0, 0, 1920, 48), MONITOR), Edge::Top);
        assert_eq!(edge_of(bar(0, 0, 48, 1080), MONITOR), Edge::Left);
        assert_eq!(edge_of(bar(1872, 0, 1920, 1080), MONITOR), Edge::Right);
        // A second monitor to the left of the primary one.
        let left_monitor = RECT { left: -1920, top: 0, right: 0, bottom: 1080 };
        assert_eq!(edge_of(bar(-48, 0, 0, 1080), left_monitor), Edge::Right);
        assert!(Edge::Left.is_vertical() && Edge::Right.is_vertical());
        assert!(!Edge::Bottom.is_vertical() && !Edge::Top.is_vertical());
    }
}
