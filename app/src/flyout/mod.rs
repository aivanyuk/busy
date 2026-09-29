//! Detail flyout: a borderless tool window with a DWM backdrop, drawn with Direct2D.

mod painter;
mod sections;

use painter::{Fonts, Painter};

use crate::ctx::Ctx;
use crate::render::{Canvas, Gfx, Rect};
use crate::theme::{Color, Theme};
use crate::win::{self, Event, raise};
use busy_core::Anchor;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

const CLASS: PCWSTR = w!("busy.flyout");
const WIDTH: f32 = 360.0;
const PAD: f32 = 16.0;
const MARGIN: f32 = 12.0;

pub struct Flyout {
    hwnd: HWND,
    rt: Option<ID2D1HwndRenderTarget>,
    fonts: Fonts,
    visible: bool,
    /// Tick of the last hide caused by deactivation (see `toggle`).
    deactivated_at: u64,
    backdrop: bool,
    dpi: u32,
    anchor: RECT,
    anchor_side: Anchor,
    scroll: f32,
    view_h: f32,
    content_h: f32,
    mouse: Option<(f32, f32)>,
    tab: usize,
    hits: Vec<(Rect, usize)>,
}

impl Flyout {
    pub fn create(gfx: &Gfx, owner: HWND, theme: &Theme) -> Option<Self> {
        win::register_class(CLASS, Some(wndproc));
        let fonts = (|| -> Result<Fonts> {
            Ok(Fonts {
                title: gfx.format(14.0, true)?,
                body: gfx.format(12.0, false)?,
                bold: gfx.format(12.0, true)?,
                small: gfx.format(11.0, false)?,
                hint: gfx.wrapping(12.0)?,
            })
        })()
        .ok()?;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                CLASS,
                w!("busy"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                Some(owner),
                None,
                Some(win::hinstance()),
                None,
            )
            .ok()?
        };
        let mut f = Self {
            hwnd,
            rt: None,
            fonts,
            visible: false,
            deactivated_at: 0,
            backdrop: false,
            dpi: 96,
            anchor: RECT::default(),
            anchor_side: Anchor::NearTray,
            scroll: 0.0,
            view_h: 0.0,
            content_h: 0.0,
            mouse: None,
            tab: 0,
            hits: Vec::new(),
        };
        f.apply_theme(theme);
        Some(f)
    }

    pub fn apply_theme(&mut self, t: &Theme) {
        // SAFETY: `self.hwnd` is our live window; every attribute set here takes a 4-byte value, read from `v`
        // for the duration of the call.
        let set = |attr, v: i32| unsafe { DwmSetWindowAttribute(self.hwnd, attr, &v as *const i32 as _, 4).is_ok() };
        set(DWMWA_USE_IMMERSIVE_DARK_MODE, t.dark as i32);
        set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND.0);
        // DWM draws the 1 px border of rounded windows; give it `--fly-line` over `--fly` (COLORREF has no alpha).
        // Fails harmlessly before Windows 11.
        let (l, f) = (t.fly_line, t.fly);
        let mix = |a: f32, b: f32| ((a * l.a + b * (1.0 - l.a)) * 255.0).round() as u32;
        set(DWMWA_BORDER_COLOR, (mix(l.r, f.r) | mix(l.g, f.g) << 8 | mix(l.b, f.b) << 16) as i32);
        // Fails before Win11 22H2; we then paint a solid background.
        self.backdrop = set(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW.0);
        if self.backdrop {
            let m = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
            self.backdrop = unsafe { DwmExtendFrameIntoClientArea(self.hwnd, &m) }.is_ok();
        }
    }

    fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    /// Shows the flyout next to `anchor` (the widget's screen rect), or hides it if it is open.
    pub fn toggle(&mut self, ctx: &Ctx, anchor: RECT, dpi: u32) {
        if self.visible {
            self.hide(false);
        } else if unsafe { GetTickCount64() }.saturating_sub(self.deactivated_at) >= 250 {
            self.show(ctx, anchor, dpi);
        }
        // Otherwise the press that deactivated (and hid) the flyout was on the widget itself: stay closed.
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn is_foreground(&self) -> bool {
        unsafe { GetForegroundWindow() == self.hwnd }
    }

    fn show(&mut self, ctx: &Ctx, anchor: RECT, dpi: u32) {
        self.anchor = anchor;
        self.anchor_side = ctx.cfg.anchor;
        self.dpi = dpi;
        self.scroll = 0.0;
        self.visible = true;
        if let Some(rt) = &self.rt {
            unsafe { rt.SetDpi(dpi as f32, dpi as f32) };
        }
        self.render(ctx);
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    pub fn hide(&mut self, deactivated: bool) {
        if !self.visible {
            return;
        }
        self.visible = false;
        self.mouse = None;
        if deactivated {
            self.deactivated_at = unsafe { GetTickCount64() };
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// Sizes the window to its content (clamped to the work area) and positions it next to the widget.
    fn place(&mut self, content_h: f32) {
        let s = self.scale();
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        unsafe {
            let _ = GetMonitorInfoW(MonitorFromRect(&self.anchor, MONITOR_DEFAULTTOPRIMARY), &mut mi);
        }
        let wa = mi.rcWork;
        let m = (MARGIN * s).round() as i32;
        let w = (WIDTH * s).round() as i32;
        let h = ((content_h * s).ceil() as i32).min(wa.bottom - wa.top - 2 * m).max(1);
        self.view_h = h as f32 / s;
        let x = match self.anchor_side {
            Anchor::NearTray => self.anchor.right - w,
            Anchor::Left => self.anchor.left,
        }
        .clamp(wa.left + m, (wa.right - m - w).max(wa.left + m));
        let below_center = self.anchor.top > (wa.top + wa.bottom) / 2;
        let y = if below_center { wa.bottom - m - h } else { wa.top + m };
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
        }
    }

    pub fn render(&mut self, ctx: &Ctx) {
        if !self.visible {
            return;
        }
        let (content_h, _) = self.layout(ctx, None);
        self.content_h = content_h;
        self.place(content_h);
        self.scroll = self.scroll.clamp(0.0, (self.content_h - self.view_h).max(0.0));
        self.paint(ctx);
    }

    /// Redraws without re-measuring (mouse/scroll feedback).
    pub fn paint(&mut self, ctx: &Ctx) {
        if !self.visible {
            return;
        }
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        let size = D2D_SIZE_U { width: rc.right as u32, height: rc.bottom as u32 };
        if self.rt.is_none() {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: self.dpi as f32,
                dpiY: self.dpi as f32,
                ..Default::default()
            };
            let hp = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd: self.hwnd,
                pixelSize: size,
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            self.rt = unsafe { ctx.gfx.d2d.CreateHwndRenderTarget(&props, &hp) }.ok();
            if let Some(rt) = &self.rt {
                // Text sits on a (semi-)transparent surface, so ClearType is not possible.
                unsafe { rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE) };
            }
        }
        let Some(rt) = self.rt.clone() else { return };
        unsafe {
            if rt.GetPixelSize() != size {
                let _ = rt.Resize(&size);
            }
            rt.BeginDraw();
            let t = ctx.theme;
            // `--fly` is translucent over the blurred backdrop, like the design's backdrop-filter.
            rt.Clear(Some(&if self.backdrop { t.fly } else { Color { a: 1.0, ..t.fly } }));
        }
        let hits = Canvas::new(&rt, ctx.gfx).ok().map(|cv| {
            let (_, hits) = self.layout(ctx, Some(&cv));
            if self.content_h > self.view_h + 0.5 {
                let track = self.view_h - 8.0;
                let thumb = (track * self.view_h / self.content_h).max(24.0);
                let y = 4.0 + (track - thumb) * self.scroll / (self.content_h - self.view_h);
                cv.round(Rect::new(WIDTH - 6.0, y, 3.0, thumb), 1.5, ctx.theme.fg3);
            }
            hits
        });
        self.hits = hits.unwrap_or_default();
        let r = unsafe { rt.EndDraw(None, None) };
        if r == Err(D2DERR_RECREATE_TARGET.into()) {
            self.rt = None;
        }
    }

    fn layout(&self, ctx: &Ctx, cv: Option<&Canvas>) -> (f32, Vec<(Rect, usize)>) {
        let mut p = Painter {
            cv,
            ctx,
            f: &self.fonts,
            x: PAD,
            w: WIDTH - 2.0 * PAD,
            y: PAD - self.scroll,
            mouse: self.mouse,
            hits: Vec::new(),
            tab: self.tab,
        };
        let mut first = true;
        for mc in ctx.cfg.modules.iter().filter(|m| m.flyout) {
            if !first {
                p.separator();
            }
            first = false;
            p.section(mc);
        }
        if first {
            p.sub("No modules enabled for the flyout.");
        }
        (p.y + self.scroll + PAD - 4.0, p.hits)
    }

    pub fn on_mouse(&mut self, ctx: &Ctx, pos: Option<(f32, f32)>) {
        self.mouse = pos.map(|(x, y)| (x / self.scale(), y / self.scale()));
        self.paint(ctx);
    }

    pub fn on_wheel(&mut self, ctx: &Ctx, delta: i16) {
        let max = (self.content_h - self.view_h).max(0.0);
        self.scroll = (self.scroll - delta as f32 / 120.0 * 48.0).clamp(0.0, max);
        self.paint(ctx);
    }

    pub fn on_click(&mut self, ctx: &Ctx, x: f32, y: f32) {
        let (x, y) = (x / self.scale(), y / self.scale());
        if let Some(&(_, tab)) = self.hits.iter().find(|(r, _)| r.contains(x, y)) {
            self.tab = tab;
            self.paint(ctx);
        }
    }
}

impl Drop for Flyout {
    fn drop(&mut self) {
        self.rt = None;
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let xy = || ((lp.0 & 0xFFFF) as i16 as f32, ((lp.0 >> 16) & 0xFFFF) as i16 as f32);
    unsafe {
        match msg {
            WM_ACTIVATE => {
                if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE {
                    raise(Event::FlyoutDeactivated);
                }
                LRESULT(0)
            }
            WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize => {
                raise(Event::FlyoutEscape);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = ((wp.0 >> 16) & 0xFFFF) as i16;
                raise(Event::FlyoutWheel(delta));
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
                raise(Event::FlyoutPointer(Some(xy())));
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                raise(Event::FlyoutPointer(None));
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = xy();
                raise(Event::FlyoutClick(x, y));
                LRESULT(0)
            }
            WM_PAINT => {
                let _ = ValidateRect(Some(hwnd), None);
                raise(Event::FlyoutPaint);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            // Size/position are driven by the widget's monitor DPI; don't let the system resize us.
            WM_DPICHANGED => LRESULT(0),
            WM_MOUSEACTIVATE => LRESULT(MA_ACTIVATE as isize),
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
