//! Theme application and dark-mode painting (checkboxes, group boxes, ListView header).

use super::controls::{Kind, checked, send, window_text};
use super::{Ui, ui};
use crate::dark;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, OpenThemeDataForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

impl Ui {
    pub(super) fn apply_theme(&self) {
        let d = self.dark.get();
        dark::title_bar(self.hwnd, d);
        unsafe {
            for &(h, k) in &self.ctl.all {
                let name = match (k, d) {
                    (Kind::Push | Kind::Check | Kind::List | Kind::Spin, true) => w!("DarkMode_Explorer"),
                    (Kind::Combo | Kind::Edit, true) => w!("DarkMode_CFD"),
                    (Kind::List, false) => w!("Explorer"),
                    _ => PCWSTR::null(),
                };
                let _ = SetWindowTheme(h, name, PCWSTR::null());
            }
            let header = HWND(send(self.ctl.list, LVM_GETHEADER, 0, 0) as _);
            let _ = SetWindowTheme(header, if d { w!("DarkMode_ItemsView") } else { PCWSTR::null() }, PCWSTR::null());
            let (bk, fg) = if d {
                (dark::FIELD, dark::TEXT)
            } else {
                (COLORREF(GetSysColor(COLOR_WINDOW)), COLORREF(GetSysColor(COLOR_WINDOWTEXT)))
            };
            send(self.ctl.list, LVM_SETBKCOLOR, 0, bk.0 as isize);
            send(self.ctl.list, LVM_SETTEXTBKCOLOR, 0, bk.0 as isize);
            send(self.ctl.list, LVM_SETTEXTCOLOR, 0, fg.0 as isize);
            let _ = RedrawWindow(Some(self.hwnd), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_FRAME);
        }
    }

    pub(super) fn update_dark(&self) {
        let d = dark::wanted(self.applied.borrow().theme);
        if d != self.dark.get() {
            self.dark.set(d);
            self.apply_theme();
        }
    }

    pub(super) fn bg_brush(&self) -> HBRUSH {
        if self.dark.get() { self.bg } else { unsafe { GetSysColorBrush(COLOR_BTNFACE) } }
    }

    /// Themed checkboxes ignore WM_CTLCOLORSTATIC text color, so in dark mode draw them ourselves.
    pub(super) fn draw_check(&self, nm: &NMCUSTOMDRAW) -> LRESULT {
        if nm.dwDrawStage != CDDS_PREPAINT {
            return LRESULT(CDRF_DODEFAULT as isize);
        }
        let (h, hdc, rc) = (nm.hdr.hwndFrom, nm.hdc, nm.rc);
        unsafe {
            FillRect(hdc, &rc, self.bg);
            let dpi = GetDpiForWindow(h);
            let enabled = IsWindowEnabled(h).as_bool();
            let st = nm.uItemState;
            let base = if checked(h) { CBS_CHECKEDNORMAL.0 } else { CBS_UNCHECKEDNORMAL.0 };
            let state = base
                + if !enabled {
                    3
                } else if st.contains(CDIS_SELECTED) {
                    2
                } else if st.contains(CDIS_HOT) {
                    1
                } else {
                    0
                };
            let theme = OpenThemeDataForDpi(Some(h), w!("Button"), dpi);
            let size = GetThemePartSize(theme, Some(hdc), BP_CHECKBOX.0, state, None, TS_DRAW)
                .unwrap_or(SIZE { cx: 13 * dpi as i32 / 96, cy: 13 * dpi as i32 / 96 });
            let top = rc.top + (rc.bottom - rc.top - size.cy) / 2;
            let bx = RECT { left: rc.left, top, right: rc.left + size.cx, bottom: top + size.cy };
            let _ = DrawThemeBackground(theme, hdc, BP_CHECKBOX.0, state, &bx, None);
            let _ = CloseThemeData(theme);

            let old = SelectObject(hdc, HGDIOBJ(send(h, WM_GETFONT, 0, 0) as _));
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, if enabled { dark::TEXT } else { dark::DIM });
            let mut text = window_text(h);
            let ui_state = send(h, WM_QUERYUISTATE, 0, 0) as u32;
            let mut fmt = DT_SINGLELINE | DT_VCENTER | DT_LEFT;
            if ui_state & UISF_HIDEACCEL != 0 {
                fmt |= DT_HIDEPREFIX;
            }
            let mut tr = RECT { left: bx.right + 4 * dpi as i32 / 96, ..rc };
            DrawTextW(hdc, &mut text, &mut tr, fmt);
            if st.contains(CDIS_FOCUS) && ui_state & UISF_HIDEFOCUS == 0 {
                let mut fr = tr;
                DrawTextW(hdc, &mut text, &mut fr, fmt | DT_CALCRECT);
                let th = fr.bottom - fr.top;
                fr.top = rc.top + (rc.bottom - rc.top - th) / 2 - 1;
                fr.bottom = fr.top + th + 2;
                fr.left -= 2;
                fr.right += 2;
                let _ = DrawFocusRect(hdc, &fr);
            }
            SelectObject(hdc, old);
        }
        LRESULT(CDRF_SKIPDEFAULT as isize)
    }
}

/// Dark-mode group box: themed group boxes ignore text color, so paint frame and caption ourselves.
pub(super) unsafe extern "system" fn group_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM, id: usize, _: usize) -> LRESULT {
    unsafe {
        match m {
            WM_PAINT => match ui().filter(|u| u.dark.get()) {
                Some(u) => {
                    paint_group(h, u.bg);
                    LRESULT(0)
                }
                None => DefSubclassProc(h, m, w, l),
            },
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(h, Some(group_proc), id);
                DefSubclassProc(h, m, w, l)
            }
            _ => DefSubclassProc(h, m, w, l),
        }
    }
}

fn paint_group(h: HWND, bg: HBRUSH) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(h, &mut ps);
        let s = |v: i32| v * GetDpiForWindow(h) as i32 / 96;
        let mut rc = RECT::default();
        let _ = GetClientRect(h, &mut rc);
        let old_font = SelectObject(hdc, HGDIOBJ(send(h, WM_GETFONT, 0, 0) as _));
        let mut text = window_text(h);
        let mut fmt = DT_SINGLELINE | DT_LEFT;
        if send(h, WM_QUERYUISTATE, 0, 0) as u32 & UISF_HIDEACCEL != 0 {
            fmt |= DT_HIDEPREFIX;
        }
        let mut tr = RECT::default();
        DrawTextW(hdc, &mut text, &mut tr, fmt | DT_CALCRECT);
        let (tw, th) = (tr.right - tr.left, tr.bottom - tr.top);

        let pen = CreatePen(PS_SOLID, 1, dark::LINE);
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        let r = s(8);
        let _ = RoundRect(hdc, rc.left, rc.top + th / 2, rc.right, rc.bottom, r, r);
        let mut tr = RECT { left: rc.left + s(6), top: rc.top, right: rc.left + s(6) + tw + s(8), bottom: rc.top + th };
        FillRect(hdc, &tr, bg);
        tr.left += s(4);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, dark::TEXT);
        DrawTextW(hdc, &mut text, &mut tr, fmt);

        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_font);
        let _ = DeleteObject(pen.into());
        let _ = EndPaint(h, &ps);
    }
}

/// Dark-mode ListView header text color (the header notifies its parent, the ListView).
pub(super) unsafe extern "system" fn list_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM, id: usize, _: usize) -> LRESULT {
    unsafe {
        if m == WM_NOTIFY && ui().is_some_and(|u| u.dark.get()) && (*(l.0 as *const NMHDR)).code == NM_CUSTOMDRAW {
            let nm = &*(l.0 as *const NMCUSTOMDRAW);
            match nm.dwDrawStage {
                CDDS_PREPAINT => return LRESULT(CDRF_NOTIFYITEMDRAW as isize),
                CDDS_ITEMPREPAINT => {
                    SetTextColor(nm.hdc, dark::TEXT);
                    return LRESULT(CDRF_DODEFAULT as isize);
                }
                _ => {}
            }
        }
        if m == WM_NCDESTROY {
            let _ = RemoveWindowSubclass(h, Some(list_proc), id);
        }
        DefSubclassProc(h, m, w, l)
    }
}
