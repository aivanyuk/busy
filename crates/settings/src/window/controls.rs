//! Child controls: IDs, creation, and the message helpers that read and write them.

use super::paint::{group_proc, list_proc};
use windows::Win32::Foundation::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Shell::SetWindowSubclass;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR};

const SS_CENTERIMAGE: u32 = 0x200;

pub(super) const ID_OK: u16 = 1;
pub(super) const ID_CANCEL: u16 = 2;
pub(super) const ID_APPLY: u16 = 100;
const ID_LIST: u16 = 101;
pub(super) const ID_UP: u16 = 102;
pub(super) const ID_DOWN: u16 = 103;
pub(super) const ID_TASKBAR: u16 = 104;
pub(super) const ID_FLYOUT: u16 = 105;
pub(super) const ID_STYLE: u16 = 106;
pub(super) const ID_ANCHOR: u16 = 107;
pub(super) const ID_OFFSET: u16 = 108;
const ID_SPIN: u16 = 109;
pub(super) const ID_INTERVAL: u16 = 110;
pub(super) const ID_HISTORY: u16 = 111;
pub(super) const ID_THEME: u16 = 112;
pub(super) const ID_UNIT: u16 = 113;
pub(super) const ID_SENSOR: u16 = 114;
pub(super) const ID_AUTOSTART: u16 = 115;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Static,
    Group,
    Push,
    Check,
    Combo,
    Edit,
    List,
    Spin,
}

/// Child control handles; creation order is tab order.
pub(super) struct Controls {
    pub(super) all: Vec<(HWND, Kind)>,
    pub(super) groups: [HWND; 3],
    pub(super) list: HWND,
    pub(super) up: HWND,
    pub(super) down: HWND,
    pub(super) taskbar: HWND,
    pub(super) flyout: HWND,
    pub(super) style_lbl: HWND,
    pub(super) style: HWND,
    pub(super) anchor_lbl: HWND,
    pub(super) anchor: HWND,
    pub(super) offset_lbl: HWND,
    pub(super) offset: HWND,
    pub(super) spin: HWND,
    pub(super) px_lbl: HWND,
    pub(super) interval_lbl: HWND,
    pub(super) interval: HWND,
    pub(super) history_lbl: HWND,
    pub(super) history: HWND,
    pub(super) theme_lbl: HWND,
    pub(super) theme: HWND,
    pub(super) unit_lbl: HWND,
    pub(super) unit: HWND,
    pub(super) sensor_lbl: HWND,
    pub(super) sensor: HWND,
    pub(super) autostart: HWND,
    pub(super) ok: HWND,
    pub(super) cancel: HWND,
    pub(super) apply: HWND,
}

impl Controls {
    pub(super) fn create(parent: HWND, hinst: HINSTANCE) -> Self {
        unsafe {
            let mut ctls = Vec::new();
            let mut mk = |class: PCWSTR, text: &str, style: u32, id: u16, kind: Kind| {
                let ex = if matches!(kind, Kind::List | Kind::Edit) { WS_EX_CLIENTEDGE } else { WINDOW_EX_STYLE(0) };
                let h = CreateWindowExW(
                    ex,
                    class,
                    &HSTRING::from(text),
                    WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
                    0,
                    0,
                    0,
                    0,
                    Some(parent),
                    Some(HMENU(id as usize as _)),
                    Some(hinst),
                    None,
                )
                .unwrap_or_default();
                ctls.push((h, kind));
                h
            };
            let tab = (WS_TABSTOP | WS_GROUP).0;
            let group = BS_GROUPBOX as u32;
            let push = tab | BS_PUSHBUTTON as u32;
            let check = tab | BS_AUTOCHECKBOX as u32;
            let combo = tab | WS_VSCROLL.0 | CBS_DROPDOWNLIST as u32;
            let edit = tab | ES_AUTOHSCROLL as u32;

            // Creation order = tab order; labels precede their control so mnemonics work.
            let grp_mod = mk(WC_BUTTONW, "&Modules", group, 0, Kind::Group);
            let lv_style = LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | LVS_NOSORTHEADER;
            let list = mk(WC_LISTVIEWW, "Modules", tab | lv_style, ID_LIST, Kind::List);
            let up = mk(WC_BUTTONW, "Move &up", push, ID_UP, Kind::Push);
            let down = mk(WC_BUTTONW, "Move &down", push, ID_DOWN, Kind::Push);
            let taskbar = mk(WC_BUTTONW, "Show on &taskbar", check, ID_TASKBAR, Kind::Check);
            let flyout = mk(WC_BUTTONW, "Show in &flyout", check, ID_FLYOUT, Kind::Check);
            let style_lbl = mk(WC_STATICW, "St&yle:", SS_CENTERIMAGE, 0, Kind::Static);
            let style = mk(WC_COMBOBOXW, "", combo, ID_STYLE, Kind::Combo);

            let grp_tb = mk(WC_BUTTONW, "Taskbar", group, 0, Kind::Group);
            let anchor_lbl = mk(WC_STATICW, "&Position:", SS_CENTERIMAGE, 0, Kind::Static);
            let anchor = mk(WC_COMBOBOXW, "", combo, ID_ANCHOR, Kind::Combo);
            let offset_lbl = mk(WC_STATICW, "Off&set:", SS_CENTERIMAGE, 0, Kind::Static);
            let offset = mk(WC_EDITW, "", edit, ID_OFFSET, Kind::Edit);
            let spin_style = UDS_SETBUDDYINT | UDS_ARROWKEYS | UDS_NOTHOUSANDS;
            let spin = mk(UPDOWN_CLASSW, "", spin_style, ID_SPIN, Kind::Spin);
            let px_lbl = mk(WC_STATICW, "px", SS_CENTERIMAGE, 0, Kind::Static);

            let grp_gen = mk(WC_BUTTONW, "General", group, 0, Kind::Group);
            let interval_lbl = mk(WC_STATICW, "Update &interval:", SS_CENTERIMAGE, 0, Kind::Static);
            let interval = mk(WC_COMBOBOXW, "", combo, ID_INTERVAL, Kind::Combo);
            let history_lbl = mk(WC_STATICW, "&History length:", SS_CENTERIMAGE, 0, Kind::Static);
            let history = mk(WC_COMBOBOXW, "", combo, ID_HISTORY, Kind::Combo);
            let theme_lbl = mk(WC_STATICW, "Th&eme:", SS_CENTERIMAGE, 0, Kind::Static);
            let theme = mk(WC_COMBOBOXW, "", combo, ID_THEME, Kind::Combo);
            let unit_lbl = mk(WC_STATICW, "Temperature u&nit:", SS_CENTERIMAGE, 0, Kind::Static);
            let unit = mk(WC_COMBOBOXW, "", combo, ID_UNIT, Kind::Combo);
            let sensor_lbl = mk(WC_STATICW, "Pinned sens&or:", SS_CENTERIMAGE, 0, Kind::Static);
            let sensor = mk(WC_EDITW, "", edit, ID_SENSOR, Kind::Edit);
            let autostart = mk(WC_BUTTONW, "Start with &Windows", check, ID_AUTOSTART, Kind::Check);

            let ok = mk(WC_BUTTONW, "OK", tab | BS_DEFPUSHBUTTON as u32, ID_OK, Kind::Push);
            let cancel = mk(WC_BUTTONW, "Cancel", push, ID_CANCEL, Kind::Push);
            let apply = mk(WC_BUTTONW, "&Apply", push, ID_APPLY, Kind::Push);

            for g in [grp_mod, grp_tb, grp_gen] {
                let _ = SetWindowSubclass(g, Some(group_proc), 1, 0);
            }
            let _ = SetWindowSubclass(list, Some(list_proc), 1, 0);

            Self {
                all: ctls,
                groups: [grp_mod, grp_tb, grp_gen],
                list,
                up,
                down,
                taskbar,
                flyout,
                style_lbl,
                style,
                anchor_lbl,
                anchor,
                offset_lbl,
                offset,
                spin,
                px_lbl,
                interval_lbl,
                interval,
                history_lbl,
                history,
                theme_lbl,
                theme,
                unit_lbl,
                unit,
                sensor_lbl,
                sensor,
                autostart,
                ok,
                cancel,
                apply,
            }
        }
    }
}

pub(super) fn send(h: HWND, msg: u32, w: usize, l: isize) -> isize {
    unsafe { SendMessageW(h, msg, Some(WPARAM(w)), Some(LPARAM(l))).0 }
}

pub(super) fn window_text(h: HWND) -> Vec<u16> {
    unsafe {
        let mut buf = vec![0u16; GetWindowTextLengthW(h) as usize + 1];
        let n = GetWindowTextW(h, &mut buf);
        buf.truncate(n.max(0) as usize);
        buf
    }
}

pub(super) fn window_string(h: HWND) -> String {
    String::from_utf16_lossy(&window_text(h))
}

pub(super) fn set_text(h: HWND, s: &str) {
    let _ = unsafe { SetWindowTextW(h, &HSTRING::from(s)) };
}

pub(super) fn checked(h: HWND) -> bool {
    send(h, BM_GETCHECK, 0, 0) == BST_CHECKED.0 as isize
}

pub(super) fn set_check(h: HWND, on: bool) {
    send(h, BM_SETCHECK, if on { BST_CHECKED } else { BST_UNCHECKED }.0 as usize, 0);
}

pub(super) fn combo_fill<'a>(h: HWND, items: impl IntoIterator<Item = (&'a str, isize)>) {
    for (text, v) in items {
        let text = HSTRING::from(text);
        let i = send(h, CB_ADDSTRING, 0, text.as_ptr() as isize);
        send(h, CB_SETITEMDATA, i as usize, v);
    }
}

/// Selects the item whose data is `v`, inserting it (sorted by value) with `label()` if missing.
pub(super) fn combo_set(h: HWND, v: isize, label: impl FnOnce() -> String) {
    let n = send(h, CB_GETCOUNT, 0, 0).max(0) as usize;
    let data = |i: usize| send(h, CB_GETITEMDATA, i, 0);
    let i = (0..n).find(|&i| data(i) == v).unwrap_or_else(|| {
        let at = (0..n).find(|&i| data(i) > v).unwrap_or(usize::MAX);
        let text = HSTRING::from(label());
        let i = send(h, CB_INSERTSTRING, at, text.as_ptr() as isize) as usize;
        send(h, CB_SETITEMDATA, i, v);
        i
    });
    send(h, CB_SETCURSEL, i, 0);
}

pub(super) fn combo_value(h: HWND) -> Option<isize> {
    let i = send(h, CB_GETCURSEL, 0, 0);
    (i >= 0).then(|| send(h, CB_GETITEMDATA, i as usize, 0))
}
