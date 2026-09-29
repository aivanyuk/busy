//! Detail flyout: a borderless tool window with a DWM backdrop, drawn with Direct2D.

use crate::ctx::Ctx;
use crate::history::Series;
use crate::render::{Align, Canvas, Gfx, Rect, nice_max};
use crate::theme::{Color, Theme, alpha};
use crate::win::{self, Event, raise};
use crate::{fmt, select};
use busy_core::{Anchor, Module, ProcEntry, SensorKind};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;
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
const TABS: [&str; 4] = ["CPU", "Memory", "Disk", "GPU"];

struct Fonts {
    title: IDWriteTextFormat,
    body: IDWriteTextFormat,
    bold: IDWriteTextFormat,
    small: IDWriteTextFormat,
    hint: IDWriteTextFormat,
}

pub struct Flyout {
    pub hwnd: HWND,
    rt: Option<ID2D1HwndRenderTarget>,
    fonts: Fonts,
    pub visible: bool,
    /// Tick of the last hide caused by deactivation (see `App::toggle_flyout`).
    pub deactivated_at: u64,
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
        let set = |attr, v: i32| unsafe { DwmSetWindowAttribute(self.hwnd, attr, &v as *const i32 as _, 4).is_ok() };
        set(DWMWA_USE_IMMERSIVE_DARK_MODE, t.dark as i32);
        set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND.0);
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

    pub fn show(&mut self, ctx: &Ctx, anchor: RECT, dpi: u32) {
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
            rt.Clear(Some(&if self.backdrop {
                alpha(t.background, if t.dark { 0.5 } else { 0.4 })
            } else {
                t.background
            }));
        }
        let hits = Canvas::new(&rt, ctx.gfx).ok().map(|cv| {
            let (_, hits) = self.layout(ctx, Some(&cv));
            if self.content_h > self.view_h + 0.5 {
                let track = self.view_h - 8.0;
                let thumb = (track * self.view_h / self.content_h).max(24.0);
                let y = 4.0 + (track - thumb) * self.scroll / (self.content_h - self.view_h);
                cv.round(Rect::new(WIDTH - 6.0, y, 3.0, thumb), 1.5, ctx.theme.tertiary);
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
            p.section(mc.module);
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

struct Painter<'a> {
    cv: Option<&'a Canvas<'a>>,
    ctx: &'a Ctx<'a>,
    f: &'a Fonts,
    x: f32,
    w: f32,
    y: f32,
    mouse: Option<(f32, f32)>,
    hits: Vec<(Rect, usize)>,
    tab: usize,
}

type Fmt<'a> = &'a dyn Fn(f32) -> String;

impl Painter<'_> {
    fn t(&self) -> &Theme {
        self.ctx.theme
    }

    fn text(&self, s: &str, f: &IDWriteTextFormat, r: Rect, c: Color, a: Align) {
        if let Some(cv) = self.cv {
            cv.text(s, f, r, c, a);
        }
    }

    fn header(&mut self, title: &str, value: &str, c: Color) {
        let r = Rect::new(self.x, self.y, self.w, 22.0);
        self.text(title, &self.f.title, r, self.t().text, Align::Left);
        self.text(value, &self.f.title, r, c, Align::Right);
        self.y += 24.0;
    }

    fn sub(&mut self, s: &str) {
        self.text(s, &self.f.small, Rect::new(self.x, self.y, self.w, 16.0), self.t().secondary, Align::Left);
        self.y += 18.0;
    }

    fn group(&mut self, s: &str) {
        self.y += 4.0;
        self.text(s, &self.f.bold, Rect::new(self.x, self.y, self.w, 18.0), self.t().text, Align::Left);
        self.y += 19.0;
    }

    fn gap(&mut self, h: f32) {
        self.y += h;
    }

    fn separator(&mut self) {
        self.y += 12.0;
        if let Some(cv) = self.cv {
            cv.fill(Rect::new(self.x, self.y, self.w, 1.0), self.t().separator);
        }
        self.y += 13.0;
    }

    /// Two-column label/value grid.
    fn kv(&mut self, items: &[(&str, String)]) {
        let cw = (self.w - 20.0) / 2.0;
        for (i, (k, v)) in items.iter().enumerate() {
            let r = Rect::new(self.x + (i % 2) as f32 * (cw + 20.0), self.y, cw, 19.0);
            self.text(k, &self.f.body, r, self.t().secondary, Align::Left);
            self.text(v, &self.f.body, r, self.t().text, Align::Right);
            if i % 2 == 1 || i + 1 == items.len() {
                self.y += 19.0;
            }
        }
    }

    fn row(&mut self, left: &str, right: &str, rc: Color) {
        let r = Rect::new(self.x, self.y, self.w, 20.0);
        let rw = self.ctx.gfx.text_width(&self.f.body, right);
        self.text(left, &self.f.body, Rect { w: (self.w - rw - 8.0).max(0.0), ..r }, self.t().text, Align::Left);
        self.text(right, &self.f.body, r, rc, Align::Right);
        self.y += 20.0;
    }

    fn bar(&mut self, frac: f32, c: Color) {
        if let Some(cv) = self.cv {
            cv.hbar(Rect::new(self.x, self.y + 1.0, self.w, 5.0), frac, c, self.t().track);
        }
        self.y += 12.0;
    }

    fn hint(&mut self, s: &str) {
        let (_, h) = self.ctx.gfx.metrics(&self.f.hint, s, self.w);
        self.text(s, &self.f.hint, Rect::new(self.x, self.y, self.w, h), self.t().secondary, Align::Left);
        self.y += h + 4.0;
    }

    /// History graph with optional hover readout. `max_label` is drawn in the top-right corner.
    fn graph(&mut self, series: &[(&Series, Color)], max: f32, h: f32, fv: Fmt, max_label: Option<String>) {
        let r = Rect::new(self.x, self.y, self.w, h);
        self.y += h + 8.0;
        let Some(cv) = self.cv else { return };
        let t = self.t();
        cv.round(r, 4.0, t.track);
        for f in [0.25, 0.5, 0.75] {
            let y = (r.y + r.h * f).round();
            cv.hline(r.x + 4.0, r.right() - 4.0, y, alpha(t.separator, 0.7));
        }
        let inner = r.inset(1.0, 2.0);
        for (s, c) in series {
            cv.graph(inner, s, max, *c, if series.len() > 1 { 0.16 } else { 0.28 }, s.cap());
        }
        if let Some(l) = max_label {
            self.text(&l, &self.f.small, Rect::new(r.x + 6.0, r.y + 2.0, r.w - 12.0, 14.0), t.tertiary, Align::Right);
        }
        let Some((mx, my)) = self.mouse.filter(|&(x, y)| r.contains(x, y)) else { return };
        let Some((s0, _)) = series.first() else { return };
        let step = inner.w / (s0.cap().max(2) - 1) as f32;
        let k = ((inner.right() - mx) / step).round().max(0.0) as usize;
        if k >= s0.len() {
            return;
        }
        let x = inner.right() - k as f32 * step;
        cv.vline(x, r.y + 2.0, r.bottom() - 2.0, t.secondary);
        let secs = k as u64 * self.ctx.cfg.interval_ms as u64 / 1000;
        let mut label = series
            .iter()
            .filter(|(s, _)| k < s.len())
            .map(|(s, _)| fv(s.get(s.len() - 1 - k)))
            .collect::<Vec<_>>()
            .join("  ");
        if secs > 0 {
            label = format!("{label}  · {} ago", fmt::duration(secs));
        }
        let lw = self.ctx.gfx.text_width(&self.f.small, &label) + 12.0;
        let lx = if mx > r.x + r.w / 2.0 { x - lw - 4.0 } else { x + 4.0 };
        let lr = Rect::new(lx.clamp(r.x, r.right() - lw), my.clamp(r.y + 2.0, r.bottom() - 20.0) - 9.0, lw, 18.0);
        cv.round(lr, 4.0, if t.dark { alpha(t.background, 0.92) } else { alpha(crate::theme::rgb(0xFFFFFF), 0.95) });
        self.text(&label, &self.f.small, lr, t.text, Align::Center);
    }

    fn cores(&mut self, v: &[f32]) {
        if v.is_empty() {
            return;
        }
        let n = v.len() as f32;
        let gap = if v.len() > 32 { 1.0 } else { 2.0 };
        let bw = ((self.w - gap * (n - 1.0)) / n).min(16.0);
        if let Some(cv) = self.cv {
            let t = self.t();
            for (i, &p) in v.iter().enumerate() {
                cv.vbar(
                    Rect::new(self.x + i as f32 * (bw + gap), self.y, bw, 22.0),
                    p / 100.0,
                    t.level(t.accent, p),
                    t.track,
                );
            }
        }
        self.y += 30.0;
    }

    fn tabs(&mut self, names: &[&str]) {
        let r = Rect::new(self.x, self.y, self.w, 24.0);
        let tw = r.w / names.len() as f32;
        let t = *self.t();
        if let Some(cv) = self.cv {
            cv.round(r, 5.0, t.track);
        }
        for (i, name) in names.iter().enumerate() {
            let tr = Rect::new(r.x + i as f32 * tw, r.y, tw, r.h);
            if let Some(cv) = self.cv {
                if i == self.tab {
                    cv.round(tr.inset(2.0, 2.0), 4.0, alpha(t.accent, 0.25));
                } else if self.mouse.is_some_and(|(x, y)| tr.contains(x, y)) {
                    cv.round(tr.inset(2.0, 2.0), 4.0, t.hover);
                }
            }
            let f = if i == self.tab { &self.f.bold } else { &self.f.body };
            self.text(name, f, tr, if i == self.tab { t.text } else { t.secondary }, Align::Center);
            self.hits.push((tr, i));
        }
        self.y += 30.0;
    }

    fn section(&mut self, m: Module) {
        let ctx = self.ctx;
        let (snap, hist, t, unit) = (ctx.snap, ctx.hist, *ctx.theme, ctx.cfg.temp_unit);
        let pct = &|v: f32| fmt::pct(v);
        let rate = &|v: f32| fmt::rate(v as f64);
        match m {
            Module::Cpu => {
                let Some(c) = &snap.cpu else { return self.missing("CPU") };
                self.header("CPU", &fmt::pct(c.total), t.level(t.accent, c.total));
                if !c.name.is_empty() {
                    self.sub(c.name.trim());
                }
                self.gap(4.0);
                self.graph(&[(&hist.cpu, t.accent)], 100.0, 56.0, pct, None);
                self.cores(&c.per_core);
                let mut kv = vec![("User", fmt::pct(c.user)), ("System", fmt::pct(c.kernel))];
                if let Some(f) = c.freq_mhz {
                    kv.push(("Frequency", fmt::mhz(f)));
                }
                kv.push(("Cores", format!("{} / {}", c.physical_cores, c.logical_cores)));
                kv.push(("Processes", fmt::count(c.processes as u64)));
                kv.push(("Threads", fmt::count(c.threads as u64)));
                kv.push(("Handles", fmt::count(c.handles as u64)));
                kv.push(("Uptime", fmt::duration(c.uptime_secs)));
                self.kv(&kv);
            }
            Module::Memory => {
                let Some(mem) = snap.memory.as_ref().filter(|m| m.total > 0) else { return self.missing("Memory") };
                let p = mem.used as f32 * 100.0 / mem.total as f32;
                self.header("Memory", &fmt::pct(p), t.level(t.mem, p));
                self.sub(&format!("{} of {} used", fmt::bytes(mem.used), fmt::bytes(mem.total)));
                self.gap(2.0);
                self.bar(p / 100.0, t.level(t.mem, p));
                self.graph(&[(&hist.mem, t.mem)], 100.0, 44.0, pct, None);
                let mut kv = vec![("Used", fmt::bytes(mem.used)), ("Available", fmt::bytes(mem.available))];
                if let Some(c) = mem.cached {
                    kv.push(("Cached", fmt::bytes(c)));
                }
                if let Some(c) = mem.compressed {
                    kv.push(("Compressed", fmt::bytes(c)));
                }
                self.kv(&kv);
                self.row_kv(
                    "Committed",
                    &format!("{} / {}", fmt::bytes(mem.commit_used), fmt::bytes(mem.commit_limit)),
                    t.text,
                );
            }
            Module::Gpu => {
                if snap.gpus.is_empty() {
                    return self.missing("GPU");
                }
                let top = snap.gpus.iter().map(|g| g.util_pct).fold(0.0, f32::max);
                self.header("GPU", &fmt::pct(top), t.level(t.gpu, top));
                for (i, g) in snap.gpus.iter().enumerate() {
                    if i > 0 {
                        self.gap(6.0);
                    }
                    let r = Rect::new(self.x, self.y, self.w, 18.0);
                    self.text(&g.name, &self.f.bold, Rect { w: r.w - 50.0, ..r }, t.text, Align::Left);
                    self.text(&fmt::pct(g.util_pct), &self.f.bold, r, t.level(t.gpu, g.util_pct), Align::Right);
                    self.y += 22.0;
                    if let Some(s) = hist.gpus.get(i) {
                        self.graph(&[(s, t.gpu)], 100.0, 40.0, pct, None);
                    }
                    if g.vram_total > 0 {
                        self.row_kv(
                            "Dedicated memory",
                            &format!("{} / {}", fmt::bytes(g.vram_used), fmt::bytes(g.vram_total)),
                            t.text,
                        );
                        self.bar(g.vram_used as f32 / g.vram_total as f32, t.gpu);
                    }
                    let mut engines: Vec<_> = g.engines.iter().collect();
                    engines.sort_by(|a, b| b.1.total_cmp(&a.1));
                    let mut kv: Vec<(&str, String)> =
                        engines.iter().take(4).map(|(n, v)| (n.as_str(), fmt::pct(*v))).collect();
                    if g.shared_used > 0 {
                        kv.push(("Shared", fmt::bytes(g.shared_used)));
                    }
                    if let Some(v) = g.temp_c {
                        kv.push(("Temperature", fmt::temp(v, unit)));
                    }
                    if let Some(v) = g.hotspot_c {
                        kv.push(("Hot spot", fmt::temp(v, unit)));
                    }
                    match (g.fan_rpm, g.fan_pct) {
                        (Some(r), _) => kv.push(("Fan", format!("{r} rpm"))),
                        (None, Some(p)) => kv.push(("Fan", fmt::pct(p))),
                        _ => {}
                    }
                    if let Some(v) = g.power_w {
                        kv.push(("Power", fmt::watts(v)));
                    }
                    if let Some(v) = g.core_clock_mhz {
                        kv.push(("Core clock", fmt::mhz(v)));
                    }
                    if let Some(v) = g.mem_clock_mhz {
                        kv.push(("Mem clock", fmt::mhz(v)));
                    }
                    self.kv(&kv);
                }
            }
            Module::Network => {
                let Some(n) = &snap.net else { return self.missing("Network") };
                self.header("Network", "", t.text);
                self.rates(("↓", n.rx_bps, t.rx), ("↑", n.tx_bps, t.tx));
                let max = nice_max(hist.net_rx.max().max(hist.net_tx.max()));
                self.graph(&[(&hist.net_rx, t.rx), (&hist.net_tx, t.tx)], max, 56.0, rate, Some(fmt::rate(max as f64)));
                self.kv(&[("Received", fmt::bytes(n.rx_total)), ("Sent", fmt::bytes(n.tx_total))]);
                for i in n.interfaces.iter().filter(|i| i.connected) {
                    self.gap(4.0);
                    self.row(&i.name, &format!("↓ {}   ↑ {}", fmt::rate(i.rx_bps), fmt::rate(i.tx_bps)), t.secondary);
                    let mut info = i.ipv4.join(", ");
                    if i.link_speed_bps > 0 {
                        let speed = match i.link_speed_bps {
                            s if s >= 1_000_000_000 => format!("{} Gbps", s as f64 / 1e9),
                            s => format!("{} Mbps", s / 1_000_000),
                        };
                        info = if info.is_empty() { speed } else { format!("{info}  ·  {speed}") };
                    }
                    if !info.is_empty() {
                        self.sub(&info);
                    }
                }
            }
            Module::Disk => {
                if snap.disks.is_empty() && snap.volumes.is_empty() {
                    return self.missing("Disk");
                }
                let (r, w) = (
                    snap.disks.iter().map(|d| d.read_bps).sum::<f64>(),
                    snap.disks.iter().map(|d| d.write_bps).sum::<f64>(),
                );
                self.header("Disk", "", t.text);
                self.rates(("R", r, t.rx), ("W", w, t.tx));
                let max = nice_max(hist.disk_r.max().max(hist.disk_w.max()));
                self.graph(&[(&hist.disk_r, t.rx), (&hist.disk_w, t.tx)], max, 48.0, rate, Some(fmt::rate(max as f64)));
                for d in &snap.disks {
                    self.row(
                        &d.name,
                        &format!("R {}   W {}", fmt::rate(d.read_bps), fmt::rate(d.write_bps)),
                        t.secondary,
                    );
                    self.meter("Active", d.active_pct, t.accent);
                }
                if !snap.volumes.is_empty() {
                    self.gap(4.0);
                }
                for v in &snap.volumes {
                    let used = v.total.saturating_sub(v.free);
                    let name = if v.label.is_empty() { v.mount.clone() } else { format!("{} {}", v.mount, v.label) };
                    self.row(&name, &format!("{} free of {}", fmt::bytes(v.free), fmt::bytes(v.total)), t.secondary);
                    let frac = if v.total > 0 { used as f32 / v.total as f32 } else { 0.0 };
                    self.bar(frac, t.level(t.accent, frac * 100.0));
                }
            }
            Module::Sensors => {
                let pinned = select::pinned_sensor(snap, ctx.cfg);
                let v = pinned.map(|s| fmt::sensor(s.value, s.kind, unit)).unwrap_or_default();
                let vc = pinned.filter(|s| s.kind == SensorKind::Temperature).map_or(t.text, |s| t.temp(s.value));
                self.header("Sensors", &v, vc);
                if snap.sensors.is_empty() {
                    self.hint("No sensors available. Run LibreHardwareMonitor or HWiNFO (with shared memory enabled) for CPU temperatures.");
                    return;
                }
                let mut groups: Vec<&str> = Vec::new();
                for s in &snap.sensors {
                    if !groups.contains(&s.hardware.as_str()) {
                        groups.push(&s.hardware);
                    }
                }
                for g in groups {
                    self.group(g);
                    for s in snap.sensors.iter().filter(|s| s.hardware == g) {
                        let c = if s.kind == SensorKind::Temperature { t.temp(s.value) } else { t.text };
                        self.row_kv(&s.name, &fmt::sensor(s.value, s.kind, unit), c);
                    }
                }
            }
            Module::Battery => {
                let Some(b) = &snap.battery else { return self.missing("Battery") };
                let c = if b.percent < 20.0 && !b.charging { t.crit } else { t.battery };
                self.header("Battery", &format!("{}{}", fmt::pct(b.percent), if b.charging { " ⚡" } else { "" }), c);
                self.bar(b.percent / 100.0, c);
                let state = match (b.charging, b.ac_online) {
                    (true, _) => "Charging",
                    (false, true) => "Plugged in",
                    _ => "On battery",
                };
                let mut kv = vec![("State", state.to_string())];
                if let Some(s) = b.secs_remaining {
                    kv.push(("Time left", fmt::duration(s as u64)));
                }
                if let Some(r) = b.rate_mw {
                    kv.push(("Rate", format!("{}{}", if r > 0 { "+" } else { "" }, fmt::watts(r as f32 / 1000.0))));
                }
                if let (Some(full), Some(design)) = (b.full_capacity_mwh, b.design_capacity_mwh.filter(|d| *d > 0)) {
                    kv.push(("Health", fmt::pct(full as f32 * 100.0 / design as f32)));
                    kv.push(("Capacity", format!("{:.1} / {:.1} Wh", full as f32 / 1000.0, design as f32 / 1000.0)));
                }
                if let Some(c) = b.cycle_count {
                    kv.push(("Cycles", c.to_string()));
                }
                self.kv(&kv);
            }
            Module::Processes => {
                self.header("Processes", "", t.text);
                self.tabs(&TABS);
                let top = &snap.top;
                let (list, val): (&[ProcEntry], fn(&ProcEntry) -> String) = match self.tab {
                    0 => (&top.by_cpu, |p| fmt::pct(p.cpu_pct)),
                    1 => (&top.by_mem, |p| fmt::bytes(p.mem_bytes)),
                    2 => (&top.by_disk, |p| fmt::rate(p.io_bps)),
                    _ => (&top.by_gpu, |p| fmt::pct(p.gpu_pct)),
                };
                if list.is_empty() {
                    self.sub("No data");
                }
                for p in list.iter().take(busy_core::TOP_N) {
                    self.row(&p.name, &val(p), t.text);
                }
            }
        }
    }

    fn meter(&mut self, label: &str, p: f32, c: Color) {
        let r = Rect::new(self.x, self.y, self.w, 18.0);
        let t = *self.t();
        self.text(label, &self.f.small, Rect { w: 60.0, ..r }, t.secondary, Align::Left);
        self.text(&fmt::pct(p), &self.f.small, r, t.secondary, Align::Right);
        if let Some(cv) = self.cv {
            cv.hbar(Rect::new(self.x + 60.0, self.y + 6.5, self.w - 100.0, 5.0), p / 100.0, c, t.track);
        }
        self.y += 20.0;
    }

    fn row_kv(&mut self, left: &str, right: &str, rc: Color) {
        let r = Rect::new(self.x, self.y, self.w, 19.0);
        self.text(left, &self.f.body, Rect { w: r.w - 80.0, ..r }, self.t().secondary, Align::Left);
        self.text(right, &self.f.body, r, rc, Align::Right);
        self.y += 19.0;
    }

    /// Two colored rate readouts side by side (download/upload, read/write).
    fn rates(&mut self, a: (&str, f64, Color), b: (&str, f64, Color)) {
        let half = self.w / 2.0;
        for (i, (p, v, c)) in [a, b].into_iter().enumerate() {
            let x = self.x + i as f32 * half;
            self.text(p, &self.f.bold, Rect::new(x, self.y, 14.0, 20.0), c, Align::Left);
            self.text(
                &fmt::rate(v),
                &self.f.bold,
                Rect::new(x + 14.0, self.y, half - 14.0, 20.0),
                self.t().text,
                Align::Left,
            );
        }
        self.y += 24.0;
    }

    fn missing(&mut self, title: &str) {
        self.header(title, "", self.t().text);
        self.sub("Waiting for data…");
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
