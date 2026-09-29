//! The widget's tooltip (design `title`: "<Module>: <value>"), one tool covering the whole widget whose text
//! follows the cell under the pointer.

use busy_win::wide;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PWSTR;

pub(super) struct Tip {
    hwnd: HWND,
    /// The widget, which the tool covers.
    owner: HWND,
    text: String,
}

impl Tip {
    pub(super) fn create(owner: HWND) -> Option<Self> {
        // SAFETY: plain window creation; `ti` and the empty text buffer outlive the SendMessageW that copies them.
        unsafe {
            let icc =
                INITCOMMONCONTROLSEX { dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_WIN95_CLASSES };
            let _ = InitCommonControlsEx(&icc);
            // Unowned: an owner must be top-level, and the widget's top-level ancestor is explorer's taskbar.
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                TOOLTIPS_CLASSW,
                None,
                WINDOW_STYLE(WS_POPUP.0 | TTS_ALWAYSTIP | TTS_NOPREFIX),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Some(crate::win::hinstance()),
                None,
            )
            .ok()?;
            let mut empty = wide("");
            // TTF_SUBCLASS: the tooltip watches the widget's mouse messages itself.
            let ti = TTTOOLINFOW {
                cbSize: size_of::<TTTOOLINFOW>() as u32,
                uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                hwnd: owner,
                uId: owner.0 as usize,
                lpszText: PWSTR(empty.as_mut_ptr()),
                ..Default::default()
            };
            SendMessageW(hwnd, TTM_ADDTOOLW, Some(WPARAM(0)), Some(LPARAM(&ti as *const _ as isize)));
            Some(Self { hwnd, owner, text: String::new() })
        }
    }

    /// Sets the text (empty: no tooltip); a no-op when unchanged, so it can follow every render.
    pub(super) fn set(&mut self, text: &str) {
        if self.text == text {
            return;
        }
        text.clone_into(&mut self.text);
        let mut buf = wide(text);
        let ti = TTTOOLINFOW {
            cbSize: size_of::<TTTOOLINFOW>() as u32,
            hwnd: self.owner,
            uId: self.owner.0 as usize,
            lpszText: PWSTR(buf.as_mut_ptr()),
            ..Default::default()
        };
        // SAFETY: our own tooltip on this thread; `ti` and `buf` outlive the call, which copies the text.
        unsafe {
            SendMessageW(self.hwnd, TTM_UPDATETIPTEXTW, Some(WPARAM(0)), Some(LPARAM(&ti as *const _ as isize)));
        }
    }
}

impl Drop for Tip {
    fn drop(&mut self) {
        // SAFETY: destroys the window this struct created; a window already gone makes the call fail harmlessly.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
