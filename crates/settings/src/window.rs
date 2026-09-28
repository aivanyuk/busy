use crate::autostart;
use busy_core::{Anchor, CellStyle, Config, Module, ModuleCfg, TempUnit, ThemeMode};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, SystemParametersInfoForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, IsWindowEnabled, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, PWSTR, Result, w};

const CLASS: PCWSTR = w!("busy.settings");
const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0);
const SS_CENTERIMAGE: u32 = 0x200;

const ID_OK: u16 = 1;
const ID_CANCEL: u16 = 2;
const ID_APPLY: u16 = 100;
const ID_LIST: u16 = 101;
const ID_UP: u16 = 102;
const ID_DOWN: u16 = 103;
const ID_TASKBAR: u16 = 104;
const ID_FLYOUT: u16 = 105;
const ID_STYLE: u16 = 106;
const ID_ANCHOR: u16 = 107;
const ID_OFFSET: u16 = 108;
const ID_SPIN: u16 = 109;
const ID_INTERVAL: u16 = 110;
const ID_HISTORY: u16 = 111;
const ID_THEME: u16 = 112;
const ID_UNIT: u16 = 113;
const ID_SENSOR: u16 = 114;
const ID_AUTOSTART: u16 = 115;

const OFFSET_RANGE: i32 = 2000;
const STYLES: [(CellStyle, &str); 3] =
    [(CellStyle::Text, "Text"), (CellStyle::Graph, "Graph"), (CellStyle::Bar, "Bar")];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Static,
    Group,
    Push,
    Check,
    Combo,
    Edit,
    List,
    Spin,
}

thread_local! {
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
}

fn ui() -> Option<Rc<Ui>> {
    UI.with_borrow(Clone::clone)
}

pub fn hwnd() -> Option<HWND> {
    UI.with_borrow(|u| u.as_ref().map(|u| u.hwnd))
}

pub fn open(cfg: &Config, on_apply: Box<dyn Fn(Config)>) {
    if let Some(u) = ui() {
        unsafe {
            if IsIconic(u.hwnd).as_bool() {
                let _ = ShowWindow(u.hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(u.hwnd);
        }
        return;
    }
    let _ = create(cfg, on_apply);
}

struct Ui {
    hwnd: HWND,
    ctls: Vec<(HWND, Kind)>,
    groups: [HWND; 3],
    list: HWND,
    up: HWND,
    down: HWND,
    taskbar: HWND,
    flyout: HWND,
    style_lbl: HWND,
    style: HWND,
    anchor_lbl: HWND,
    anchor: HWND,
    offset_lbl: HWND,
    offset: HWND,
    spin: HWND,
    px_lbl: HWND,
    interval_lbl: HWND,
    interval: HWND,
    history_lbl: HWND,
    history: HWND,
    theme_lbl: HWND,
    theme: HWND,
    unit_lbl: HWND,
    unit: HWND,
    sensor_lbl: HWND,
    sensor: HWND,
    autostart: HWND,
    ok: HWND,
    cancel: HWND,
    apply: HWND,
    modules: RefCell<Vec<ModuleCfg>>,
    applied: RefCell<Config>,
    on_apply: Box<dyn Fn(Config)>,
    dpi: Cell<u32>,
    font: Cell<HFONT>,
    syncing: Cell<bool>,
    focus: Cell<HWND>,
}

impl Drop for Ui {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font.get().into());
        }
    }
}

fn create(cfg: &Config, on_apply: Box<dyn Fn(Config)>) -> Result<()> {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let icc = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_LISTVIEW_CLASSES | ICC_UPDOWN_CLASS,
        };
        let _ = InitCommonControlsEx(&icc);
        let icon = LoadImageW(Some(hinst), PCWSTR(1 as _), IMAGE_ICON, 0, 0, LR_DEFAULTSIZE | LR_SHARED)
            .map(|h| HICON(h.0))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default();
        RegisterClassExW(&WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hIcon: icon,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        });
        // Created on the primary monitor so GetDpiForWindow matches where it will be centered.
        let work = work_area();
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            &HSTRING::from("busy \u{2014} Settings"),
            STYLE,
            work.left,
            work.top,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )?;

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
                Some(hwnd),
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

        let ui = Rc::new(Ui {
            hwnd,
            ctls,
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
            modules: RefCell::new(cfg.modules.clone()),
            applied: RefCell::new(cfg.clone()),
            on_apply,
            dpi: Cell::new(GetDpiForWindow(hwnd).max(96)),
            font: Cell::new(HFONT::default()),
            syncing: Cell::new(false),
            focus: Cell::new(list),
        });
        ui.set_font();
        ui.init(cfg);
        ui.apply_theme();
        let (cw, ch) = ui.layout();
        let (ww, wh) = ui.frame_size(cw, ch);
        let work = work_area();
        let x = work.left + (work.right - work.left - ww) / 2;
        let y = work.top + (work.bottom - work.top - wh) / 2;
        let _ = SetWindowPos(hwnd, None, x, y, ww, wh, SWP_NOZORDER | SWP_NOACTIVATE);
        UI.set(Some(ui));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        Ok(())
    }
}

impl Ui {
    fn init(&self, cfg: &Config) {
        send(self.list, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize);
        for (i, name) in ["Module", "Taskbar", "Flyout", "Style"].into_iter().enumerate() {
            let text = HSTRING::from(name);
            let col = LVCOLUMNW {
                mask: LVCF_TEXT | LVCF_FMT,
                fmt: if i == 1 || i == 2 { LVCFMT_CENTER } else { LVCFMT_LEFT },
                pszText: PWSTR(text.as_ptr() as _),
                ..Default::default()
            };
            send(self.list, LVM_INSERTCOLUMNW, i, &col as *const _ as isize);
        }
        for i in 0..cfg.modules.len() {
            let item = LVITEMW {
                mask: LVIF_TEXT,
                iItem: i as i32,
                pszText: PWSTR(w!("").as_ptr() as _),
                ..Default::default()
            };
            send(self.list, LVM_INSERTITEMW, 0, &item as *const _ as isize);
            self.fill_row(i);
        }

        combo_fill(self.style, STYLES.iter().enumerate().map(|(i, (_, s))| (*s, i as isize)));
        combo_fill(self.anchor, [("Near tray", 0), ("Left edge", 1)]);
        combo_set(self.anchor, (cfg.anchor == Anchor::Left) as isize, String::new);
        combo_fill(
            self.interval,
            [500, 1000, 2000, 5000].map(|v| (fmt_ms(v), v as isize)).iter().map(|(s, v)| (s.as_str(), *v)),
        );
        combo_set(self.interval, cfg.interval_ms as isize, || fmt_ms(cfg.interval_ms));
        combo_fill(
            self.history,
            [60, 120, 300, 600].map(|v| (fmt_secs(v), v as isize)).iter().map(|(s, v)| (s.as_str(), *v)),
        );
        combo_set(self.history, cfg.history_secs as isize, || fmt_secs(cfg.history_secs));
        combo_fill(self.theme, [("System", 0), ("Light", 1), ("Dark", 2)]);
        combo_set(self.theme, cfg.theme as isize, String::new);
        combo_fill(self.unit, [("Celsius (\u{b0}C)", 0), ("Fahrenheit (\u{b0}F)", 1)]);
        combo_set(self.unit, (cfg.temp_unit == TempUnit::Fahrenheit) as isize, String::new);

        send(self.spin, UDM_SETBUDDY, self.offset.0 as usize, 0);
        send(self.spin, UDM_SETRANGE32, -OFFSET_RANGE as usize, OFFSET_RANGE as isize);
        send(self.spin, UDM_SETPOS32, 0, cfg.offset_px.clamp(-OFFSET_RANGE, OFFSET_RANGE) as isize);
        set_text(self.offset, &cfg.offset_px.to_string());
        set_text(self.sensor, &cfg.pinned_sensor);
        let cue = HSTRING::from("hardware/name \u{2014} empty = hottest CPU");
        send(self.sensor, EM_SETCUEBANNER, 0, cue.as_ptr() as isize);
        set_check(self.autostart, autostart::is_enabled());

        self.select(0);
        self.sync_editors();
        self.changed();
    }

    fn set_font(&self) {
        unsafe {
            let mut ncm = NONCLIENTMETRICSW { cbSize: size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
            let pv = Some((&mut ncm as *mut NONCLIENTMETRICSW).cast());
            let _ = SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS.0, ncm.cbSize, pv, 0, self.dpi.get());
            let font = CreateFontIndirectW(&ncm.lfMessageFont);
            for &(h, _) in &self.ctls {
                send(h, WM_SETFONT, font.0 as usize, 1);
            }
            let old = self.font.replace(font);
            if !old.is_invalid() {
                let _ = DeleteObject(old.into());
            }
        }
    }

    /// Positions all controls for the current DPI; returns the client size in pixels.
    fn layout(&self) -> (i32, i32) {
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
            send(self.list, LVM_SETCOLUMNWIDTH, i, cx as isize);
        }
        let mut y = m;
        let ly = y + top;
        // Size the list to fit all rows exactly: lay out tall, then measure the last row.
        mv(self.list, ix, ly, lw, s(1000));
        let mut last = RECT::default();
        send(self.list, LVM_GETITEMRECT, self.modules.borrow().len().saturating_sub(1), &mut last as *mut _ as isize);
        let (mut wr, mut cr) = (RECT::default(), RECT::default());
        unsafe {
            let _ = GetWindowRect(self.list, &mut wr);
            let _ = GetClientRect(self.list, &mut cr);
        }
        let lh = last.bottom + (wr.bottom - wr.top) - (cr.bottom - cr.top);
        mv(self.list, ix, ly, lw, lh);
        mv(self.up, ix + iw - bw, ly, bw, h + s(2));
        mv(self.down, ix + iw - bw, ly + h + s(8), bw, h + s(2));
        let ey = ly + lh + gap;
        mv(self.taskbar, ix, ey, s(120), h);
        mv(self.flyout, ix + s(130), ey, s(115), h);
        mv(self.style, ix + iw - bw, ey, bw, s(200));
        mv(self.style_lbl, ix + iw - bw - s(48), ey, s(44), h);
        let gh = ey + h + pad - y;
        mv(self.groups[0], gx, y, gw, gh);
        y += gh + s(10);

        // Two-column label/control grid shared by Taskbar and General.
        let (c1l, c1c, c2l, c2c, lwid, cwid) = (ix, ix + s(110), ix + s(242), ix + s(352), s(104), s(120));
        let ry = y + top;
        mv(self.anchor_lbl, c1l, ry, lwid, h);
        mv(self.anchor, c1c, ry, cwid, s(200));
        mv(self.offset_lbl, c2l, ry, lwid, h);
        let (ew, sw) = (s(64), s(18));
        mv(self.offset, c2c, ry, ew, h);
        mv(self.spin, c2c + ew, ry, sw, h);
        mv(self.px_lbl, c2c + ew + sw + s(6), ry, s(30), h);
        let gh = top + h + pad;
        mv(self.groups[1], gx, y, gw, gh);
        y += gh + s(10);

        let row = |i: i32| y + top + i * (h + gap);
        mv(self.interval_lbl, c1l, row(0), lwid, h);
        mv(self.interval, c1c, row(0), cwid, s(200));
        mv(self.history_lbl, c2l, row(0), lwid, h);
        mv(self.history, c2c, row(0), cwid, s(200));
        mv(self.theme_lbl, c1l, row(1), lwid, h);
        mv(self.theme, c1c, row(1), cwid, s(200));
        mv(self.unit_lbl, c2l, row(1), lwid, h);
        mv(self.unit, c2c, row(1), cwid, s(200));
        mv(self.sensor_lbl, c1l, row(2), lwid, h);
        mv(self.sensor, c1c, row(2), ix + iw - c1c, h);
        mv(self.autostart, c1l, row(3), s(200), h);
        let gh = row(3) + h + pad - y;
        mv(self.groups[2], gx, y, gw, gh);
        y += gh + s(12);

        let (bw, bh) = (s(80), h + s(2));
        for (i, b) in [self.ok, self.cancel, self.apply].into_iter().enumerate() {
            mv(b, width - m - (3 - i as i32) * bw - (2 - i as i32) * gap, y, bw, bh);
        }
        (width, y + bh + m)
    }

    fn frame_size(&self, cw: i32, ch: i32) -> (i32, i32) {
        let mut rc = RECT { left: 0, top: 0, right: cw, bottom: ch };
        let _ = unsafe { AdjustWindowRectExForDpi(&mut rc, STYLE, false, WINDOW_EX_STYLE(0), self.dpi.get()) };
        (rc.right - rc.left, rc.bottom - rc.top)
    }

    fn apply_theme(&self) {
        let _ = unsafe { SetWindowTheme(self.list, w!("Explorer"), PCWSTR::null()) };
    }

    fn bg_brush(&self) -> HBRUSH {
        unsafe { GetSysColorBrush(COLOR_BTNFACE) }
    }

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
                    if !field {
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

    fn command(&self, id: u16, code: u32) {
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
                let on = checked(self.taskbar);
                self.edit_row(|m| m.taskbar = on);
            }
            (ID_FLYOUT, BN_CLICKED) => {
                let on = checked(self.flyout);
                self.edit_row(|m| m.flyout = on);
            }
            (ID_STYLE, CBN_SELCHANGE) => {
                if let Some((st, _)) = combo_value(self.style).and_then(|v| STYLES.get(v as usize)) {
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

    fn notify(&self, l: LPARAM) -> Option<LRESULT> {
        let hdr = unsafe { &*(l.0 as *const NMHDR) };
        match hdr.code {
            LVN_ITEMCHANGED if hdr.hwndFrom == self.list => {
                let nm = unsafe { &*(l.0 as *const NMLISTVIEW) };
                if (nm.uNewState ^ nm.uOldState) & LVIS_SELECTED.0 != 0 {
                    self.sync_editors();
                }
                None
            }
            _ => None,
        }
    }

    fn enable(&self, h: HWND, on: bool) {
        unsafe {
            if !on && GetFocus() == h {
                let next = match h {
                    _ if h == self.apply => self.ok,
                    _ if h == self.up => self.down,
                    _ if h == self.down => self.up,
                    _ => self.list,
                };
                let _ = SetFocus(Some(if IsWindowEnabled(next).as_bool() { next } else { self.list }));
            }
            let _ = EnableWindow(h, on);
        }
    }

    fn selected(&self) -> Option<usize> {
        let i = send(self.list, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize);
        (i >= 0).then_some(i as usize)
    }

    fn select(&self, i: usize) {
        let st = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
        let item = LVITEMW { state: st, stateMask: st, ..Default::default() };
        send(self.list, LVM_SETITEMSTATE, i, &item as *const _ as isize);
        send(self.list, LVM_ENSUREVISIBLE, i, 0);
    }

    fn fill_row(&self, i: usize) {
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
            send(self.list, LVM_SETITEMTEXTW, i, &item as *const _ as isize);
        }
    }

    fn sync_editors(&self) {
        let sel = self.selected();
        let len = self.modules.borrow().len();
        let m = sel.and_then(|i| self.modules.borrow().get(i).cloned());
        let flyout_only = m.as_ref().is_some_and(|m| m.module == Module::Processes);
        let editable = m.as_ref().filter(|_| !flyout_only);
        set_check(self.taskbar, editable.is_some_and(|m| m.taskbar));
        set_check(self.flyout, m.as_ref().is_some_and(|m| m.flyout));
        match editable {
            Some(m) => {
                combo_set(
                    self.style,
                    STYLES.iter().position(|(s, _)| *s == m.style).unwrap_or(0) as isize,
                    String::new,
                );
            }
            None => {
                send(self.style, CB_SETCURSEL, usize::MAX, 0);
            }
        }
        self.enable(self.taskbar, editable.is_some());
        self.enable(self.flyout, m.is_some());
        self.enable(self.style, editable.is_some());
        self.enable(self.up, sel.is_some_and(|i| i > 0));
        self.enable(self.down, sel.is_some_and(|i| i + 1 < len));
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

    fn collect(&self) -> Config {
        let mut c = self.applied.borrow().clone();
        c.modules = self.modules.borrow().clone();
        c.anchor = if combo_value(self.anchor) == Some(1) { Anchor::Left } else { Anchor::NearTray };
        if let Ok(v) = window_string(self.offset).trim().parse::<i32>() {
            c.offset_px = v.clamp(-OFFSET_RANGE, OFFSET_RANGE);
        }
        c.interval_ms = combo_value(self.interval).map_or(c.interval_ms, |v| v as u32);
        c.history_secs = combo_value(self.history).map_or(c.history_secs, |v| v as u32);
        c.theme = match combo_value(self.theme) {
            Some(1) => ThemeMode::Light,
            Some(2) => ThemeMode::Dark,
            _ => ThemeMode::System,
        };
        c.temp_unit = if combo_value(self.unit) == Some(1) { TempUnit::Fahrenheit } else { TempUnit::Celsius };
        c.pinned_sensor = window_string(self.sensor).trim().to_string();
        c.autostart = checked(self.autostart);
        c
    }

    fn changed(&self) {
        let dirty = self.collect() != *self.applied.borrow();
        self.enable(self.apply, dirty);
    }

    fn apply(&self) {
        let mut c = self.collect();
        if c == *self.applied.borrow() {
            return;
        }
        if c.autostart != autostart::is_enabled()
            && let Err(e) = autostart::set(c.autostart)
        {
            let msg = HSTRING::from(format!("Couldn't update the startup entry.\n\n{}", e.message()));
            unsafe { MessageBoxW(Some(self.hwnd), &msg, w!("busy"), MB_ICONERROR | MB_OK) };
            c.autostart = autostart::is_enabled();
            set_check(self.autostart, c.autostart);
        }
        self.syncing.set(true);
        if window_string(self.offset).trim() != c.offset_px.to_string() {
            set_text(self.offset, &c.offset_px.to_string());
        }
        if window_string(self.sensor) != c.pinned_sensor {
            set_text(self.sensor, &c.pinned_sensor);
        }
        self.syncing.set(false);
        *self.applied.borrow_mut() = c.clone();
        self.changed();
        (self.on_apply)(c);
    }
}

extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if let Some(r) = ui().filter(|u| u.hwnd == h).and_then(|u| u.handle(m, w, l)) {
        return r;
    }
    unsafe { DefWindowProcW(h, m, w, l) }
}

fn work_area() -> RECT {
    unsafe {
        let mon = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        mi.rcWork
    }
}

fn send(h: HWND, msg: u32, w: usize, l: isize) -> isize {
    unsafe { SendMessageW(h, msg, Some(WPARAM(w)), Some(LPARAM(l))).0 }
}

fn window_text(h: HWND) -> Vec<u16> {
    unsafe {
        let mut buf = vec![0u16; GetWindowTextLengthW(h) as usize + 1];
        let n = GetWindowTextW(h, &mut buf);
        buf.truncate(n.max(0) as usize);
        buf
    }
}

fn window_string(h: HWND) -> String {
    String::from_utf16_lossy(&window_text(h))
}

fn set_text(h: HWND, s: &str) {
    let _ = unsafe { SetWindowTextW(h, &HSTRING::from(s)) };
}

fn checked(h: HWND) -> bool {
    send(h, BM_GETCHECK, 0, 0) == BST_CHECKED.0 as isize
}

fn set_check(h: HWND, on: bool) {
    send(h, BM_SETCHECK, if on { BST_CHECKED } else { BST_UNCHECKED }.0 as usize, 0);
}

fn combo_fill<'a>(h: HWND, items: impl IntoIterator<Item = (&'a str, isize)>) {
    for (text, v) in items {
        let text = HSTRING::from(text);
        let i = send(h, CB_ADDSTRING, 0, text.as_ptr() as isize);
        send(h, CB_SETITEMDATA, i as usize, v);
    }
}

/// Selects the item whose data is `v`, inserting it (sorted by value) with `label()` if missing.
fn combo_set(h: HWND, v: isize, label: impl FnOnce() -> String) {
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

fn combo_value(h: HWND) -> Option<isize> {
    let i = send(h, CB_GETCURSEL, 0, 0);
    (i >= 0).then(|| send(h, CB_GETITEMDATA, i as usize, 0))
}

fn fmt_ms(ms: u32) -> String {
    format!("{} s", ms as f64 / 1000.0)
}

fn fmt_secs(secs: u32) -> String {
    if secs.is_multiple_of(60) { format!("{} min", secs / 60) } else { format!("{secs} s") }
}
