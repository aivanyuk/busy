//! Application state, config handling and message routing (UI thread only).
//!
//! The widget's input queue is attached to explorer's taskbar thread, so nothing here may block:
//! sampling runs on the sampler thread and config saves on the config writer thread.

use crate::ctx::Ctx;
use crate::flyout::Flyout;
use crate::history::{History, capacity};
use crate::render::Gfx;
use crate::sampler::{Params, Sampler};
use crate::select::pinned_sensor;
use crate::sync::lock;
use crate::taskbar::Taskbar;
use crate::theme::Theme;
use crate::win::{self, Event, register_class};
use crate::worker::Worker;
use busy_core::{Anchor, Config, Module, Snapshot, ThemeMode};
use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::System::Power::{
    HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification, UnregisterPowerSettingNotification,
};
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

const WM_APP_SNAPSHOT: u32 = WM_APP + 1;
const WM_APP_RENDER: u32 = WM_APP + 2;
const WM_APP_CONFIG: u32 = WM_APP + 3;
const WM_APP_FLYOUT_DEACTIVATED: u32 = WM_APP + 4;
const WM_APP_THEME: u32 = WM_APP + 5;

const TIMER_WATCH: usize = 1;
const ID_SETTINGS: u32 = 1;
const ID_EXIT: u32 = 2;
const ID_ANCHOR_TRAY: u32 = 3;
const ID_ANCHOR_LEFT: u32 = 4;
const ID_TOGGLE: u32 = 100;

static MAIN: AtomicIsize = AtomicIsize::new(0);
/// `RegisterPowerSettingNotification` handle for the display state, unregistered with the main window.
static DISPLAY_NOTIFY: AtomicIsize = AtomicIsize::new(0);
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
    /// The session is locked (`WTS_SESSION_LOCK`).
    locked: bool,
    /// The console display is off (`GUID_CONSOLE_DISPLAY_STATE` = 0).
    display_off: bool,
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
        // Lock/unlock and display on/off pause the sampler. Registration sends the current display state at once.
        let _ = WTSRegisterSessionNotification(main, NOTIFY_FOR_THIS_SESSION);
        if let Ok(h) =
            RegisterPowerSettingNotification(HANDLE(main.0), &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE)
        {
            DISPLAY_NOTIFY.store(h.0, Ordering::Relaxed);
        }
    }
    let theme = Theme::resolve(cfg.theme);
    let flyout = Flyout::create(&gfx, main, &theme);
    let app = App {
        main,
        sampler: Sampler::start(Params::new(&cfg, false, false), main, WM_APP_SNAPSHOT),
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
        locked: false,
        display_off: false,
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
            WM_DWMCOLORIZATIONCOLORCHANGED => {
                with(App::refresh_theme);
            }
            WM_DISPLAYCHANGE => {
                with(App::render_all);
            }
            WM_WTSSESSION_CHANGE if matches!(wp.0 as u32, WTS_SESSION_LOCK | WTS_SESSION_UNLOCK) => {
                let locked = wp.0 as u32 == WTS_SESSION_LOCK;
                with(|a| {
                    a.locked = locked;
                    a.sync_sampler();
                });
            }
            WM_POWERBROADCAST => {
                if wp.0 as u32 == PBT_POWERSETTINGCHANGE
                    && let Some(on) = display_state(lp)
                {
                    with(|a| {
                        a.display_off = !on;
                        a.sync_sampler();
                    });
                }
                return LRESULT(1);
            }
            WM_ENDSESSION if wp.0 != 0 => {
                APP.with(|a| a.try_borrow_mut().map(|mut a| a.take()).ok());
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
            }
            WM_DESTROY => {
                let _ = WTSUnRegisterSessionNotification(hwnd);
                let notify = DISPLAY_NOTIFY.swap(0, Ordering::Relaxed);
                if notify != 0 {
                    let _ = UnregisterPowerSettingNotification(HPOWERNOTIFY(notify));
                }
                PostQuitMessage(0);
            }
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
        let sensor = pinned_sensor(&snap, &self.cfg).map(|s| s.value);
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
    }

    fn hide_flyout(&mut self) {
        if let Some(f) = &mut self.flyout {
            f.hide(false);
        }
        self.sync_sampler();
    }

    /// Tells the sampler what is visible now: flyout-only modules only while the flyout is open, and nothing
    /// while the session is locked or the display is off.
    fn sync_sampler(&self) {
        let flyout_open = self.flyout.as_ref().is_some_and(Flyout::is_open);
        self.sampler.set_params(Params::new(&self.cfg, flyout_open, self.locked || self.display_off));
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

    fn on_command(&mut self, id: u32) {
        let mut cfg = self.cfg.clone();
        match id {
            ID_SETTINGS => return self.open_settings(),
            ID_EXIT => return post_close(),
            ID_ANCHOR_TRAY => cfg.anchor = Anchor::NearTray,
            ID_ANCHOR_LEFT => cfg.anchor = Anchor::Left,
            id if (ID_TOGGLE..ID_TOGGLE + Module::ALL.len() as u32).contains(&id) => {
                let m = Module::ALL[(id - ID_TOGGLE) as usize];
                if let Some(mc) = cfg.modules.iter_mut().find(|c| c.module == m) {
                    mc.taskbar = !mc.taskbar;
                }
            }
            _ => return,
        }
        self.apply_config(cfg, true);
    }
}

/// Whether a `PBT_POWERSETTINGCHANGE` for `GUID_CONSOLE_DISPLAY_STATE` says the display is on (dimmed counts
/// as on); `None` for any other setting.
fn display_state(lp: LPARAM) -> Option<bool> {
    let p = lp.0 as *const POWERBROADCAST_SETTING;
    if p.is_null() {
        return None;
    }
    // SAFETY: for PBT_POWERSETTINGCHANGE the system passes a POWERBROADCAST_SETTING valid for this message,
    // with `DataLength` bytes of data from `Data`; the length is checked before the unaligned read.
    unsafe {
        let s = &*p;
        if s.PowerSetting != GUID_CONSOLE_DISPLAY_STATE || (s.DataLength as usize) < size_of::<u32>() {
            return None;
        }
        Some(std::ptr::addr_of!(s.Data).cast::<u32>().read_unaligned() != 0)
    }
}

fn post_close() {
    unsafe {
        let _ = PostMessageW(Some(main_hwnd()), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

/// Shows the widget's context menu. Runs outside the app borrow: TrackPopupMenu spins a modal loop.
fn context_menu() {
    let Some(cfg) = with(|a| {
        a.hide_flyout();
        a.cfg.clone()
    }) else {
        return;
    };
    let main = main_hwnd();
    unsafe {
        let (Ok(menu), Ok(sub_mods), Ok(sub_pos)) = (CreatePopupMenu(), CreatePopupMenu(), CreatePopupMenu()) else {
            return;
        };
        let checked = |b: bool| if b { MF_CHECKED } else { MF_UNCHECKED };
        for mc in &cfg.modules {
            let idx = Module::ALL.iter().position(|m| *m == mc.module).unwrap_or(0) as u32;
            let label = busy_win::wide(mc.module.label());
            let _ = AppendMenuW(
                sub_mods,
                MF_STRING | checked(mc.taskbar),
                (ID_TOGGLE + idx) as usize,
                PCWSTR(label.as_ptr()),
            );
        }
        let _ = AppendMenuW(
            sub_pos,
            MF_STRING | checked(cfg.anchor == Anchor::NearTray),
            ID_ANCHOR_TRAY as usize,
            w!("Next to notification area"),
        );
        let _ = AppendMenuW(
            sub_pos,
            MF_STRING | checked(cfg.anchor == Anchor::Left),
            ID_ANCHOR_LEFT as usize,
            w!("Left edge"),
        );
        let _ = AppendMenuW(menu, MF_STRING, ID_SETTINGS as usize, w!("Settings…"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_POPUP, sub_mods.0 as usize, w!("Show on taskbar"));
        let _ = AppendMenuW(menu, MF_POPUP, sub_pos.0 as usize, w!("Position"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, ID_EXIT as usize, w!("Exit"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu closes when clicking elsewhere.
        let _ = SetForegroundWindow(main);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
            pt.x,
            pt.y,
            None,
            main,
            None,
        );
        let _ = PostMessageW(Some(main), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        if cmd.0 > 0 {
            with(|a| a.on_command(cmd.0 as u32));
        }
    }
}
