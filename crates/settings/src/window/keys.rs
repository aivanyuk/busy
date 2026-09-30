//! Keyboard input: Tab order over the nav and the page's controls, Space/Enter to activate, arrows within
//! the nav, segments, swatches and an open popup, F4 / Alt+↓ to open a dropdown, Page Up/Down to scroll, Esc
//! to close the popup (else the window). The focus ring shows once the keyboard is used (`focus_visible`).

use super::Ui;
use super::layout::Target;
use super::model::{Control, Edit, Item};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

impl Ui {
    /// `WM_KEYDOWN` (and `WM_SYSKEYDOWN` with `alt`; `repeat` for an auto-repeat); false leaves the key to the
    /// default handling.
    pub(super) fn on_key(&self, vk: VIRTUAL_KEY, alt: bool, repeat: bool) -> bool {
        // SAFETY: reads the calling thread's keyboard state.
        let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
        if self.in_setup() {
            return self.setup_key(vk, alt, repeat);
        }
        if alt && vk != VK_DOWN {
            return false;
        }
        if self.view.borrow().popup.is_some() {
            return self.popup_key(vk);
        }
        // SAFETY: reads the calling thread's keyboard state.
        if vk == VK_F && unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0 {
            self.view.borrow_mut().set_focus(Target::Search);
            self.invalidate();
            return true;
        }
        if self.view.borrow().focus == Some(Target::Search) && self.search_key(vk) {
            return true;
        }
        match vk {
            VK_TAB => self.tab(shift),
            VK_ESCAPE => self.close(),
            VK_SPACE | VK_RETURN => {
                if let Some(t) = self.focused() {
                    self.press(t);
                }
            }
            VK_UP | VK_DOWN | VK_LEFT | VK_RIGHT => self.arrow(vk, alt),
            VK_F4 => {
                let is_dropdown = |i| matches!(self.view.borrow().items.get(i), Some(Item::Row(r)) if matches!(r.control, Control::Dropdown(..)));
                if let Some(Target::Ctl(i)) = self.focused()
                    && is_dropdown(i)
                {
                    self.control(i);
                }
            }
            VK_PRIOR | VK_NEXT => {
                let mut v = self.view.borrow_mut();
                let page = v.pane().h * 0.9;
                v.scroll_by(if vk == VK_PRIOR { -page } else { page });
                drop(v);
                self.invalidate();
            }
            _ => return false,
        }
        true
    }

    fn focused(&self) -> Option<Target> {
        let mut v = self.view.borrow_mut();
        v.focus_visible = true;
        v.focus
    }

    /// Moves the focus to the next (or, with Shift, previous) stop, wrapping.
    fn tab(&self, back: bool) {
        let mut v = self.view.borrow_mut();
        let stops = v.stops();
        if stops.is_empty() {
            return;
        }
        let at = v.focus.and_then(|f| stops.iter().position(|&s| s == f));
        let n = stops.len();
        let next = match (at, back) {
            (None, false) => 0,
            (None, true) => n - 1,
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
        };
        v.set_focus(stops[next]);
        drop(v);
        self.invalidate();
    }

    /// Space/Enter on the focused target: what a click does, keeping the focus on the same thing.
    pub(super) fn press(&self, t: Target) {
        match t {
            Target::Up(i) | Target::Down(i) => {
                let up = matches!(t, Target::Up(_));
                let Some(m) = self.order_module(i) else { return };
                self.edit(Edit::Move(m, up));
                // The module moved: follow it, to the other button once it reached an end.
                let j = if up { i - 1 } else { i + 1 };
                let mut v = self.view.borrow_mut();
                let target = if up { Target::Up(j) } else { Target::Down(j) };
                let fallback = if up { Target::Down(j) } else { Target::Up(j) };
                let stops = v.stops();
                v.set_focus(if stops.contains(&target) { target } else { fallback });
                drop(v);
                self.invalidate();
            }
            _ => self.activate(t),
        }
    }

    fn arrow(&self, vk: VIRTUAL_KEY, alt: bool) {
        let Some(t) = self.focused() else { return self.tab(false) };
        let delta: isize = if matches!(vk, VK_UP | VK_LEFT) { -1 } else { 1 };
        match t {
            Target::Nav(i) if matches!(vk, VK_UP | VK_DOWN) => {
                let mut v = self.view.borrow_mut();
                let j = (i as isize + delta).clamp(0, v.nav.len() as isize - 1) as usize;
                v.set_focus(Target::Nav(j));
                drop(v);
                self.invalidate();
            }
            Target::Ctl(i) => {
                let edit = match self.view.borrow().items.get(i) {
                    Some(Item::Row(r)) => match &r.control {
                        // Alt+↓ (or ↓ alone, as the design has no other use for it) opens a dropdown.
                        Control::Dropdown(..) if vk == VK_DOWN || alt => None,
                        Control::Segmented(opts, sel) if matches!(vk, VK_LEFT | VK_RIGHT) => {
                            let k = (*sel as isize + delta).clamp(0, opts.len() as isize - 1) as usize;
                            (k != *sel).then(|| Edit::Pick(opts[k].pick.clone()))
                        }
                        Control::Swatches(m, sel) if matches!(vk, VK_LEFT | VK_RIGHT) => {
                            let k = (*sel as isize + delta).clamp(0, busy_core::PALETTE_LEN as isize - 1) as u8;
                            (k != *sel).then_some(Edit::Color(*m, k))
                        }
                        _ => return,
                    },
                    _ => return,
                };
                match edit {
                    Some(e) => self.edit(e),
                    None if vk == VK_DOWN => self.control(i),
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// Editing keys in the search box; the rest (Tab, Esc on an empty box) work as anywhere.
    fn search_key(&self, vk: VIRTUAL_KEY) -> bool {
        let mut v = self.view.borrow_mut();
        match vk {
            VK_BACK if !v.query.is_empty() => {
                v.query.pop();
            }
            VK_ESCAPE if !v.query.is_empty() => v.query.clear(),
            // Enter or ↓ go to the first result.
            VK_RETURN | VK_DOWN => {
                if let Some(&first) = v.stops().iter().find(|s| matches!(s, Target::Ctl(_))).filter(|_| v.searching()) {
                    v.set_focus(first);
                }
                drop(v);
                self.invalidate();
                return true;
            }
            _ => return false,
        }
        v.scroll = 0.0;
        drop(v);
        self.rebuild();
        true
    }

    /// `WM_CHAR`: typing goes to the search box while it has the focus.
    pub(super) fn on_char(&self, c: u16) -> bool {
        let mut v = self.view.borrow_mut();
        if self.in_setup() || v.focus != Some(Target::Search) {
            return false;
        }
        // Printable characters only (no control characters, no halves of a surrogate pair), and a sane length.
        let Some(ch) = char::from_u32(c as u32).filter(|ch| !ch.is_control()) else { return true };
        if v.query.chars().count() < 64 {
            v.query.push(ch);
            v.scroll = 0.0;
        }
        drop(v);
        self.rebuild();
        true
    }

    fn popup_key(&self, vk: VIRTUAL_KEY) -> bool {
        let mut v = self.view.borrow_mut();
        if matches!(vk, VK_ESCAPE | VK_TAB | VK_F4) {
            v.popup = None;
            drop(v);
            self.invalidate();
            return true;
        }
        let Some(n) = v.options().map(|(o, _)| o.len()) else { return false };
        match vk {
            VK_UP | VK_DOWN | VK_HOME | VK_END => {
                let Some(p) = v.popup.as_mut() else { return false };
                p.cursor = match vk {
                    VK_UP => p.cursor.saturating_sub(1),
                    VK_DOWN => (p.cursor + 1).min(n - 1),
                    VK_HOME => 0,
                    _ => n - 1,
                };
                // Keep the cursor among the options shown.
                if p.cursor < p.first {
                    p.first = p.cursor;
                } else if p.cursor >= p.first + p.visible {
                    p.first = p.cursor + 1 - p.visible;
                }
                v.hover = Some(Target::Opt(p.cursor));
                drop(v);
                self.invalidate();
            }
            VK_SPACE | VK_RETURN => {
                let k = v.popup.as_ref().map_or(0, |p| p.cursor);
                drop(v);
                self.activate(Target::Opt(k));
            }
            _ => return false,
        }
        true
    }
}
