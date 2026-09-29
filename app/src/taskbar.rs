//! The widget embedded in the taskbar: a layered WS_CHILD of `Shell_TrayWnd` rendered with per-pixel alpha.

use crate::ctx::Ctx;
use crate::history::Series;
use crate::render::{Align, Canvas, Gfx, Rect, nice_max};
use crate::theme::{Color, rgba};
use crate::win::{self, Event, raise};
use crate::{fmt, select};
use busy_core::{Anchor, CellStyle, Config, Module};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;
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
const GRAPH_W: f32 = 54.0;
const BAR_W: f32 = 4.0;

pub struct Taskbar {
    pub hwnd: HWND,
    pub tray: HWND,
    rt: ID2D1DCRenderTarget,
    fonts: Fonts,
    surf: Option<Surface>,
    placed: RECT,
    dpi: u32,
    pub hover: bool,
}

struct Fonts {
    label: IDWriteTextFormat,
    value: IDWriteTextFormat,
    pair: IDWriteTextFormat,
    tiny: IDWriteTextFormat,
}

/// 32-bpp top-down DIB selected into a memory DC.
struct Surface {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    w: i32,
    h: i32,
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    bits: *mut u8,
}

impl Surface {
    fn new(w: i32, h: i32) -> Option<Self> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let Ok(bmp) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, bmp.into());
            Some(Self { dc, bmp, old, w, h, bits: bits as _ })
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

pub fn find_tray() -> Option<HWND> {
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None).ok() }
}

fn child(parent: HWND, class: PCWSTR) -> Option<HWND> {
    unsafe { FindWindowExW(Some(parent), None, class, None).ok() }
}

/// Window rect of `h` in `parent`'s client coordinates.
fn rect_in(h: HWND, parent: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    unsafe {
        GetWindowRect(h, &mut r).ok()?;
        let mut pts = [POINT { x: r.left, y: r.top }, POINT { x: r.right, y: r.bottom }];
        MapWindowPoints(None, Some(parent), &mut pts);
        Some(RECT { left: pts[0].x, top: pts[0].y, right: pts[1].x, bottom: pts[1].y })
    }
}

impl Taskbar {
    pub fn create(gfx: &Gfx) -> Option<Self> {
        let tray = find_tray()?;
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
            IsWindow(Some(self.hwnd)).as_bool() && IsWindow(Some(self.tray)).as_bool() && find_tray() == Some(self.tray)
        }
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
        let mut cells = cells(ctx);
        let mut widths: Vec<f32> = cells.iter().map(|c| c.width(ctx.gfx, &self.fonts)).collect();
        let total = |ws: &[f32]| 2.0 * PAD + ws.iter().sum::<f32>() + GAP * ws.len().saturating_sub(1) as f32;
        let slot = self.slot(ctx.cfg, scale, &client);
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
        debug_dump(surf, ctx.theme.dark);
    }

    /// Where the widget may go, in taskbar client pixels: (anchor edge, room). For `NearTray` the edge is the
    /// widget's right side, for `Left` its left side; `room` is the width available before the task buttons.
    fn slot(&self, cfg: &Config, scale: f32, client: &RECT) -> (i32, i32) {
        let px = |dip: f32| (dip * scale).round() as i32;
        let rect = |class| child(self.tray, class).and_then(|h| rect_in(h, self.tray));
        // TrayNotifyWnd is kept in sync with the XAML notification area on Win11 (verified on 26200).
        let tray_left = rect(w!("TrayNotifyWnd")).map_or(client.right, |r| r.left);
        // The (hidden) legacy Start window still tracks the XAML Start button. With centered icons the
        // button group is symmetric around the taskbar center, which gives its right end; ReBarWindow32
        // (the legacy task list) is not kept in sync, so it is only a fallback for left alignment.
        let start = rect(w!("Start"));
        let left_aligned = start.is_none_or(|r| r.left < px(40.0));
        let (tasks_left, tasks_right) = match start {
            Some(s) if !left_aligned => (s.left, client.right - s.left),
            _ => (0, rect(w!("ReBarWindow32")).map_or(client.right / 2, |r| r.right)),
        };
        let off = px(cfg.offset_px as f32);
        let gap = px(8.0);
        match cfg.anchor {
            Anchor::NearTray => {
                let edge = tray_left - off;
                (edge, edge - tasks_right - gap)
            }
            Anchor::Left if left_aligned => {
                let edge = tasks_right + gap + off;
                (edge, tray_left - gap - edge)
            }
            Anchor::Left => (off, tasks_left - gap - off),
        }
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

enum Val {
    One(String),
    /// Two stacked (prefix, prefix color, text) rows, e.g. upload/download.
    Two([(&'static str, Color, String); 2]),
}

struct Cell<'a> {
    style: CellStyle,
    label: String,
    val: Val,
    /// Widest strings the value can take, so the cell doesn't jitter.
    worst: &'static [&'static str],
    short: String,
    series: Vec<(&'a Series, Color)>,
    max: f32,
    bars: Vec<(f32, Color)>,
}

const PCT: &[&str] = &["100%"];
const RATES: &[&str] = &["99.9 MB/s", "999 MB/s", "99.9 KB/s", "999 KB/s"];

fn cells<'a>(ctx: &Ctx<'a>) -> Vec<Cell<'a>> {
    let (snap, hist, t) = (ctx.snap, ctx.hist, ctx.theme);
    let mut out = Vec::new();
    for mc in ctx.cfg.modules.iter().filter(|m| m.taskbar) {
        let base = |label: &str, val, worst, short: String| Cell {
            style: mc.style,
            label: label.into(),
            val,
            worst,
            short,
            series: Vec::new(),
            max: 100.0,
            bars: Vec::new(),
        };
        let cell = match mc.module {
            Module::Cpu => snap.cpu.as_ref().map(|c| Cell {
                series: vec![(&hist.cpu, t.accent)],
                bars: vec![(c.total / 100.0, t.level(t.accent, c.total))],
                ..base("CPU", Val::One(fmt::pct(c.total)), PCT, fmt::pct(c.total))
            }),
            Module::Memory => snap.memory.as_ref().filter(|m| m.total > 0).map(|m| {
                let p = m.used as f32 * 100.0 / m.total as f32;
                Cell {
                    series: vec![(&hist.mem, t.mem)],
                    bars: vec![(p / 100.0, t.level(t.mem, p))],
                    ..base("MEM", Val::One(fmt::pct(p)), PCT, fmt::pct(p))
                }
            }),
            Module::Gpu => {
                snap.gpus.iter().enumerate().max_by(|a, b| a.1.util_pct.total_cmp(&b.1.util_pct)).map(|(i, g)| Cell {
                    series: hist.gpus.get(i).map(|s| vec![(s, t.gpu)]).unwrap_or_default(),
                    bars: snap.gpus.iter().take(2).map(|g| (g.util_pct / 100.0, t.level(t.gpu, g.util_pct))).collect(),
                    ..base("GPU", Val::One(fmt::pct(g.util_pct)), PCT, fmt::pct(g.util_pct))
                })
            }
            Module::Network => snap.net.as_ref().map(|n| {
                let max = nice_max(hist.net_rx.max().max(hist.net_tx.max()));
                Cell {
                    series: vec![(&hist.net_rx, t.rx), (&hist.net_tx, t.tx)],
                    max,
                    bars: vec![(n.rx_bps as f32 / max, t.rx), (n.tx_bps as f32 / max, t.tx)],
                    ..base(
                        "NET",
                        Val::Two([("↑", t.tx, fmt::rate(n.tx_bps)), ("↓", t.rx, fmt::rate(n.rx_bps))]),
                        RATES,
                        format!("↓{}", fmt::rate_short(n.rx_bps)),
                    )
                }
            }),
            Module::Disk => (!snap.disks.is_empty()).then(|| {
                let (r, w) = (
                    snap.disks.iter().map(|d| d.read_bps).sum::<f64>(),
                    snap.disks.iter().map(|d| d.write_bps).sum::<f64>(),
                );
                let max = nice_max(hist.disk_r.max().max(hist.disk_w.max()));
                Cell {
                    series: vec![(&hist.disk_r, t.rx), (&hist.disk_w, t.tx)],
                    max,
                    bars: vec![(r as f32 / max, t.rx), (w as f32 / max, t.tx)],
                    ..base(
                        "DISK",
                        Val::Two([("R", t.rx, fmt::rate(r)), ("W", t.tx, fmt::rate(w))]),
                        RATES,
                        fmt::rate_short(r + w),
                    )
                }
            }),
            Module::Battery => snap.battery.as_ref().map(|b| {
                let v = format!("{}{}", fmt::pct(b.percent), if b.charging { "⚡" } else { "" });
                let c = if b.percent < 20.0 && !b.charging { t.crit } else { t.battery };
                Cell {
                    series: vec![(&hist.battery, c)],
                    bars: vec![(b.percent / 100.0, c)],
                    ..base("BAT", Val::One(v.clone()), &["100%⚡"], v)
                }
            }),
            Module::Sensors => select::pinned_sensor(snap, ctx.cfg).map(|s| {
                let v = fmt::sensor(s.value, s.kind, ctx.cfg.temp_unit);
                let is_temp = s.kind == busy_core::SensorKind::Temperature;
                let max = if is_temp { 100.0 } else { nice_max(hist.sensor.max()) };
                let label =
                    if is_temp { "TEMP".into() } else { s.name.chars().take(6).collect::<String>().to_uppercase() };
                let col = if is_temp { t.temp(s.value) } else { t.text };
                let bar_col = if is_temp && col == t.text { t.accent } else { col };
                Cell {
                    series: vec![(&hist.sensor, bar_col)],
                    max,
                    bars: vec![(s.value / max, bar_col)],
                    ..base(&label, Val::One(v.clone()), &["100°C", "212°F", "8888 rpm", "888.8 W"], v)
                }
            }),
            Module::Processes => snap.top.by_cpu.first().map(|p| {
                let name: String = p.name.trim_end_matches(".exe").chars().take(12).collect();
                Cell {
                    bars: vec![(p.cpu_pct / 100.0, t.accent)],
                    ..base(&name, Val::One(fmt::pct(p.cpu_pct)), &["100%", "WWWWWWWW"], fmt::pct(p.cpu_pct))
                }
            }),
        };
        out.extend(cell);
    }
    out
}

impl Cell<'_> {
    fn text_width(&self, gfx: &Gfx, f: &Fonts) -> f32 {
        let worst = |fmt: &IDWriteTextFormat| self.worst.iter().map(|s| gfx.text_width(fmt, s)).fold(0.0, f32::max);
        match &self.val {
            Val::One(v) => gfx.text_width(&f.label, &self.label).max(worst(&f.value)).max(gfx.text_width(&f.value, v)),
            Val::Two(rows) => {
                let prefix = rows.iter().map(|r| gfx.text_width(&f.pair, r.0)).fold(0.0, f32::max);
                prefix
                    + 3.0
                    + worst(&f.pair).max(rows.iter().map(|r| gfx.text_width(&f.pair, &r.2)).fold(0.0, f32::max))
            }
        }
    }

    fn bars_width(&self) -> f32 {
        self.bars.len() as f32 * (BAR_W + 2.0) - 2.0
    }

    fn width(&self, gfx: &Gfx, f: &Fonts) -> f32 {
        match self.style {
            CellStyle::Text => self.text_width(gfx, f),
            CellStyle::Graph => {
                GRAPH_W.max(gfx.text_width(&f.label, &self.label) + gfx.text_width(&f.tiny, &self.short) + 6.0)
            }
            CellStyle::Bar => self.bars_width() + 6.0 + self.text_width(gfx, f),
        }
        .ceil()
    }

    fn draw_text(&self, cv: &Canvas, f: &Fonts, ctx: &Ctx, r: Rect) {
        let t = ctx.theme;
        let y0 = (r.h - 30.0) / 2.0;
        match &self.val {
            Val::One(v) => {
                cv.text(&self.label, &f.label, Rect::new(r.x, y0, r.w, 13.0), t.secondary, Align::Left);
                cv.text(v, &f.value, Rect::new(r.x, y0 + 12.0, r.w, 18.0), t.text, Align::Left);
            }
            Val::Two(rows) => {
                let pw = rows.iter().map(|row| ctx.gfx.text_width(&f.pair, row.0)).fold(0.0, f32::max);
                for (i, (prefix, col, text)) in rows.iter().enumerate() {
                    let y = y0 + i as f32 * 15.0;
                    cv.text(prefix, &f.pair, Rect::new(r.x, y, pw, 15.0), *col, Align::Left);
                    cv.text(text, &f.pair, Rect::new(r.x + pw + 3.0, y, r.w - pw - 3.0, 15.0), t.text, Align::Left);
                }
            }
        }
    }

    fn draw(&self, cv: &Canvas, f: &Fonts, ctx: &Ctx, r: Rect) {
        let t = ctx.theme;
        match self.style {
            CellStyle::Text => self.draw_text(cv, f, ctx, r),
            CellStyle::Graph => {
                let y0 = (r.h - 32.0) / 2.0;
                cv.text(&self.label, &f.label, Rect::new(r.x, y0, r.w, 13.0), t.secondary, Align::Left);
                cv.text(&self.short, &f.tiny, Rect::new(r.x, y0, r.w, 13.0), t.text, Align::Right);
                let g = Rect::new(r.x, y0 + 15.0, r.w, 17.0);
                cv.round(g, 3.0, t.track);
                let inner = g.inset(1.0, 1.5);
                for (s, c) in &self.series {
                    // ~2 DIPs per sample: the full history would be unreadably dense at this size.
                    cv.graph(
                        inner,
                        s,
                        self.max,
                        *c,
                        if self.series.len() > 1 { 0.18 } else { 0.3 },
                        (inner.w / 2.0) as usize,
                    );
                }
            }
            CellStyle::Bar => {
                let bh = 30.0;
                let y0 = (r.h - bh) / 2.0;
                for (i, (frac, c)) in self.bars.iter().enumerate() {
                    cv.vbar(Rect::new(r.x + i as f32 * (BAR_W + 2.0), y0, BAR_W, bh), *frac, *c, rgba(0x808080, 0.28));
                }
                let dx = self.bars_width() + 6.0;
                self.draw_text(cv, f, ctx, Rect::new(r.x + dx, r.y, r.w - dx, r.h));
            }
        }
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

/// Debug builds: `BUSY_DUMP=<dir>` writes the widget (composited over a taskbar-ish color) to `taskbar.bmp`.
#[cfg(debug_assertions)]
fn debug_dump(s: &Surface, dark: bool) {
    let Some(dir) = std::env::var_os("BUSY_DUMP") else { return };
    let (w, h) = (s.w as usize, s.h as usize);
    let px = unsafe { std::slice::from_raw_parts(s.bits, w * h * 4) };
    let bg: [f32; 3] = if dark { [32.0, 32.0, 32.0] } else { [238.0, 238.0, 238.0] };
    let mut out = Vec::with_capacity(54 + w * h * 4);
    let file_len = (54 + w * h * 4) as u32;
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_len.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&[1, 0, 32, 0]);
    out.extend_from_slice(&[0; 24]);
    for p in px.as_chunks::<4>().0 {
        let a = p[3] as f32 / 255.0;
        for c in 0..3 {
            out.push((p[c] as f32 + bg[c] * (1.0 - a)).min(255.0) as u8);
        }
        out.push(255);
    }
    // Hard rule: no file I/O on the UI thread, not even in debug builds.
    std::thread::spawn(move || std::fs::write(std::path::Path::new(&dir).join("taskbar.bmp"), out));
}
