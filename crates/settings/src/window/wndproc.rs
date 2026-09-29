//! The window procedure and its message dispatch.

use super::layout::{MIN_H, MIN_W};
use super::worker::{Job, WM_APP_REPLY};
use super::{UI, Ui, frame, ui};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

fn x_of(l: LPARAM) -> i32 {
    (l.0 & 0xFFFF) as i16 as i32
}

fn y_of(l: LPARAM) -> i32 {
    ((l.0 >> 16) & 0xFFFF) as i16 as i32
}

impl Ui {
    fn handle(&self, m: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
        match m {
            WM_NCCALCSIZE if w.0 != 0 => Some(frame::calc_size(self.hwnd, w, l, self.dpi.get())),
            WM_NCHITTEST => Some(frame::hit_test(self.hwnd, l, self.dpi.get(), self.view.borrow().w)),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                // SAFETY: our window; BeginPaint/EndPaint pair around the draw.
                unsafe {
                    BeginPaint(self.hwnd, &mut ps);
                    self.render();
                    let _ = EndPaint(self.hwnd, &ps);
                }
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_SIZE => {
                self.resized();
                Some(LRESULT(0))
            }
            WM_GETMINMAXINFO => {
                let s = self.scale();
                // SAFETY: for WM_GETMINMAXINFO, lParam points to a MINMAXINFO valid for the message.
                let mmi = unsafe { &mut *(l.0 as *mut MINMAXINFO) };
                mmi.ptMinTrackSize = POINT { x: (MIN_W * s) as i32, y: (MIN_H * s) as i32 };
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                self.on_move(x_of(l), y_of(l));
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.on_leave();
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                self.on_down(x_of(l), y_of(l));
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                self.on_up(x_of(l), y_of(l));
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => {
                self.on_wheel((w.0 >> 16) as u16 as i16);
                Some(LRESULT(0))
            }
            WM_KEYDOWN if w.0 as u16 == VK_ESCAPE.0 => {
                self.on_escape();
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                self.dpi.set(w.0 as u16 as u32);
                // SAFETY: for WM_DPICHANGED, lParam points to the suggested window RECT.
                let r = unsafe { *(l.0 as *const RECT) };
                // SAFETY: our window; the rect comes from the system.
                unsafe {
                    let _ = SetWindowPos(
                        self.hwnd,
                        None,
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                self.resized();
                Some(LRESULT(0))
            }
            WM_SETTINGCHANGE => {
                // SAFETY: for WM_SETTINGCHANGE, a non-null lParam is a NUL-terminated string.
                if l.0 != 0 && unsafe { PCWSTR(l.0 as _).to_string() }.is_ok_and(|s| s == "ImmersiveColorSet") {
                    self.worker.submit(Job::Theme);
                }
                None
            }
            WM_APP_REPLY => {
                for r in self.worker.replies() {
                    self.on_reply(r);
                }
                Some(LRESULT(0))
            }
            WM_ACTIVATE if w.0 as u16 as u32 == WA_INACTIVE => {
                // An open popup closes with the window's activation, like a system dropdown.
                if self.view.borrow_mut().popup.take().is_some() {
                    self.invalidate();
                }
                None
            }
            WM_CLOSE => {
                self.close();
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

pub(super) extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if let Some(r) = ui().filter(|u| u.hwnd == h).and_then(|u| u.handle(m, w, l)) {
        return r;
    }
    // SAFETY: default handling with the message's own parameters.
    unsafe { DefWindowProcW(h, m, w, l) }
}
