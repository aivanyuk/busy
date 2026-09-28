//! Application state and message routing (UI thread only).
//!
//! The widget's input queue is attached to explorer's taskbar thread, so nothing here may block:
//! sampling runs on the sampler thread and config file IO on short-lived worker threads.

use crate::history::{History, capacity};
use crate::sampler::Sampler;
use busy_core::{Config, SensorKind, SensorReading, Snapshot};
use std::cell::RefCell;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

pub const WM_APP_SNAPSHOT: u32 = WM_APP + 1;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub struct App {
    cfg: Config,
    sampler: Sampler,
    hist: History,
}

/// Runs `f` on the app state unless it is already borrowed (re-entrant message); then returns None.
pub fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
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
    let app = App {
        sampler: Sampler::start(cfg.clone(), main, WM_APP_SNAPSHOT),
        hist: History::new(capacity(cfg.history_secs, cfg.interval_ms)),
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
    // Drops the state, which joins the sampler.
    APP.with(|a| a.borrow_mut().take());
    Ok(())
}

extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_APP_SNAPSHOT => {
                with(App::on_snapshot);
            }
            WM_ENDSESSION if wp.0 != 0 => {
                APP.with(|a| a.try_borrow_mut().map(|mut a| a.take()).ok());
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
            }
            WM_DESTROY => PostQuitMessage(0),
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
    }
}
