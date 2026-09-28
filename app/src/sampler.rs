//! Background sampling thread. Sources are created and driven only on this thread.

use busy_core::{Config, Snapshot, Source};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// The single place where collectors are registered.
fn build_sources() -> Vec<Box<dyn Source>> {
    // Synthetic data until the real collectors are wired in.
    crate::fake::sources()
}

struct State {
    cfg: Config,
    latest: Option<Snapshot>,
    stop: bool,
    /// Bumped on config change so the waiting loop re-evaluates its deadline.
    generation: u64,
}

struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

pub struct Sampler {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Sampler {
    /// Posts `msg` to `notify` whenever a new snapshot is ready.
    pub fn start(cfg: Config, notify: HWND, msg: u32) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State { cfg, latest: None, stop: false, generation: 0 }),
            cv: Condvar::new(),
        });
        let hwnd = notify.0 as isize;
        let sh = shared.clone();
        let thread = std::thread::Builder::new().name("busy-sampler".into()).spawn(move || run(sh, hwnd, msg)).ok();
        Self { shared, thread }
    }

    pub fn set_config(&self, cfg: &Config) {
        let mut st = lock(&self.shared.state);
        st.cfg = cfg.clone();
        st.generation += 1;
        self.shared.cv.notify_all();
    }

    pub fn take(&self) -> Option<Snapshot> {
        lock(&self.shared.state).latest.take()
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        lock(&self.shared.state).stop = true;
        self.shared.cv.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run(shared: Arc<Shared>, hwnd: isize, msg: u32) {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let mut sources = build_sources();
    loop {
        let started = Instant::now();
        let cfg = {
            let st = lock(&shared.state);
            if st.stop {
                break;
            }
            st.cfg.clone()
        };
        let mut snap = Snapshot::default();
        for s in sources.iter_mut().filter(|s| cfg.is_active(s.module())) {
            s.sample(&mut snap);
        }
        lock(&shared.state).latest = Some(snap);
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as _)), msg, WPARAM(0), LPARAM(0));
        }

        let mut st = lock(&shared.state);
        loop {
            if st.stop {
                break;
            }
            let deadline = started + Duration::from_millis(st.cfg.interval_ms as u64);
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let generation = st.generation;
            st = shared.cv.wait_timeout(st, deadline - now).unwrap_or_else(PoisonError::into_inner).0;
            // A config change may enable modules that should appear promptly.
            if st.generation != generation && started.elapsed() >= Duration::from_millis(250) {
                break;
            }
        }
    }
    drop(sources);
    if com {
        unsafe { CoUninitialize() };
    }
}

/// Poison-tolerant lock: a panicking source must not take the UI down with it.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
