//! The window procedure and its message dispatch.

use super::controls::ID_OK;
use super::{UI, Ui, ui};
use crate::dark;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

impl Ui {
    fn handle(&self, m: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
        unsafe {
            match m {
                WM_COMMAND => {
                    self.command(w.0 as u16, (w.0 >> 16) as u16 as u32);
                    Some(LRESULT(0))
                }
                WM_NOTIFY => self.notify(l),
                WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                    let hdc = HDC(w.0 as _);
                    let field = m == WM_CTLCOLOREDIT || m == WM_CTLCOLORLISTBOX;
                    if self.dark.get() {
                        SetTextColor(hdc, dark::TEXT);
                        SetBkColor(hdc, if field { dark::FIELD } else { dark::BG });
                        Some(LRESULT((if field { self.field } else { self.bg }).0 as isize))
                    } else if !field {
                        SetTextColor(hdc, COLORREF(GetSysColor(COLOR_WINDOWTEXT)));
                        SetBkColor(hdc, COLORREF(GetSysColor(COLOR_BTNFACE)));
                        Some(LRESULT(self.bg_brush().0 as isize))
                    } else {
                        None
                    }
                }
                WM_ERASEBKGND => {
                    let mut rc = RECT::default();
                    let _ = GetClientRect(self.hwnd, &mut rc);
                    FillRect(HDC(w.0 as _), &rc, self.bg_brush());
                    Some(LRESULT(1))
                }
                // Lets IsDialogMessage treat Enter as OK.
                DM_GETDEFID => Some(LRESULT(((DC_HASDEFID << 16) | ID_OK as u32) as isize)),
                WM_ACTIVATE => {
                    if w.0 as u16 as u32 == WA_INACTIVE {
                        let f = GetFocus();
                        if IsChild(self.hwnd, f).as_bool() {
                            self.focus.set(f);
                        }
                    } else {
                        let _ = SetFocus(Some(self.focus.get()));
                    }
                    Some(LRESULT(0))
                }
                WM_DPICHANGED => {
                    self.dpi.set(w.0 as u16 as u32);
                    self.set_font();
                    let (cw, ch) = self.layout();
                    let (ww, wh) = self.frame_size(cw, ch);
                    let r = &*(l.0 as *const RECT);
                    let _ = SetWindowPos(self.hwnd, None, r.left, r.top, ww, wh, SWP_NOZORDER | SWP_NOACTIVATE);
                    Some(LRESULT(0))
                }
                WM_SETTINGCHANGE => {
                    if l.0 != 0 && PCWSTR(l.0 as _).to_string().is_ok_and(|s| s == "ImmersiveColorSet") {
                        self.update_dark();
                    }
                    None
                }
                WM_CLOSE => {
                    let _ = DestroyWindow(self.hwnd);
                    Some(LRESULT(0))
                }
                WM_NCDESTROY => {
                    UI.take();
                    None
                }
                _ => None,
            }
        }
    }
}

pub(super) extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if let Some(r) = ui().filter(|u| u.hwnd == h).and_then(|u| u.handle(m, w, l)) {
        return r;
    }
    unsafe { DefWindowProcW(h, m, w, l) }
}
