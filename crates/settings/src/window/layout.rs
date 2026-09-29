//! DPI-dependent font, control positions and window size.

use super::controls::send;
use super::{STYLE, Ui};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, SystemParametersInfoForDpi};
use windows::Win32::UI::WindowsAndMessaging::*;

impl Ui {
    pub(super) fn set_font(&self) {
        unsafe {
            let mut ncm = NONCLIENTMETRICSW { cbSize: size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
            let pv = Some((&mut ncm as *mut NONCLIENTMETRICSW).cast());
            let _ = SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS.0, ncm.cbSize, pv, 0, self.dpi.get());
            let font = CreateFontIndirectW(&ncm.lfMessageFont);
            for &(h, _) in &self.ctl.all {
                send(h, WM_SETFONT, font.0 as usize, 1);
            }
            let old = self.font.replace(font);
            if !old.is_invalid() {
                let _ = DeleteObject(old.into());
            }
        }
    }

    /// Positions all controls for the current DPI; returns the client size in pixels.
    pub(super) fn layout(&self) -> (i32, i32) {
        let d = self.dpi.get() as i32;
        let s = |v: i32| v * d / 96;
        let mv = |h: HWND, x: i32, y: i32, w: i32, hh: i32| unsafe {
            let _ = MoveWindow(h, x, y, w, hh, true);
        };
        let (m, h, gap, top, pad) = (s(12), s(23), s(8), s(22), s(12));
        let width = s(520);
        let (gx, gw) = (m, width - 2 * m);
        let (ix, iw) = (gx + pad, gw - 2 * pad);
        let bw = s(90);

        // Modules
        let lw = iw - bw - gap;
        let col = s(72);
        for (i, cx) in [lw - s(4) - 3 * col, col, col, col].into_iter().enumerate() {
            send(self.ctl.list, LVM_SETCOLUMNWIDTH, i, cx as isize);
        }
        let mut y = m;
        let ly = y + top;
        // Size the list to fit all rows exactly: lay out tall, then measure the last row.
        mv(self.ctl.list, ix, ly, lw, s(1000));
        let mut last = RECT::default();
        send(
            self.ctl.list,
            LVM_GETITEMRECT,
            self.modules.borrow().len().saturating_sub(1),
            &mut last as *mut _ as isize,
        );
        let (mut wr, mut cr) = (RECT::default(), RECT::default());
        unsafe {
            let _ = GetWindowRect(self.ctl.list, &mut wr);
            let _ = GetClientRect(self.ctl.list, &mut cr);
        }
        let lh = last.bottom + (wr.bottom - wr.top) - (cr.bottom - cr.top);
        mv(self.ctl.list, ix, ly, lw, lh);
        mv(self.ctl.up, ix + iw - bw, ly, bw, h + s(2));
        mv(self.ctl.down, ix + iw - bw, ly + h + s(8), bw, h + s(2));
        let ey = ly + lh + gap;
        mv(self.ctl.taskbar, ix, ey, s(120), h);
        mv(self.ctl.flyout, ix + s(130), ey, s(115), h);
        mv(self.ctl.style, ix + iw - bw, ey, bw, s(200));
        mv(self.ctl.style_lbl, ix + iw - bw - s(48), ey, s(44), h);
        let gh = ey + h + pad - y;
        mv(self.ctl.groups[0], gx, y, gw, gh);
        y += gh + s(10);

        // Two-column label/control grid shared by Taskbar and General.
        let (c1l, c1c, c2l, c2c, lwid, cwid) = (ix, ix + s(110), ix + s(242), ix + s(352), s(104), s(120));
        let ry = y + top;
        mv(self.ctl.anchor_lbl, c1l, ry, lwid, h);
        mv(self.ctl.anchor, c1c, ry, cwid, s(200));
        mv(self.ctl.offset_lbl, c2l, ry, lwid, h);
        let (ew, sw) = (s(64), s(18));
        mv(self.ctl.offset, c2c, ry, ew, h);
        mv(self.ctl.spin, c2c + ew, ry, sw, h);
        mv(self.ctl.px_lbl, c2c + ew + sw + s(6), ry, s(30), h);
        let gh = top + h + pad;
        mv(self.ctl.groups[1], gx, y, gw, gh);
        y += gh + s(10);

        let row = |i: i32| y + top + i * (h + gap);
        mv(self.ctl.interval_lbl, c1l, row(0), lwid, h);
        mv(self.ctl.interval, c1c, row(0), cwid, s(200));
        mv(self.ctl.history_lbl, c2l, row(0), lwid, h);
        mv(self.ctl.history, c2c, row(0), cwid, s(200));
        mv(self.ctl.theme_lbl, c1l, row(1), lwid, h);
        mv(self.ctl.theme, c1c, row(1), cwid, s(200));
        mv(self.ctl.unit_lbl, c2l, row(1), lwid, h);
        mv(self.ctl.unit, c2c, row(1), cwid, s(200));
        mv(self.ctl.sensor_lbl, c1l, row(2), lwid, h);
        mv(self.ctl.sensor, c1c, row(2), ix + iw - c1c, h);
        mv(self.ctl.autostart, c1l, row(3), s(200), h);
        let gh = row(3) + h + pad - y;
        mv(self.ctl.groups[2], gx, y, gw, gh);
        y += gh + s(12);

        let (bw, bh) = (s(80), h + s(2));
        for (i, b) in [self.ctl.ok, self.ctl.cancel, self.ctl.apply].into_iter().enumerate() {
            mv(b, width - m - (3 - i as i32) * bw - (2 - i as i32) * gap, y, bw, bh);
        }
        (width, y + bh + m)
    }

    pub(super) fn frame_size(&self, cw: i32, ch: i32) -> (i32, i32) {
        let mut rc = RECT { left: 0, top: 0, right: cw, bottom: ch };
        let _ = unsafe { AdjustWindowRectExForDpi(&mut rc, STYLE, false, WINDOW_EX_STYLE(0), self.dpi.get()) };
        (rc.right - rc.left, rc.bottom - rc.top)
    }
}

pub(super) fn work_area() -> RECT {
    unsafe {
        let mon = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        mi.rcWork
    }
}
