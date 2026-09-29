//! Pointer input: hover (repainting only when the hovered target changes), clicks (a press and release on the
//! same target) and the wheel. A click on a control becomes a `model::Edit`; everything else is view state.

use super::Ui;
use super::layout::Target;
use super::model::{Control, Edit, Item};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Content scrolled per wheel notch (three 16-DIP lines).
const WHEEL_STEP: f32 = 48.0;

impl Ui {
    fn dips(&self, x: i32, y: i32) -> (f32, f32) {
        let s = self.scale();
        (x as f32 / s, y as f32 / s)
    }

    pub(super) fn on_move(&self, x: i32, y: i32) {
        if !self.tracking.get() {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: `tme` is valid for the call and names our window.
            if unsafe { TrackMouseEvent(&mut tme) }.is_ok() {
                self.tracking.set(true);
            }
        }
        let (x, y) = self.dips(x, y);
        let hit = self.view.borrow().hit(x, y);
        self.set_hover(hit);
    }

    pub(super) fn on_leave(&self) {
        self.tracking.set(false);
        self.set_hover(None);
    }

    fn set_hover(&self, hit: Option<Target>) {
        let mut v = self.view.borrow_mut();
        if v.hover != hit {
            v.hover = hit;
            drop(v);
            self.invalidate();
        }
    }

    pub(super) fn on_down(&self, x: i32, y: i32) {
        let (x, y) = self.dips(x, y);
        let hit = self.view.borrow().hit(x, y);
        // A press outside an open popup only closes it.
        if hit.is_none() && self.view.borrow().popup.is_some() {
            self.view.borrow_mut().popup = None;
            self.pressed.set(None);
            self.on_move_dips(x, y);
            return;
        }
        self.pressed.set(hit);
        // SAFETY: our live window.
        unsafe { SetCapture(self.hwnd) };
    }

    pub(super) fn on_up(&self, x: i32, y: i32) {
        // SAFETY: releases the capture taken in `on_down`, if any.
        let _ = unsafe { ReleaseCapture() };
        let (x, y) = self.dips(x, y);
        let hit = self.view.borrow().hit(x, y);
        if let Some(t) = self.pressed.take().filter(|&p| Some(p) == hit) {
            self.activate(t);
        }
    }

    /// Re-evaluates hover at a point after the view changed under the pointer.
    fn on_move_dips(&self, x: f32, y: f32) {
        let hit = self.view.borrow().hit(x, y);
        self.view.borrow_mut().hover = hit;
        self.invalidate();
    }

    pub(super) fn on_wheel(&self, delta: i16) {
        let dy = -(delta as f32) / WHEEL_DELTA as f32 * WHEEL_STEP;
        let mut v = self.view.borrow_mut();
        // Over an open popup the wheel scrolls its options, one per notch.
        let popup = v.popup.as_ref().and_then(|p| Some((v.options()?.0.len(), p.visible, p.first)));
        if let Some((n, visible, first)) = popup {
            let step = if dy > 0.0 { 1 } else { -1 };
            let next = (first as i64 + step).clamp(0, n.saturating_sub(visible) as i64) as usize;
            if let Some(p) = v.popup.as_mut() {
                p.first = next;
            }
        } else if !v.scroll_by(dy) {
            return;
        }
        drop(v);
        // The content moved under the pointer: hover follows where it now is.
        let mut pt = windows::Win32::Foundation::POINT::default();
        // SAFETY: `pt` is a valid in/out pointer; `self.hwnd` is our live window.
        let at = unsafe {
            GetCursorPos(&mut pt).is_ok() && windows::Win32::Graphics::Gdi::ScreenToClient(self.hwnd, &mut pt).as_bool()
        };
        if at {
            let (x, y) = self.dips(pt.x, pt.y);
            self.on_move_dips(x, y);
        } else {
            self.invalidate();
        }
    }

    fn activate(&self, t: Target) {
        match t {
            // SAFETY: our live window.
            Target::Min => unsafe {
                let _ = ShowWindow(self.hwnd, SW_MINIMIZE);
            },
            // SAFETY: our live window.
            Target::Max => unsafe {
                let zoomed = IsZoomed(self.hwnd).as_bool();
                let _ = ShowWindow(self.hwnd, if zoomed { SW_RESTORE } else { SW_MAXIMIZE });
            },
            Target::Close => self.close(),
            Target::Nav(i) => {
                let page = self.view.borrow().nav.get(i).copied();
                if let Some(p) = page {
                    self.show_page(p);
                }
            }
            Target::Ctl(i) => self.control(i),
            Target::Up(i) | Target::Down(i) => {
                let m = match self.view.borrow().items.iter().find_map(|it| match it {
                    Item::Order(list) => list.get(i).map(|&(m, _)| m),
                    _ => None,
                }) {
                    Some(m) => m,
                    None => return,
                };
                self.edit(Edit::Move(m, matches!(t, Target::Up(_))));
            }
            Target::Opt(k) => {
                let pick = {
                    let mut v = self.view.borrow_mut();
                    let pick = v.options().and_then(|(opts, _)| opts.get(k)).map(|o| o.pick.clone());
                    v.popup = None;
                    pick
                };
                match pick {
                    Some(p) => self.edit(Edit::Pick(p)),
                    None => self.invalidate(),
                }
            }
        }
    }

    /// A click on row `i`'s control: a toggle flips, a dropdown opens (or closes).
    fn control(&self, i: usize) {
        let mut v = self.view.borrow_mut();
        let edit = match v.items.get(i) {
            Some(Item::Row(r)) => match &r.control {
                Control::Toggle(flag, on) => Some(Edit::Flag(*flag, !on)),
                Control::Dropdown(..) => None,
            },
            _ => return,
        };
        match edit {
            Some(e) => {
                drop(v);
                self.edit(e);
            }
            None => {
                if v.popup.as_ref().is_some_and(|p| p.item == i) {
                    v.popup = None;
                } else {
                    v.open_popup(i, &self.gfx, &self.fonts);
                }
                drop(v);
                self.invalidate();
            }
        }
    }

    /// Esc: closes the open popup, else the window.
    pub(super) fn on_escape(&self) {
        if self.view.borrow_mut().popup.take().is_some() {
            self.invalidate();
        } else {
            self.close();
        }
    }
}
