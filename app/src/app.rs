//! Application state, config handling and message routing (UI thread only).
//!
//! The widget's input queue is attached to explorer's taskbar thread, so nothing here may block:
//! sampling runs on the sampler thread and config saves on the config writer thread.

use crate::ctx::Ctx;
use crate::flyout::Flyout;
use crate::history::{History, capacity};
use crate::menu::{self, Command};
use crate::render::Gfx;
use crate::sampler::{Params, Sampler};
use crate::select::taskbar_sensor;
use crate::sync::lock;
use crate::taskbar::Taskbar;
use crate::theme::Theme;
use crate::win::{self, Event, register_class};
use crate::worker::Worker;
use busy_core::{Config, Snapshot, ThemeMode};
use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

const WM_APP_SNAPSHOT: u32 = WM_APP + 1;
const WM_APP_RENDER: u32 = WM_APP + 2;
const WM_APP_CONFIG: u32 = WM_APP + 3;
const WM_APP_FLYOUT_DEACTIVATED: u32 = WM_APP + 4;
const WM_APP_THEME: u32 = WM_APP + 5;

const TIMER_WATCH: usize = 1;

static MAIN: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// Config handed over from another thread / the settings window, applied on WM_APP_CONFIG.
static PENDING: Mutex<Option<Config>> = Mutex::new(None);
/// Theme resolved by the theme reader, applied on WM_APP_THEME.
static RESOLVED: Mutex<Option<Theme>> = Mutex::new(None);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

struct App {
    main: HWND,
    cfg: Config,
    sampler: Sampler,
    writer: Worker<Config>,
    /// Reads the theme's registry values off the UI thread.
    theme_reader: Worker<ThemeMode>,
    hist: History,
    snap: Snapshot,
    theme: Theme,
    gfx: Gfx,
    taskbar: Option<Taskbar>,
    flyout: Option<Flyout>,
    open_flyout: bool,
}

/// Runs `f` on the app state unless it is already borrowed (re-entrant message); then returns None.
fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

fn main_hwnd() -> HWND {
    HWND(MAIN.load(Ordering::Relaxed) as _)
}

fn post(msg: u32) {
    unsafe {
        let _ = PostMessageW(Some(main_hwnd()), msg, WPARAM(0), LPARAM(0));
    }
}

/// Hands a new config to the UI thread (safe from any thread).
pub fn submit_config(cfg: Config) {
    *lock(&PENDING) = Some(cfg);
    post(WM_APP_CONFIG);
}

pub fn run(open_flyout: bool) -> Result<()> {
    let cfg = Config::load();
    let gfx = Gfx::new()?;
    win::set_handler(on_event);
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
            Some(win::hinstance()),
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
    let flyout = Flyout::create(&gfx, main, &theme);
    let app = App {
        main,
        sampler: Sampler::start(Params::new(&cfg, false), main, WM_APP_SNAPSHOT),
        writer: Worker::start("busy-config", |cfg: Config| {
            let _ = cfg.save();
        }),
        theme_reader: Worker::start("busy-theme", |mode| {
            *lock(&RESOLVED) = Some(Theme::resolve(mode));
            post(WM_APP_THEME);
        }),
        hist: History::new(capacity(cfg.history_secs, cfg.interval_ms)),
        snap: Snapshot::default(),
        theme,
        taskbar: Taskbar::create(&gfx),
        flyout,
        gfx,
        cfg,
        open_flyout,
    };
    APP.with(|a| *a.borrow_mut() = Some(app));

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            // Keyboard navigation (Tab, Enter, Esc) for the modeless settings window.
            if busy_settings::is_dialog_message(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    // Drops the widget (destroying our child of explorer's taskbar) and joins the sampler.
    APP.with(|a| a.borrow_mut().take());
    Ok(())
}

/// Input from our windows (see `win::Event`).
fn on_event(ev: Event) {
    match ev {
        Event::WidgetHover(h) => {
            with(|a| a.set_hover(h));
        }
        Event::WidgetClick => {
            with(App::toggle_flyout);
        }
        Event::WidgetMenu => context_menu(),
        Event::WidgetRerender => post(WM_APP_RENDER),
        // Posted: WM_ACTIVATE may arrive re-entrantly while the app state is borrowed.
        Event::FlyoutDeactivated => post(WM_APP_FLYOUT_DEACTIVATED),
        Event::FlyoutEscape => {
            with(App::hide_flyout);
        }
        Event::FlyoutWheel(delta) => {
            with(|a| a.flyout_event(|f, ctx| f.on_wheel(ctx, delta)));
        }
        Event::FlyoutPointer(p) => {
            with(|a| a.flyout_event(|f, ctx| f.on_mouse(ctx, p)));
        }
        Event::FlyoutClick(x, y) => {
            with(|a| a.flyout_event(|f, ctx| f.on_click(ctx, x, y)));
        }
        Event::FlyoutPaint => {
            with(|a| a.flyout_event(Flyout::render));
        }
    }
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
            WM_APP_CONFIG => {
                if let Some(cfg) = lock(&PENDING).take() {
                    with(|a| a.apply_config(cfg, true));
                }
            }
            WM_APP_THEME => {
                if let Some(t) = lock(&RESOLVED).take() {
                    with(|a| a.set_theme(t));
                }
            }
            WM_APP_FLYOUT_DEACTIVATED => {
                with(|a| {
                    if let Some(f) = &mut a.flyout
                        && !f.is_foreground()
                        // Debug builds: `BUSY_PIN_FLYOUT=1` keeps it open for screenshots.
                        && !(cfg!(debug_assertions) && std::env::var_os("BUSY_PIN_FLYOUT").is_some())
                    {
                        f.hide(true);
                    }
                    a.sync_sampler();
                    a.sync_active();
                });
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

impl Drop for App {
    /// Windows go first: until our child of explorer's taskbar is gone, the sampler join below
    /// (the `sampler` field's drop, possibly waiting out a WMI call) would freeze the user's taskbar.
    fn drop(&mut self) {
        self.taskbar = None;
        self.flyout = None;
    }
}

impl App {
    fn on_snapshot(&mut self) {
        let Some(snap) = self.sampler.take() else { return };
        let sensor = taskbar_sensor(&snap, &self.cfg).map(|s| s.value);
        self.hist.push(&snap, sensor);
        self.snap = snap;
        self.render_all();
        if std::mem::take(&mut self.open_flyout) {
            self.toggle_flyout();
        }
    }

    fn render_all(&mut self) {
        let ctx = Ctx { cfg: &self.cfg, snap: &self.snap, hist: &self.hist, theme: &self.theme, gfx: &self.gfx };
        if let Some(tb) = &mut self.taskbar {
            tb.render(&ctx);
        }
        if let Some(f) = &mut self.flyout {
            f.render(&ctx);
        }
    }

    /// Periodic health check: re-embed after explorer restarts, follow tray width/taskbar changes.
    fn watch(&mut self) {
        if !self.taskbar.as_ref().is_some_and(Taskbar::is_alive) {
            self.recreate_taskbar();
        } else if let Some(tb) = &mut self.taskbar {
            let ctx = Ctx { cfg: &self.cfg, snap: &self.snap, hist: &self.hist, theme: &self.theme, gfx: &self.gfx };
            tb.watch(&ctx);
        }
    }

    fn recreate_taskbar(&mut self) {
        self.taskbar = None;
        self.taskbar = Taskbar::create(&self.gfx);
        self.render_all();
        self.sync_active();
    }

    /// Re-reads the theme on the theme reader; `set_theme` applies the result.
    fn refresh_theme(&mut self) {
        self.theme_reader.submit(self.cfg.theme);
    }

    fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        if let Some(f) = &mut self.flyout {
            f.apply_theme(&self.theme);
        }
        self.render_all();
    }

    fn set_hover(&mut self, hover: bool) {
        if let Some(tb) = &mut self.taskbar
            && tb.set_hover(hover)
        {
            let ctx = Ctx { cfg: &self.cfg, snap: &self.snap, hist: &self.hist, theme: &self.theme, gfx: &self.gfx };
            tb.render(&ctx);
        }
    }

    fn toggle_flyout(&mut self) {
        let (Some(f), Some(tb)) = (&mut self.flyout, &self.taskbar) else { return };
        let ctx = Ctx { cfg: &self.cfg, snap: &self.snap, hist: &self.hist, theme: &self.theme, gfx: &self.gfx };
        f.toggle(&ctx, tb.screen_rect(), tb.dpi());
        self.sync_sampler();
        self.sync_active();
    }

    fn hide_flyout(&mut self) {
        if let Some(f) = &mut self.flyout {
            f.hide(false);
        }
        self.sync_sampler();
        self.sync_active();
    }

    /// Tells the sampler what is visible now, so it samples flyout-only modules only while the flyout is open.
    fn sync_sampler(&self) {
        let flyout_open = self.flyout.as_ref().is_some_and(Flyout::is_visible);
        self.sampler.set_params(Params::new(&self.cfg, flyout_open));
    }

    /// The widget shows `--active` while its flyout is open (design: the open module's cell); redrawn only when
    /// that changes.
    fn sync_active(&mut self) {
        let open = self.flyout.as_ref().is_some_and(Flyout::is_visible);
        if let Some(tb) = &mut self.taskbar
            && tb.set_active(open)
        {
            let ctx = Ctx { cfg: &self.cfg, snap: &self.snap, hist: &self.hist, theme: &self.theme, gfx: &self.gfx };
            tb.render(&ctx);
        }
    }

    fn flyout_event(&mut self, ev: impl FnOnce(&mut Flyout, &Ctx)) {
        let ctx = Ctx { cfg: &self.cfg, snap: &self.snap, hist: &self.hist, theme: &self.theme, gfx: &self.gfx };
        if let Some(f) = &mut self.flyout {
            ev(f, &ctx);
        }
    }

    /// Applies a new config live; `save` queues it to the config writer.
    fn apply_config(&mut self, mut cfg: Config, save: bool) {
        cfg.normalize();
        if cfg == self.cfg {
            return;
        }
        let old = std::mem::replace(&mut self.cfg, cfg);
        self.sync_sampler();
        let cap = capacity(self.cfg.history_secs, self.cfg.interval_ms);
        if cap != capacity(old.history_secs, old.interval_ms) {
            self.hist.set_capacity(cap);
        }
        if old.theme != self.cfg.theme {
            self.refresh_theme();
        }
        self.render_all();
        if save {
            self.writer.submit(self.cfg.clone());
        }
    }

    fn open_settings(&mut self) {
        busy_settings::open(self.main, &self.cfg, Box::new(submit_config));
    }

    fn on_command(&mut self, cmd: Command) {
        let mut cfg = self.cfg.clone();
        match cmd {
            Command::Settings => return self.open_settings(),
            Command::Exit => return post_close(),
            Command::Anchor(anchor) => cfg.anchor = anchor,
            Command::Toggle(m) => {
                if let Some(mc) = cfg.modules.iter_mut().find(|c| c.module == m) {
                    mc.taskbar = !mc.taskbar;
                }
            }
        }
        self.apply_config(cfg, true);
    }
}

fn post_close() {
    unsafe {
        let _ = PostMessageW(Some(main_hwnd()), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

/// Shows the widget's context menu. Runs outside the app borrow: `menu::show` spins a modal loop.
fn context_menu() {
    let Some(cfg) = with(|a| {
        a.hide_flyout();
        a.cfg.clone()
    }) else {
        return;
    };
    if let Some(cmd) = menu::show(main_hwnd(), &cfg) {
        with(|a| a.on_command(cmd));
    }
}
