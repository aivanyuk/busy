//! The widget embedded in the taskbar: a layered WS_CHILD of `Shell_TrayWnd` rendered with per-pixel alpha.

mod cells;
mod explorer;
mod surface;

use cells::{Cell, Fonts};
use surface::Surface;

use crate::ctx::Ctx;
use crate::render::{Canvas, Gfx, Rect};
use crate::theme::Theme;
use crate::win::{self, Event, raise};
use busy_core::{Anchor, Config, Module};
use std::cell::RefCell;
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

thread_local! {
    /// Each drawn cell's horizontal extent in client pixels, for hit-testing in the window procedure.
    static HITS: RefCell<Vec<(i32, i32, Module)>> = const { RefCell::new(Vec::new()) };
}

/// The cell at client x `x`, if any.
fn hit(x: i32) -> Option<Module> {
    HITS.with_borrow(|h| h.iter().find(|&&(l, r, _)| (l..r).contains(&x)).map(|&(.., m)| m))
}

/// Where the taskbar lets us draw; any change calls for a new layout.
#[derive(Clone, Copy, Default, PartialEq)]
struct Geometry {
    dpi: u32,
    /// Taskbar client rect.
    client: RECT,
    /// `explorer::slot`: (anchor edge, room).
    slot: (i32, i32),
}

/// Everything the widget's pixels depend on; an equal frame is not drawn again.
#[derive(PartialEq)]
struct Frame {
    dpi: u32,
    w_px: i32,
    h_px: i32,
    hover: Option<Module>,
    active: Option<Module>,
    theme: Theme,
    cells: Vec<(cells::Key, f32)>,
}

pub struct Taskbar {
    hwnd: HWND,
    tray: HWND,
    rt: ID2D1DCRenderTarget,
    fonts: Fonts,
    surf: Option<Surface>,
    placed: RECT,
    geom: Geometry,
    /// What the surface and the layered window currently show.
    drawn: Option<Frame>,
    /// The cell under the pointer.
    hover: Option<Module>,
    /// The cell whose flyout is open.
    active: Option<Module>,
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
            Some(Self {
                hwnd,
                tray,
                rt,
                fonts,
                surf: None,
                placed: RECT::default(),
                geom: Geometry::default(),
                drawn: None,
                hover: None,
                active: None,
            })
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

    /// Records the cell under the pointer (drawn as `--hover`); true if that changed and a redraw is due.
    pub fn set_hover(&mut self, hover: Option<Module>) -> bool {
        std::mem::replace(&mut self.hover, hover) != hover
    }

    /// Records the cell whose flyout is open (drawn as `--active`); true if that changed and a redraw is due.
    pub fn set_active(&mut self, active: Option<Module>) -> bool {
        std::mem::replace(&mut self.active, active) != active
    }

    pub fn screen_rect(&self) -> RECT {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut r);
        }
        r
    }

    fn geometry(&self, cfg: &Config) -> Geometry {
        let dpi = unsafe { GetDpiForWindow(self.tray) }.max(96);
        let mut client = RECT::default();
        unsafe {
            let _ = GetClientRect(self.tray, &mut client);
        }
        let slot = explorer::slot(self.tray, cfg, dpi as f32 / 96.0, &client);
        Geometry { dpi, client, slot }
    }

    /// Timer path: follows taskbar geometry changes, otherwise only restores z-order and visibility.
    /// Content changes arrive through `render`, so an unchanged taskbar costs no layout or drawing here.
    pub fn watch(&mut self, ctx: &Ctx) {
        if self.geometry(ctx.cfg) != self.geom {
            self.render(ctx);
        } else if self.placed != RECT::default() {
            self.place(self.placed);
        }
    }

    /// Re-lays out and repositions the widget (moving it only on change); draws only when the frame differs
    /// from the one on screen.
    pub fn render(&mut self, ctx: &Ctx) {
        let geom = self.geometry(ctx.cfg);
        if geom.dpi != self.geom.dpi {
            unsafe { self.rt.SetDpi(geom.dpi as f32, geom.dpi as f32) };
        }
        self.geom = geom;
        let Geometry { dpi, client, slot } = geom;
        let scale = dpi as f32 / 96.0;
        let h_px = client.bottom - client.top;
        let mut cells = cells::cells(ctx);
        let mut widths: Vec<f32> = cells.iter().map(|c| c.width(ctx.gfx, &self.fonts)).collect();
        let total = |ws: &[f32]| 2.0 * PAD + ws.iter().sum::<f32>() + GAP * ws.len().saturating_sub(1) as f32;
        // Never cover the task buttons: drop trailing (lowest-priority) cells that don't fit.
        while !widths.is_empty() && (total(&widths) * scale).ceil() as i32 > slot.1 {
            widths.pop();
            cells.pop();
        }
        HITS.with_borrow_mut(|hits| {
            hits.clear();
            let mut x = PAD;
            for (c, cw) in cells.iter().zip(&widths) {
                // Each cell owns half the gap on either side, so the pointer is always over some cell.
                let (l, r) = (x - GAP / 2.0, x + cw + GAP / 2.0);
                hits.push(((l * scale).round() as i32, (r * scale).round() as i32, c.module()));
                x += cw + GAP;
            }
        });
        if cells.is_empty() || h_px <= 0 {
            if self.placed != RECT::default() {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
            }
            self.placed = RECT::default();
            self.drawn = None;
            return;
        }
        let h = h_px as f32 / scale;
        let w = total(&widths);
        let w_px = (w * scale).ceil() as i32;
        let x = if ctx.cfg.anchor == Anchor::NearTray { slot.0 - w_px } else { slot.0 };
        let x = x.clamp(0, (client.right - w_px).max(0));
        self.place(RECT { left: x, top: 0, right: x + w_px, bottom: h_px });

        let frame = Frame {
            dpi,
            w_px,
            h_px,
            hover: self.hover,
            active: self.active,
            theme: *ctx.theme,
            cells: cells.iter().map(Cell::key).zip(widths.iter().copied()).collect(),
        };
        if self.drawn.as_ref() == Some(&frame) {
            return;
        }
        self.drawn = None;
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
            let mut x = PAD;
            for (c, cw) in cells.iter().zip(&widths) {
                let m = Some(c.module());
                let bg = if m == self.active {
                    Some(ctx.theme.active)
                } else {
                    (m == self.hover).then_some(ctx.theme.hover)
                };
                if let Some(bg) = bg {
                    cv.round(Rect::new(x - GAP / 2.0 + 1.0, 4.0, cw + GAP - 2.0, h - 8.0), 4.0, bg);
                }
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
            let shown = UpdateLayeredWindow(
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
            if shown.is_ok() {
                self.drawn = Some(frame);
            }
        }
        #[cfg(debug_assertions)]
        surface::debug_dump(surf, ctx.theme.tb);
    }

    /// Moves the window to `want` if that changed, and restores z-order and visibility if explorer changed them.
    fn place(&mut self, want: RECT) {
        let moved = std::mem::replace(&mut self.placed, want) != want;
        unsafe {
            // Something (e.g. the XAML island) was raised above us: restore our z-order.
            let covered = GetWindow(self.hwnd, GW_HWNDPREV).is_ok();
            if moved || covered || !IsWindowVisible(self.hwnd).as_bool() {
                let (w, h) = (want.right - want.left, want.bottom - want.top);
                let _ = SetWindowPos(self.hwnd, Some(HWND_TOP), want.left, 0, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
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
                raise(Event::WidgetHover(hit(x_of(lp))));
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
                raise(Event::WidgetHover(None));
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(m) = hit(x_of(lp)) {
                    raise(Event::WidgetClick(m));
                }
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

/// Client x of a mouse message (`GET_X_LPARAM`).
fn x_of(lp: LPARAM) -> i32 {
    (lp.0 & 0xFFFF) as u16 as i16 as i32
}
