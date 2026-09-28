//! Application state and message routing (UI thread only).
//!
//! The widget's input queue is attached to explorer's taskbar thread, so nothing here may block:
//! sampling runs on the sampler thread and config file IO on short-lived worker threads.

use crate::history::{History, capacity};
use crate::render::Gfx;
use crate::sampler::Sampler;
use crate::taskbar::Taskbar;
use crate::theme::Theme;
use busy_core::{Config, SensorKind, SensorReading, Snapshot};
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

pub const WM_APP_SNAPSHOT: u32 = WM_APP + 1;
pub const WM_APP_RENDER: u32 = WM_APP + 2;

const TIMER_WATCH: usize = 1;

static MAIN: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Read-only view passed to renderers.
pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub snap: &'a Snapshot,
    pub hist: &'a History,
    pub theme: &'a Theme,
    pub gfx: &'a Gfx,
}

pub struct App {
    cfg: Config,
    sampler: Sampler,
    hist: History,
    snap: Snapshot,
    theme: Theme,
    gfx: Gfx,
    taskbar: Option<Taskbar>,
}

/// Runs `f` on the app state unless it is already borrowed (re-entrant message); then returns None.
pub fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

pub fn main_hwnd() -> HWND {
    HWND(MAIN.load(Ordering::Relaxed) as _)
}

pub fn post(msg: u32) {
    unsafe {
        let _ = PostMessageW(Some(main_hwnd()), msg, WPARAM(0), LPARAM(0));
    }
}

pub fn hinstance() -> HINSTANCE {
    unsafe { GetModuleHandleW(None).map(Into::into).unwrap_or_default() }
}

pub fn register_class(name: PCWSTR, proc: WNDPROC) {
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: proc,
        hInstance: hinstance(),
        lpszClassName: name,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        ..Default::default()
    };
    // Fails harmlessly with ERROR_CLASS_ALREADY_EXISTS when re-creating windows.
    unsafe { RegisterClassExW(&wc) };
}

/// The user's pinned sensor ("hardware/name"), else the hottest CPU temperature, else the hottest temperature.
pub fn pinned_sensor<'a>(snap: &'a Snapshot, cfg: &Config) -> Option<&'a SensorReading> {
    if !cfg.pinned_sensor.is_empty()
        && let Some(s) = snap
            .sensors
            .iter()
            .find(|s| cfg.pinned_sensor.split_once('/') == Some((s.hardware.as_str(), s.name.as_str())))
    {
        return Some(s);
    }
    let temps = || snap.sensors.iter().filter(|s| s.kind == SensorKind::Temperature);
    let is_cpu = |s: &&SensorReading| {
        let (h, n) = (s.hardware.to_lowercase(), s.name.to_lowercase());
        ["cpu", "ryzen", "intel", "core i", "processor"].iter().any(|k| h.contains(k))
            || ["cpu", "package", "tctl", "tdie"].iter().any(|k| n.contains(k))
    };
    let hottest = |a: &&SensorReading, b: &&SensorReading| a.value.total_cmp(&b.value);
    temps().filter(is_cpu).max_by(hottest).or_else(|| temps().max_by(hottest))
}

pub fn run() -> Result<()> {
    let cfg = Config::load();
    let gfx = Gfx::new()?;
    register_class(w!("busy.main"), Some(main_proc));
    // A hidden top-level window (not HWND_MESSAGE) so broadcasts like TaskbarCreated reach us.
    let main = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("busy.main"),
            w!("busy"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance()),
            None,
        )?
    };
    MAIN.store(main.0 as isize, Ordering::Relaxed);
    unsafe {
        let msg = RegisterWindowMessageW(w!("TaskbarCreated"));
        TASKBAR_CREATED.store(msg, Ordering::Relaxed);
        // Lets the broadcast through UIPI if we happen to run elevated.
        let _ = ChangeWindowMessageFilterEx(main, msg, MSGFLT_ALLOW, None);
        SetTimer(Some(main), TIMER_WATCH, 1000, None);
    }
    let theme = Theme::resolve(cfg.theme);
    let app = App {
        sampler: Sampler::start(cfg.clone(), main, WM_APP_SNAPSHOT),
        hist: History::new(capacity(cfg.history_secs, cfg.interval_ms)),
        snap: Snapshot::default(),
        theme,
        taskbar: Taskbar::create(&gfx),
        gfx,
        cfg,
    };
    APP.with(|a| *a.borrow_mut() = Some(app));

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    // Drops the widget (destroying our child of explorer's taskbar) and joins the sampler.
    APP.with(|a| a.borrow_mut().take());
    Ok(())
}

extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SNAPSHOT => {
                with(App::on_snapshot);
            }
            WM_APP_RENDER => {
                with(App::render_all);
            }
            WM_TIMER => {
                with(App::watch);
            }
            WM_SETTINGCHANGE => {
                let area = PCWSTR(lp.0 as *const u16);
                if !area.is_null() && area.to_string().is_ok_and(|s| s == "ImmersiveColorSet") {
                    with(App::refresh_theme);
                } else {
                    with(App::render_all);
                }
            }
            WM_DWMCOLORIZATIONCOLORCHANGED => {
                with(App::refresh_theme);
            }
            WM_DISPLAYCHANGE => {
                with(App::render_all);
            }
            WM_ENDSESSION if wp.0 != 0 => {
                APP.with(|a| a.try_borrow_mut().map(|mut a| a.take()).ok());
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
            }
            WM_DESTROY => PostQuitMessage(0),
            m if m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
                with(App::recreate_taskbar);
            }
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
    LRESULT(0)
}

impl App {
    fn on_snapshot(&mut self) {
        let Some(snap) = self.sampler.take() else { return };
        let sensor = pinned_sensor(&snap, &self.cfg).map(|s| s.value);
        self.hist.push(&snap, sensor);
        self.snap = snap;
        self.render_all();
    }

    fn render_all(&mut self) {
        let ctx = Ctx { cfg: &self.cfg, snap: &self.snap, hist: &self.hist, theme: &self.theme, gfx: &self.gfx };
        if let Some(tb) = &mut self.taskbar {
            tb.render(&ctx);
        }
    }

    /// Periodic health check: re-embed after explorer restarts, follow tray width/taskbar changes.
    fn watch(&mut self) {
        if !self.taskbar.as_ref().is_some_and(Taskbar::is_alive) {
            self.recreate_taskbar();
        } else if let Some(tb) = &mut self.taskbar {
            let ctx = Ctx { cfg: &self.cfg, snap: &self.snap, hist: &self.hist, theme: &self.theme, gfx: &self.gfx };
            tb.render(&ctx);
        }
    }

    fn recreate_taskbar(&mut self) {
        self.taskbar = None;
        self.taskbar = Taskbar::create(&self.gfx);
        self.render_all();
    }

    fn refresh_theme(&mut self) {
        self.theme = Theme::resolve(self.cfg.theme);
        self.render_all();
    }

    pub fn set_hover(&mut self, hover: bool) {
        if let Some(tb) = self.taskbar.as_mut().filter(|t| t.hover != hover) {
            tb.hover = hover;
            let ctx = Ctx { cfg: &self.cfg, snap: &self.snap, hist: &self.hist, theme: &self.theme, gfx: &self.gfx };
            tb.render(&ctx);
        }
    }
}
