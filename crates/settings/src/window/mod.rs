//! The settings window: its state (`Ui`), creation and the open/focus entry point.
//! Split by concern into control creation, layout, painting, command handling, config and the window procedure.

use crate::dark;
use busy_core::{Config, ModuleCfg};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, Result, w};

mod commands;
mod config;
mod controls;
mod layout;
mod paint;
mod wndproc;

use controls::Controls;
use layout::work_area;
use wndproc::wndproc;

const CLASS: PCWSTR = w!("busy.settings");
const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0);

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
    ctl: Controls,
    modules: RefCell<Vec<ModuleCfg>>,
    applied: RefCell<Config>,
    on_apply: Box<dyn Fn(Config)>,
    dpi: Cell<u32>,
    font: Cell<HFONT>,
    dark: Cell<bool>,
    bg: HBRUSH,
    field: HBRUSH,
    syncing: Cell<bool>,
    focus: Cell<HWND>,
}

impl Drop for Ui {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font.get().into());
            let _ = DeleteObject(self.bg.into());
            let _ = DeleteObject(self.field.into());
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

        let ctl = Controls::create(hwnd, hinst);
        let list = ctl.list;
        let ui = Rc::new(Ui {
            hwnd,
            ctl,
            modules: RefCell::new(cfg.modules.clone()),
            applied: RefCell::new(cfg.clone()),
            on_apply,
            dpi: Cell::new(GetDpiForWindow(hwnd).max(96)),
            font: Cell::new(HFONT::default()),
            dark: Cell::new(dark::wanted(cfg.theme)),
            bg: CreateSolidBrush(dark::BG),
            field: CreateSolidBrush(dark::FIELD),
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
