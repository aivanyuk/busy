//! Setup's input and its window: entering setup (fixed size, Close only, every reading sampled), the pointer
//! and the keyboard, and finishing. Choices are edits like the Settings pages' (`model::Edit`), so they apply
//! live and the host persists them.

use super::{Layout, Target, W};
use crate::window::choices::Pick;
use crate::window::model::{Edit, Flag};
use crate::window::{Mode, Ui, work_area};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

impl Ui {
    pub(in crate::window) fn in_setup(&self) -> bool {
        self.mode.get() == Mode::Setup
    }

    /// Turns the window into setup: fixed size, centered on the primary work area, Close only; samples every
    /// reading a card shows.
    pub(in crate::window) fn enter_setup(&self) {
        // Measured on a normal window: a minimized one has an icon's size, and a minimized maximized one
        // restores to maximized first.
        for _ in 0..2 {
            // SAFETY: our live window.
            unsafe {
                if IsIconic(self.hwnd).as_bool() || IsZoomed(self.hwnd).as_bool() {
                    let _ = ShowWindow(self.hwnd, SW_RESTORE);
                }
            }
        }
        self.mode.set(Mode::Setup);
        self.view.borrow_mut().popup = None;
        let layout = Layout::new(&self.gfx, &self.fonts, super::cards(&self.cfg.borrow()).len());
        {
            let mut v = self.setup.borrow_mut();
            v.layout = Some(layout);
            v.focus = Some(Target::Card(0));
            v.focus_visible = false;
        }
        let shown: Vec<_> = super::cards(&self.cfg.borrow()).into_iter().map(|(m, _)| m).collect();
        self.host.shown(&shown);
        // The readings the host has now; new ones follow with each sample (`refresh`).
        self.host.with_data(&mut |snap, _| self.setup_samples(snap));
        // SAFETY: our live window; the title outlives the call.
        let style = unsafe {
            let _ = SetWindowTextW(self.hwnd, w!("busy"));
            GetWindowLongPtrW(self.hwnd, GWL_STYLE)
        };
        let fixed = style & !((WS_THICKFRAME.0 | WS_MAXIMIZEBOX.0) as isize);
        // SAFETY: our live window; a style without the sizing frame and maximize box.
        unsafe { SetWindowLongPtrW(self.hwnd, GWL_STYLE, fixed) };
        self.fit_setup(true);
    }

    /// Sizes the window so its client is setup's layout at the current DPI (after entering setup, and after a
    /// DPI change, whose suggested size is for a standard frame); `center` centers it on the primary work area,
    /// else its top-left stays.
    pub(in crate::window) fn fit_setup(&self, center: bool) {
        let Some(h) = self.setup.borrow().layout.as_ref().map(|l| l.h) else { return };
        let s = self.scale();
        let (cw, ch) = ((W * s).round() as i32, (h * s).round() as i32);
        let (mut wr, mut cr) = (RECT::default(), RECT::default());
        // SAFETY: our live window; the rect out-pointers are valid for the calls.
        unsafe {
            let flags = SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
            let _ = SetWindowPos(self.hwnd, None, 0, 0, cw, ch, flags);
            let _ = GetWindowRect(self.hwnd, &mut wr);
            let _ = GetClientRect(self.hwnd, &mut cr);
        }
        // The frame's sides and bottom are Windows'; grow the window by them so the client is the layout.
        let (ww, wh) = (cw + (wr.right - wr.left) - cr.right, ch + (wr.bottom - wr.top) - cr.bottom);
        let (x, y) = if center {
            let work = work_area();
            (work.left + (work.right - work.left - ww) / 2, work.top + (work.bottom - work.top - wh) / 2)
        } else {
            (wr.left, wr.top)
        };
        // SAFETY: our live window.
        let _ = unsafe { SetWindowPos(self.hwnd, None, x, y, ww, wh, SWP_NOZORDER | SWP_NOACTIVATE) };
        self.invalidate();
    }

    /// A finish that waits for an autostart write: the page takes no more input meanwhile.
    fn finishing(&self) -> bool {
        self.pending.get() == Some(true)
    }

    /// Skip, Start monitoring, Close, Esc: all keep the choices made and mark setup done.
    pub(in crate::window) fn finish_setup(&self) {
        let mut c = self.cfg.borrow().clone();
        c.onboarded = true;
        self.commit(c);
        self.close();
    }

    fn setup_hit(&self, x: i32, y: i32) -> Option<Target> {
        let s = self.scale();
        self.setup.borrow().layout.as_ref().and_then(|l| l.hit(x as f32 / s, y as f32 / s))
    }

    pub(in crate::window) fn setup_move(&self, x: i32, y: i32) {
        let hit = self.setup_hit(x, y);
        self.setup_hover(hit);
    }

    pub(in crate::window) fn setup_hover(&self, hit: Option<Target>) {
        let mut v = self.setup.borrow_mut();
        if v.hover != hit {
            v.hover = hit;
            drop(v);
            self.invalidate();
        }
    }

    pub(in crate::window) fn setup_down(&self, x: i32, y: i32) {
        let hit = self.setup_hit(x, y);
        let mut v = self.setup.borrow_mut();
        v.pressed = hit;
        v.focus_visible = false;
        if let Some(t) = hit.filter(|&t| t != Target::Close) {
            v.focus = Some(t);
        }
    }

    pub(in crate::window) fn setup_up(&self, x: i32, y: i32) {
        let hit = self.setup_hit(x, y);
        let pressed = self.setup.borrow_mut().pressed.take();
        if let Some(t) = pressed.filter(|&p| Some(p) == hit) {
            self.setup_activate(t);
        }
    }

    /// What a click (or Space) on `t` does.
    pub(in crate::window) fn setup_activate(&self, t: Target) {
        if self.finishing() {
            return;
        }
        let cfg = self.cfg.borrow().clone();
        match t {
            Target::Close | Target::Skip | Target::Start => self.finish_setup(),
            Target::Card(i) => {
                if let Some(&(m, on)) = super::cards(&cfg).get(i) {
                    self.edit(Edit::Flag(Flag::Taskbar(m), !on));
                }
            }
            Target::Place(i) => {
                if let Some(&(a, ..)) = super::PLACES.get(i) {
                    self.edit(Edit::Pick(Pick::Anchor(a)));
                }
            }
            Target::Startup => self.edit(Edit::Flag(Flag::Autostart, !cfg.autostart)),
        }
        self.invalidate();
    }

    /// Moves the keyboard focus to `t`, with its ring.
    pub(in crate::window) fn setup_focus(&self, t: Target) {
        let mut v = self.setup.borrow_mut();
        v.focus = Some(t);
        v.focus_visible = true;
        drop(v);
        self.invalidate();
    }

    /// `WM_KEYDOWN` in setup: Tab order, Space on the focused target, Enter and Esc finish, arrows move among
    /// the cards (a 3-wide grid) and pick the neighbouring position. A held Space, Enter or Esc acts once: the
    /// Enter that pressed Settings' "Run setup" must not finish setup as it repeats.
    pub(in crate::window) fn setup_key(&self, vk: VIRTUAL_KEY, alt: bool, repeat: bool) -> bool {
        if alt {
            return false;
        }
        if self.finishing() || (repeat && matches!(vk, VK_SPACE | VK_RETURN | VK_ESCAPE)) {
            return true;
        }
        let stops = super::stops(&self.cfg.borrow());
        let focus = self.setup.borrow().focus;
        match vk {
            VK_TAB => {
                // SAFETY: reads the calling thread's keyboard state.
                let back = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
                // Positions are one stop, whichever is focused.
                let at = focus.and_then(|f| {
                    stops.iter().position(|&s| s == f || matches!((s, f), (Target::Place(_), Target::Place(_))))
                });
                let n = stops.len();
                let next = match (at, back) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                };
                self.setup_focus(stops[next]);
            }
            VK_SPACE => match focus {
                Some(t) => {
                    self.setup.borrow_mut().focus_visible = true;
                    self.setup_activate(t);
                }
                None => self.setup_focus(stops[0]),
            },
            VK_RETURN | VK_ESCAPE => self.finish_setup(),
            VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN => {
                let n = super::cards(&self.cfg.borrow()).len();
                match focus {
                    Some(Target::Card(i)) => {
                        let j = match vk {
                            VK_LEFT => i.checked_sub(1),
                            VK_RIGHT => Some(i + 1),
                            VK_UP => i.checked_sub(super::COLS),
                            _ => Some(i + super::COLS),
                        };
                        if let Some(j) = j.filter(|&j| j < n) {
                            self.setup_focus(Target::Card(j));
                        }
                    }
                    Some(Target::Place(i)) => {
                        let j = if matches!(vk, VK_LEFT | VK_UP) { i.saturating_sub(1) } else { (i + 1).min(1) };
                        self.setup_focus(Target::Place(j));
                        self.setup_activate(Target::Place(j));
                    }
                    Some(_) => {}
                    None => self.setup_focus(stops[0]),
                }
            }
            _ => return false,
        }
        true
    }
}
