//! `WM_COMMAND`/`WM_NOTIFY` handling and the module list editors.

use super::Ui;
use super::config::STYLES;
use super::controls::*;
use busy_core::{Module, ModuleCfg};
use windows::Win32::Foundation::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, IsWindowEnabled, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PWSTR};

impl Ui {
    pub(super) fn command(&self, id: u16, code: u32) {
        match (id, code) {
            (ID_OK, _) => {
                self.apply();
                let _ = unsafe { DestroyWindow(self.hwnd) };
            }
            (ID_CANCEL, _) => {
                let _ = unsafe { DestroyWindow(self.hwnd) };
            }
            (ID_APPLY, BN_CLICKED) => self.apply(),
            (ID_UP, BN_CLICKED) => self.move_row(-1),
            (ID_DOWN, BN_CLICKED) => self.move_row(1),
            (ID_TASKBAR, BN_CLICKED) => {
                let on = checked(self.ctl.taskbar);
                self.edit_row(|m| m.taskbar = on);
            }
            (ID_FLYOUT, BN_CLICKED) => {
                let on = checked(self.ctl.flyout);
                self.edit_row(|m| m.flyout = on);
            }
            (ID_STYLE, CBN_SELCHANGE) => {
                if let Some((st, _)) = combo_value(self.ctl.style).and_then(|v| STYLES.get(v as usize)) {
                    self.edit_row(|m| m.style = *st);
                }
            }
            (ID_ANCHOR | ID_INTERVAL | ID_HISTORY | ID_THEME | ID_UNIT, CBN_SELCHANGE) | (ID_AUTOSTART, BN_CLICKED) => {
                self.changed();
            }
            (ID_OFFSET | ID_SENSOR, EN_CHANGE) if !self.syncing.get() => self.changed(),
            _ => {}
        }
    }

    pub(super) fn notify(&self, l: LPARAM) -> Option<LRESULT> {
        let hdr = unsafe { &*(l.0 as *const NMHDR) };
        match hdr.code {
            LVN_ITEMCHANGED if hdr.hwndFrom == self.ctl.list => {
                let nm = unsafe { &*(l.0 as *const NMLISTVIEW) };
                if (nm.uNewState ^ nm.uOldState) & LVIS_SELECTED.0 != 0 {
                    self.sync_editors();
                }
                None
            }
            NM_CUSTOMDRAW
                if self.dark.get()
                    && [self.ctl.taskbar, self.ctl.flyout, self.ctl.autostart].contains(&hdr.hwndFrom) =>
            {
                Some(self.draw_check(unsafe { &*(l.0 as *const NMCUSTOMDRAW) }))
            }
            _ => None,
        }
    }

    pub(super) fn enable(&self, h: HWND, on: bool) {
        unsafe {
            if !on && GetFocus() == h {
                let next = match h {
                    _ if h == self.ctl.apply => self.ctl.ok,
                    _ if h == self.ctl.up => self.ctl.down,
                    _ if h == self.ctl.down => self.ctl.up,
                    _ => self.ctl.list,
                };
                let _ = SetFocus(Some(if IsWindowEnabled(next).as_bool() { next } else { self.ctl.list }));
            }
            let _ = EnableWindow(h, on);
        }
    }

    fn selected(&self) -> Option<usize> {
        let i = send(self.ctl.list, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize);
        (i >= 0).then_some(i as usize)
    }

    pub(super) fn select(&self, i: usize) {
        let st = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
        let item = LVITEMW { state: st, stateMask: st, ..Default::default() };
        send(self.ctl.list, LVM_SETITEMSTATE, i, &item as *const _ as isize);
        send(self.ctl.list, LVM_ENSUREVISIBLE, i, 0);
    }

    pub(super) fn fill_row(&self, i: usize) {
        let Some(m) = self.modules.borrow().get(i).cloned() else { return };
        let flyout_only = m.module == Module::Processes;
        let mark = |on: bool| if on { "\u{2713}" } else { "" };
        let style = STYLES.iter().find(|(s, _)| *s == m.style).map_or("", |(_, n)| n);
        let cells = [
            m.module.label(),
            if flyout_only { "\u{2014}" } else { mark(m.taskbar) },
            mark(m.flyout),
            if flyout_only { "\u{2014}" } else { style },
        ];
        for (sub, text) in cells.into_iter().enumerate() {
            let text = HSTRING::from(text);
            let item = LVITEMW { iSubItem: sub as i32, pszText: PWSTR(text.as_ptr() as _), ..Default::default() };
            send(self.ctl.list, LVM_SETITEMTEXTW, i, &item as *const _ as isize);
        }
    }

    pub(super) fn sync_editors(&self) {
        let sel = self.selected();
        let len = self.modules.borrow().len();
        let m = sel.and_then(|i| self.modules.borrow().get(i).cloned());
        let flyout_only = m.as_ref().is_some_and(|m| m.module == Module::Processes);
        let editable = m.as_ref().filter(|_| !flyout_only);
        set_check(self.ctl.taskbar, editable.is_some_and(|m| m.taskbar));
        set_check(self.ctl.flyout, m.as_ref().is_some_and(|m| m.flyout));
        match editable {
            Some(m) => {
                combo_set(
                    self.ctl.style,
                    STYLES.iter().position(|(s, _)| *s == m.style).unwrap_or(0) as isize,
                    String::new,
                );
            }
            None => {
                send(self.ctl.style, CB_SETCURSEL, usize::MAX, 0);
            }
        }
        self.enable(self.ctl.taskbar, editable.is_some());
        self.enable(self.ctl.flyout, m.is_some());
        self.enable(self.ctl.style, editable.is_some());
        self.enable(self.ctl.up, sel.is_some_and(|i| i > 0));
        self.enable(self.ctl.down, sel.is_some_and(|i| i + 1 < len));
    }

    fn edit_row(&self, f: impl FnOnce(&mut ModuleCfg)) {
        let Some(i) = self.selected() else { return };
        if let Some(m) = self.modules.borrow_mut().get_mut(i) {
            f(m);
        }
        self.fill_row(i);
        self.changed();
    }

    fn move_row(&self, delta: isize) {
        let Some(i) = self.selected() else { return };
        let j = i.wrapping_add_signed(delta);
        if j >= self.modules.borrow().len() {
            return;
        }
        self.modules.borrow_mut().swap(i, j);
        self.fill_row(i);
        self.fill_row(j);
        self.select(j);
        self.sync_editors();
        self.changed();
    }
}
