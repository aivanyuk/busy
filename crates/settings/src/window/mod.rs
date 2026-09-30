//! The settings window, custom-drawn with Direct2D to the design (`design/Meterbar.dc.html`, Settings): its
//! state (`Ui`), creation, the open/focus entry point and applying changes. Split by concern: the page model
//! (`model`, `choices`, `edit`), `layout`, `paint`, one file per control kind, `input`, the title bar (`frame`),
//! the window procedure and the registry worker.

use crate::{Host, dark};
use busy_core::{Config, Module};
use busy_ui::render::{Canvas, Gfx};
use busy_ui::theme::Theme;
use choices::Choices;
use layout::{Fonts, View, WIN_H, WIN_W};
use model::{Edit, Flag, Page};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, Result, w};
use worker::{Job, Reply, Worker};

mod choices;
mod clock;
mod controls;
#[cfg(debug_assertions)]
mod dump;
mod edit;
mod frame;
mod input;
mod layout;
mod live;
mod model;
mod paint;
mod wndproc;
mod worker;

const CLASS: PCWSTR = w!("busy.settings");

thread_local! {
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
}

fn ui() -> Option<Rc<Ui>> {
    UI.with_borrow(Clone::clone)
}

pub fn hwnd() -> Option<HWND> {
    UI.with_borrow(|u| u.as_ref().map(|u| u.hwnd))
}

pub(crate) use live::{refresh, sync};

pub fn open(cfg: &Config, host: Rc<dyn Host>, page: Option<Module>) {
    if let Some(u) = ui() {
        // SAFETY: `u.hwnd` is our live window.
        unsafe {
            if IsIconic(u.hwnd).as_bool() {
                let _ = ShowWindow(u.hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(u.hwnd);
        }
    } else {
        let _ = create(cfg, host);
    }
    if let (Some(u), Some(m)) = (ui(), page) {
        u.show_page(Page::Module(m));
    }
}

struct Ui {
    hwnd: HWND,
    gfx: Gfx,
    fonts: Fonts,
    rt: RefCell<Option<ID2D1HwndRenderTarget>>,
    dpi: Cell<u32>,
    /// The config as last applied; every edit starts from it.
    cfg: RefCell<Config>,
    host: Rc<dyn Host>,
    view: RefCell<View>,
    /// The machine's drives, adapters and sensors, as last read in `refresh`.
    choices: RefCell<Choices>,
    /// The preview's cell fonts and what it last showed (redrawn only when that changes).
    cell_fonts: busy_ui::cell::Fonts,
    shown: RefCell<Option<live::Shown>>,
    worker: Worker,
    /// Windows app mode, as last read by the worker.
    system_dark: Cell<bool>,
    /// Autostart state in the registry; `None` until the worker's first read.
    reg_autostart: Cell<Option<bool>>,
    /// An autostart write in flight, and whether to close once it lands.
    pending: Cell<Option<bool>>,
    /// The target the left button went down on; a click is a release on the same target.
    pressed: Cell<Option<layout::Target>>,
    /// `TrackMouseEvent` is armed for `WM_MOUSELEAVE`.
    tracking: Cell<bool>,
}

fn work_area() -> RECT {
    // SAFETY: `mi` is a valid out-pointer with its size set.
    unsafe {
        let mon = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        mi.rcWork
    }
}

fn create(cfg: &Config, host: Rc<dyn Host>) -> Result<()> {
    let gfx = Gfx::new()?;
    let fonts = Fonts::new(&gfx)?;
    let cell_fonts = busy_ui::cell::Fonts::new(&gfx)?;
    // SAFETY: plain Win32 calls with valid arguments; the class name and title outlive them.
    let hwnd = unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let icon = LoadImageW(Some(hinst), PCWSTR(1 as _), IMAGE_ICON, 0, 0, LR_DEFAULTSIZE | LR_SHARED)
            .map(|h| HICON(h.0))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default();
        RegisterClassExW(&WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc::wndproc),
            hInstance: hinst,
            hIcon: icon,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        });
        // Created on the primary monitor so GetDpiForWindow matches where it will be centered.
        let work = work_area();
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            &HSTRING::from("busy Settings"),
            WS_OVERLAPPEDWINDOW,
            work.left,
            work.top,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )?
    };
    // SAFETY: `hwnd` is the window just created.
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let ui = Rc::new(Ui {
        hwnd,
        gfx,
        fonts,
        rt: RefCell::new(None),
        dpi: Cell::new(dpi),
        cfg: RefCell::new(cfg.clone()),
        host,
        view: RefCell::new(View::new(Page::General)),
        choices: RefCell::new(Choices::default()),
        cell_fonts,
        shown: RefCell::new(None),
        worker: Worker::start(hwnd),
        system_dark: Cell::new(false),
        reg_autostart: Cell::new(None),
        pending: Cell::new(None),
        pressed: Cell::new(None),
        tracking: Cell::new(false),
    });
    UI.set(Some(ui.clone()));
    let s = dpi as f32 / 96.0;
    let (ww, wh) = ((WIN_W * s).round() as i32, (WIN_H * s).round() as i32);
    let work = work_area();
    let (x, y) = (work.left + (work.right - work.left - ww) / 2, work.top + (work.bottom - work.top - wh) / 2);
    // SAFETY: our window; SWP_FRAMECHANGED makes WM_NCCALCSIZE drop the caption.
    unsafe {
        let _ = SetWindowPos(hwnd, None, x, y, ww, wh, SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }
    ui.apply_theme();
    ui.rebuild();
    // Stays hidden until the worker's first read, so it opens in the right theme; without a worker, it opens
    // now with what it has.
    if !ui.worker.submit(Job::Read) {
        show(hwnd);
    }
    Ok(())
}

fn show(hwnd: HWND) {
    // SAFETY: our live window.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

impl Ui {
    fn theme(&self) -> Theme {
        Theme::new(dark::wanted(self.cfg.borrow().theme, self.system_dark.get()))
    }

    fn apply_theme(&self) {
        dark::title_bar(self.hwnd, self.theme().dark);
        self.invalidate();
    }

    fn invalidate(&self) {
        // SAFETY: our live window; a null rect invalidates the whole client area.
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn scale(&self) -> f32 {
        self.dpi.get() as f32 / 96.0
    }

    /// Rebuilds the page's items and layout from the config, e.g. after an edit or a page switch.
    fn rebuild(&self) {
        let cfg = self.cfg.borrow();
        let mut v = self.view.borrow_mut();
        v.nav = model::nav(&cfg);
        v.items = model::items(v.page, &cfg, &self.choices.borrow());
        v.lay_out(&self.gfx, &self.fonts);
        drop(v);
        drop(cfg);
        self.invalidate();
    }

    fn show_page(&self, page: Page) {
        let mut v = self.view.borrow_mut();
        if v.page != page {
            v.page = page;
            v.scroll = 0.0;
        }
        v.popup = None;
        drop(v);
        self.shown.take();
        self.host.page(match page {
            Page::Module(m) => Some(m),
            Page::General => None,
        });
        self.rebuild();
    }

    fn edit(&self, e: Edit) {
        if let Edit::Flag(Flag::Autostart, on) = e {
            return self.set_autostart(on);
        }
        let mut c = self.cfg.borrow().clone();
        edit::apply(&mut c, &e);
        c.normalize();
        self.commit(c);
    }

    /// Makes `c` the applied config and hands it to the host, if it differs.
    fn commit(&self, c: Config) {
        if c == *self.cfg.borrow() {
            return;
        }
        let theme = self.cfg.borrow().theme != c.theme;
        *self.cfg.borrow_mut() = c.clone();
        if theme {
            self.apply_theme();
        }
        self.rebuild();
        self.host.apply(c);
    }

    /// Autostart is written by the worker; the toggle waits (drawn disabled) until the write is read back.
    fn set_autostart(&self, on: bool) {
        if self.reg_autostart.get().is_none() || self.pending.get().is_some() {
            return;
        }
        if self.worker.submit(Job::SetAutostart(on)) {
            self.pending.set(Some(false));
            self.invalidate();
        }
    }

    fn autostart_busy(&self) -> bool {
        self.reg_autostart.get().is_none() || self.pending.get().is_some()
    }

    fn on_reply(&self, r: Reply) {
        match r {
            Reply::Read { autostart, system_dark } => {
                self.system_dark.set(system_dark);
                self.apply_theme();
                self.reg_autostart.set(Some(autostart));
                self.reflect_autostart(autostart);
                // SAFETY: our live window.
                if !unsafe { IsWindowVisible(self.hwnd) }.as_bool() {
                    show(self.hwnd);
                }
            }
            Reply::Theme { system_dark } => {
                self.system_dark.set(system_dark);
                self.apply_theme();
            }
            Reply::SetAutostart { error, autostart } => {
                self.reg_autostart.set(Some(autostart));
                let close = self.pending.take() == Some(true);
                if let Some(e) = error {
                    let msg = HSTRING::from(format!("Couldn't update the startup entry.\n\n{e}"));
                    // SAFETY: our window as owner; the strings outlive the call.
                    unsafe { MessageBoxW(Some(self.hwnd), &msg, w!("busy"), MB_ICONERROR | MB_OK) };
                }
                // Reflects the registry as read back, not the request.
                self.reflect_autostart(autostart);
                if close {
                    self.close();
                }
            }
        }
    }

    fn reflect_autostart(&self, on: bool) {
        let mut c = self.cfg.borrow().clone();
        c.autostart = on;
        self.commit(c);
        self.invalidate();
    }

    /// Closes the window, or once a pending autostart write is done.
    fn close(&self) {
        if self.pending.get().is_some() {
            self.pending.set(Some(true));
            return;
        }
        // SAFETY: our live window.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }

    fn render(&self) {
        let mut rc = RECT::default();
        // SAFETY: our live window; `rc` is a valid out-pointer.
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        let size = D2D_SIZE_U { width: rc.right.max(1) as u32, height: rc.bottom.max(1) as u32 };
        if self.rt.borrow().is_none() {
            let dpi = self.dpi.get() as f32;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: dpi,
                dpiY: dpi,
                ..Default::default()
            };
            let hp = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd: self.hwnd,
                pixelSize: size,
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            // SAFETY: the property structs are valid for the call; the factory is a live COM object.
            *self.rt.borrow_mut() = unsafe { self.gfx.d2d.CreateHwndRenderTarget(&props, &hp) }.ok();
        }
        let Some(rt) = self.rt.borrow().clone() else { return };
        // SAFETY: the render target is a live COM object; drawing happens between BeginDraw and EndDraw.
        unsafe {
            if rt.GetPixelSize() != size {
                let _ = rt.Resize(&size);
            }
            rt.SetDpi(self.dpi.get() as f32, self.dpi.get() as f32);
        }
        // The preview draws from the host's readings, lent for the frame; without them it draws the rest.
        let mut drawn = false;
        self.host.with_data(&mut |snap, hist| {
            self.draw(&rt, (rc.right, rc.bottom), Some(controls::preview::Data { snap, hist }));
            drawn = true;
        });
        if !drawn {
            self.draw(&rt, (rc.right, rc.bottom), None);
        }
    }

    /// One frame into `rt`, between its BeginDraw and EndDraw.
    fn draw(&self, rt: &ID2D1HwndRenderTarget, _size: (i32, i32), data: Option<controls::preview::Data>) {
        let (theme, cfg, view) = (self.theme(), self.cfg.borrow(), self.view.borrow());
        // SAFETY: our live window.
        let maximized = unsafe { IsZoomed(self.hwnd) }.as_bool();
        let state = paint::State {
            cfg: &cfg,
            theme: &theme,
            fonts: &self.fonts,
            gfx: &self.gfx,
            cell_fonts: &self.cell_fonts,
            data,
            maximized,
            autostart_busy: self.autostart_busy(),
        };
        // SAFETY: the render target is a live COM object; drawing happens between BeginDraw and EndDraw.
        unsafe { rt.BeginDraw() };
        if let Ok(cv) = Canvas::new(rt, &self.gfx) {
            paint::paint(&cv, &view, &state);
        }
        // SAFETY: pairs the BeginDraw above.
        if unsafe { rt.EndDraw(None, None) } == Err(D2DERR_RECREATE_TARGET.into()) {
            *self.rt.borrow_mut() = None;
        }
        #[cfg(debug_assertions)]
        dump::frame(&self.gfx, _size, self.dpi.get(), |cv| paint::paint(cv, &view, &state));
    }

    /// The client size changed (or the DPI): the layout follows the new width.
    fn resized(&self) {
        let mut rc = RECT::default();
        // SAFETY: our live window; `rc` is a valid out-pointer.
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        let s = self.scale();
        let mut v = self.view.borrow_mut();
        v.w = rc.right as f32 / s;
        v.h = rc.bottom as f32 / s;
        v.popup = None;
        v.lay_out(&self.gfx, &self.fonts);
        drop(v);
        self.invalidate();
    }
}
