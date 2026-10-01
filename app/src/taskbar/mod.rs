//! The widget embedded in the taskbar: a layered WS_CHILD of `Shell_TrayWnd` rendered with per-pixel alpha.

mod explorer;
mod surface;
mod tip;

use surface::Surface;
use tip::Tip;

use crate::win::{self, Event, raise};
use busy_core::{Anchor, Config, Module};
use busy_ui::cell::{self, CELL_H, Cell, Fonts};
use busy_ui::ctx::Ctx;
use busy_ui::render::{Canvas, Gfx, Rect};
use busy_ui::theme::Theme;
use busy_win::Edge;
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
/// Between cells (design: the widget row's `gap: 2px`).
const GAP: f32 = 2.0;
/// NearTray: a 1×20 `--line` divider with 4 px margins after the cells, before the notification area (24×1
/// below the cells down a vertical taskbar).
const DIVIDER: f32 = GAP + 4.0 + 1.0 + 4.0;

/// Each drawn cell's extent along the taskbar in client pixels (x, or y when it is vertical), for hit-testing
/// in the window procedure.
struct Hits {
    vertical: bool,
    cells: Vec<(i32, i32, Module)>,
}

thread_local! {
    static HITS: RefCell<Hits> = const { RefCell::new(Hits { vertical: false, cells: Vec::new() }) };
}

/// The cell under the pointer of mouse message `lp`, if any.
fn hit(lp: LPARAM) -> Option<Module> {
    // GET_X_LPARAM / GET_Y_LPARAM.
    let (x, y) = ((lp.0 & 0xFFFF) as u16 as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32);
    HITS.with_borrow(|h| {
        let at = if h.vertical { y } else { x };
        h.cells.iter().find(|&&(l, r, _)| (l..r).contains(&at)).map(|&(.., m)| m)
    })
}

/// Where the taskbar lets us draw; any change calls for a new layout.
#[derive(Clone, Copy, Default, PartialEq)]
struct Geometry {
    dpi: u32,
    /// Taskbar client rect.
    client: RECT,
    /// On the left or right edge (taller than wide): cells stack top to bottom in a column.
    vertical: bool,
    /// `explorer::slot`: (anchor edge, room), along the taskbar.
    slot: (i32, i32),
}

/// Everything the widget's pixels depend on; an equal frame is not drawn again.
#[derive(PartialEq)]
struct Frame {
    dpi: u32,
    w_px: i32,
    h_px: i32,
    vertical: bool,
    hover: Option<Module>,
    active: Option<Module>,
    theme: Theme,
    cells: Vec<(cell::Key, f32)>,
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
    tip: Option<Tip>,
    /// Each drawn cell's tooltip text, from the last layout.
    tips: Vec<(Module, String)>,
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
                Ok((rt, Fonts::new(gfx)?))
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
                tip: Tip::create(hwnd),
                tips: Vec::new(),
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

    /// The screen edge of the taskbar we are embedded in (the flyout opens beside it).
    pub fn edge(&self) -> Edge {
        busy_win::window_edge(self.tray)
    }

    /// Records the cell under the pointer (drawn as `--hover`); true if that changed and a redraw is due.
    pub fn set_hover(&mut self, hover: Option<Module>) -> bool {
        let changed = std::mem::replace(&mut self.hover, hover) != hover;
        if changed {
            self.sync_tip();
        }
        changed
    }

    /// Shows the hovered cell's text in the tooltip (design `title`), none between cells.
    fn sync_tip(&mut self) {
        let text = self.hover.and_then(|m| self.tips.iter().find(|t| t.0 == m)).map_or("", |t| t.1.as_str());
        if let Some(tip) = &mut self.tip {
            tip.set(text);
        }
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
        let vertical = client.bottom - client.top > client.right - client.left;
        let slot = explorer::slot(self.tray, cfg, dpi as f32 / 96.0, &client, vertical);
        Geometry { dpi, client, vertical, slot }
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
        let Geometry { dpi, client, vertical, slot } = geom;
        let scale = dpi as f32 / 96.0;
        // The taskbar's thickness: its height, or its width when it is vertical.
        let thick_px = if vertical { client.right - client.left } else { client.bottom - client.top };
        let col_w = cell::column_width(thick_px as f32 / scale);
        let mut cells = cell::cells(ctx);
        // Each cell's length along the taskbar: its width, or its height in a column.
        let mut lens: Vec<f32> =
            cells.iter().map(|c| if vertical { c.column_height() } else { c.width(ctx.gfx, &self.fonts) }).collect();
        let divider = if ctx.cfg.anchor == Anchor::NearTray { DIVIDER } else { 0.0 };
        let total = |ls: &[f32]| ls.iter().sum::<f32>() + GAP * ls.len().saturating_sub(1) as f32 + divider;
        // Never cover the task buttons: drop trailing (lowest-priority) cells that don't fit.
        while !lens.is_empty() && (total(&lens) * scale).ceil() as i32 > slot.1 {
            lens.pop();
            cells.pop();
        }
        if vertical && col_w <= 0.0 {
            cells.clear();
            lens.clear();
        }
        self.tips.clear();
        self.tips.extend(cells.iter().map(|c| (c.module, c.tip())));
        self.sync_tip();
        HITS.with_borrow_mut(|hits| {
            hits.vertical = vertical;
            hits.cells.clear();
            let mut at = 0.0;
            for (c, len) in cells.iter().zip(&lens) {
                // Each cell owns half the gap on either side, so the pointer is always over some cell.
                let (l, r) = (at - GAP / 2.0, at + len + GAP / 2.0);
                hits.cells.push(((l * scale).round() as i32, (r * scale).round() as i32, c.module));
                at += len + GAP;
            }
        });
        if cells.is_empty() || thick_px <= 0 {
            if self.placed != RECT::default() {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
            }
            self.placed = RECT::default();
            self.drawn = None;
            return;
        }
        let len_px = (total(&lens) * scale).ceil() as i32;
        let end = if vertical { client.bottom } else { client.right };
        let at = if ctx.cfg.anchor == Anchor::NearTray { slot.0 - len_px } else { slot.0 };
        let at = at.clamp(0, (end - len_px).max(0));
        let (w_px, h_px) = if vertical { (thick_px, len_px) } else { (len_px, thick_px) };
        let rect = if vertical {
            RECT { left: 0, top: at, right: w_px, bottom: at + h_px }
        } else {
            RECT { left: at, top: 0, right: at + w_px, bottom: h_px }
        };
        self.place(rect);
        let (w, h) = (w_px as f32 / scale, h_px as f32 / scale);

        let frame = Frame {
            dpi,
            w_px,
            h_px,
            vertical,
            hover: self.hover,
            active: self.active,
            theme: *ctx.theme,
            cells: cells.iter().map(Cell::key).zip(lens.iter().copied()).collect(),
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
            let t = ctx.theme;
            // Cells are centered across the taskbar: 40 high across, `col_w` wide down a column.
            let (x0, y0) = if vertical { ((w - col_w) / 2.0, 0.0) } else { (0.0, ((h - CELL_H) / 2.0).max(0.0)) };
            let mut at = 0.0;
            for (c, len) in cells.iter().zip(&lens) {
                let r = if vertical { Rect::new(x0, at, col_w, *len) } else { Rect::new(at, y0, *len, CELL_H) };
                let m = Some(c.module);
                let bg = if m == self.active { Some(t.active) } else { (m == self.hover).then_some(t.hover) };
                if let Some(bg) = bg {
                    cv.round(r, 4.0, bg);
                }
                if vertical {
                    c.draw_column(&cv, ctx.gfx, &self.fonts, t, r);
                } else {
                    c.draw(&cv, ctx.gfx, &self.fonts, t, r);
                }
                at += len + GAP;
            }
            if divider > 0.0 {
                let line = if vertical {
                    Rect::new((w - 24.0) / 2.0, at + 4.0, 24.0, 1.0)
                } else {
                    Rect::new(at + 4.0, (h - 20.0) / 2.0, 1.0, 20.0)
                };
                cv.fill(line, t.line);
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
                let flags = SWP_NOACTIVATE | SWP_SHOWWINDOW;
                let _ = SetWindowPos(self.hwnd, Some(HWND_TOP), want.left, want.top, w, h, flags);
            }
        }
    }

    pub fn destroy(&mut self) {
        self.tip = None;
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
                raise(Event::WidgetHover(hit(lp)));
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
                if let Some(m) = hit(lp) {
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
