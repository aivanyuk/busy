//! The widget embedded in the taskbar: a layered WS_CHILD of `Shell_TrayWnd` rendered with per-pixel alpha.

mod cells;
mod explorer;
mod surface;

use cells::Fonts;
use surface::Surface;

use crate::ctx::Ctx;
use crate::render::{Canvas, Gfx, Rect};
use crate::win::{self, Event, raise};
use busy_core::Anchor;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

const CLASS: PCWSTR = w!("busy.taskbar");
const PAD: f32 = 6.0;
const GAP: f32 = 12.0;

pub struct Taskbar {
    hwnd: HWND,
    tray: HWND,
    rt: ID2D1DCRenderTarget,
    fonts: Fonts,
    surf: Option<Surface>,
    placed: RECT,
    dpi: u32,
    hover: bool,
}

impl Taskbar {
    pub fn create(gfx: &Gfx) -> Option<Self> {
        let tray = explorer::find_tray()?;
        win::register_class(CLASS, Some(wndproc));
        unsafe {
            // Created directly as a child of the (foreign-process) taskbar; equivalent to SetParent
            // on a popup but without the style flip. WS_EX_LAYERED on a child needs the Win8 manifest.
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOPARENTNOTIFY,
                CLASS,
                w!("busy"),
                WS_CHILD | WS_CLIPSIBLINGS,
                0,
                0,
                0,
                0,
                Some(tray),
                None,
                Some(win::hinstance()),
                None,
            )
            .ok()?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                ..Default::default()
            };
            let made = (|| -> Result<_> {
                let rt = gfx.d2d.CreateDCRenderTarget(&props)?;
                rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                let fonts = Fonts {
                    label: gfx.format(10.0, false)?,
                    value: gfx.format(13.5, true)?,
                    pair: gfx.format(11.5, true)?,
                    tiny: gfx.format(10.0, true)?,
                };
                Ok((rt, fonts))
            })();
            let Ok((rt, fonts)) = made else {
                let _ = DestroyWindow(hwnd);
                return None;
            };
            Some(Self { hwnd, tray, rt, fonts, surf: None, placed: RECT::default(), dpi: 0, hover: false })
        }
    }

    /// False once explorer restarted (our child died with the old taskbar) or the taskbar was replaced.
    pub fn is_alive(&self) -> bool {
        unsafe {
            IsWindow(Some(self.hwnd)).as_bool()
                && IsWindow(Some(self.tray)).as_bool()
                && explorer::find_tray() == Some(self.tray)
        }
    }

    /// DPI of the taskbar we are embedded in (the flyout opens at the same scale).
    pub fn dpi(&self) -> u32 {
        unsafe { GetDpiForWindow(self.tray) }
    }

    /// Records whether the pointer is over the widget; true if that changed and a redraw is due.
    pub fn set_hover(&mut self, hover: bool) -> bool {
        std::mem::replace(&mut self.hover, hover) != hover
    }

    pub fn screen_rect(&self) -> RECT {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut r);
        }
        r
    }

    /// Re-lays out, repositions (only when changed) and redraws the widget.
    pub fn render(&mut self, ctx: &Ctx) {
        let dpi = unsafe { GetDpiForWindow(self.tray) }.max(96);
        let scale = dpi as f32 / 96.0;
        if dpi != self.dpi {
            self.dpi = dpi;
            unsafe { self.rt.SetDpi(dpi as f32, dpi as f32) };
        }
        let mut client = RECT::default();
        unsafe {
            let _ = GetClientRect(self.tray, &mut client);
        }
        let h_px = client.bottom - client.top;
        let mut cells = cells::cells(ctx);
        let mut widths: Vec<f32> = cells.iter().map(|c| c.width(ctx.gfx, &self.fonts)).collect();
        let total = |ws: &[f32]| 2.0 * PAD + ws.iter().sum::<f32>() + GAP * ws.len().saturating_sub(1) as f32;
        let slot = explorer::slot(self.tray, ctx.cfg, scale, &client);
        // Never cover the task buttons: drop trailing (lowest-priority) cells that don't fit.
        while !widths.is_empty() && (total(&widths) * scale).ceil() as i32 > slot.1 {
            widths.pop();
            cells.pop();
        }
        if cells.is_empty() || h_px <= 0 {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.placed = RECT::default();
            return;
        }
        let h = h_px as f32 / scale;
        let w = total(&widths);
        let w_px = (w * scale).ceil() as i32;
        self.place(ctx.cfg.anchor, slot, w_px, h_px, &client);

        if self.surf.as_ref().is_none_or(|s| s.w != w_px || s.h != h_px) {
            self.surf = None;
            self.surf = Surface::new(w_px, h_px);
        }
        let Some(surf) = &self.surf else { return };
        let bind = RECT { left: 0, top: 0, right: w_px, bottom: h_px };
        unsafe {
            if self.rt.BindDC(surf.dc, &bind).is_err() {
                return;
            }
            self.rt.BeginDraw();
            // Alpha 1/255 instead of 0 keeps the whole widget hit-testable.
            self.rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 / 255.0 }));
        }
        if let Ok(cv) = Canvas::new(&self.rt, ctx.gfx) {
            if self.hover {
                cv.round(Rect::new(1.0, 4.0, w - 2.0, h - 8.0), 4.0, ctx.theme.hover);
            }
            let mut x = PAD;
            for (c, cw) in cells.iter().zip(&widths) {
                c.draw(&cv, &self.fonts, ctx, Rect::new(x, 0.0, *cw, h));
                x += cw + GAP;
            }
        }
        unsafe {
            if self.rt.EndDraw(None, None).is_err() {
                return;
            }
            let size = SIZE { cx: w_px, cy: h_px };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
                BlendFlags: 0,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                None,
                Some(&size),
                Some(surf.dc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
        #[cfg(debug_assertions)]
        surface::debug_dump(surf, ctx.theme.dark);
    }

    fn place(&mut self, anchor: Anchor, (edge, _): (i32, i32), w: i32, h: i32, client: &RECT) {
        let x = if anchor == Anchor::NearTray { edge - w } else { edge };
        let x = x.clamp(0, (client.right - w).max(0));
        let want = RECT { left: x, top: 0, right: x + w, bottom: h };
        unsafe {
            // Something (e.g. the XAML island) was raised above us: restore our z-order.
            let covered = GetWindow(self.hwnd, GW_HWNDPREV).is_ok();
            if want != self.placed || covered || !IsWindowVisible(self.hwnd).as_bool() {
                let _ = SetWindowPos(self.hwnd, Some(HWND_TOP), x, 0, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                self.placed = want;
            }
        }
    }

    pub fn destroy(&mut self) {
        self.surf = None;
        unsafe {
            if IsWindow(Some(self.hwnd)).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

impl Drop for Taskbar {
    fn drop(&mut self) {
        self.destroy();
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_SETCURSOR => {
                let _ = SetCursor(LoadCursorW(None, IDC_ARROW).ok());
                LRESULT(1)
            }
            WM_MOUSEMOVE => {
                raise(Event::WidgetHover(true));
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                raise(Event::WidgetHover(false));
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                raise(Event::WidgetClick);
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                raise(Event::WidgetMenu);
                LRESULT(0)
            }
            WM_DPICHANGED_AFTERPARENT | WM_DISPLAYCHANGE => {
                raise(Event::WidgetRerender);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
