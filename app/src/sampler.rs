//! Background sampling thread. Sources are created and driven only on this thread.

use crate::sync::lock;
use busy_core::{Config, Module, Snapshot, Source};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// The single place where collectors are registered.
fn build_sources() -> Vec<Box<dyn Source>> {
    // Debug builds: `BUSY_FAKE=1` gives synthetic data filling every field, for UI work on machines without a
    // GPU/battery/sensors.
    #[cfg(debug_assertions)]
    if std::env::var_os("BUSY_FAKE").is_some() {
        return crate::fake::sources();
    }
    let mut sources = busy_metrics::sources();
    // Order matters: the sensors crate's GPU source must run before its Sensors source.
    sources.extend(busy_sensors::sources());
    sources
}

/// The part of the config the sampler needs; `Copy`, so reading it under the lock never allocates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Params {
    interval_ms: u32,
    /// Indexed like `Module::ALL`.
    active: [bool; Module::ALL.len()],
}

impl Params {
    pub fn new(cfg: &Config) -> Self {
        Self { interval_ms: cfg.interval_ms, active: Module::ALL.map(|m| cfg.is_active(m)) }
    }

    fn is_active(&self, m: Module) -> bool {
        Module::ALL.iter().position(|&x| x == m).is_some_and(|i| self.active[i])
    }
}

struct State {
    params: Params,
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
    pub fn start(params: Params, notify: HWND, msg: u32) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State { params, latest: None, stop: false, generation: 0 }),
            cv: Condvar::new(),
        });
        let hwnd = notify.0 as isize;
        let sh = shared.clone();
        let thread = std::thread::Builder::new().name("busy-sampler".into()).spawn(move || run(sh, hwnd, msg)).ok();
        Self { shared, thread }
    }

    pub fn set_params(&self, params: Params) {
        let mut st = lock(&self.shared.state);
        st.params = params;
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
        let params = {
            let st = lock(&shared.state);
            if st.stop {
                break;
            }
            st.params
        };
        let mut snap = Snapshot::default();
        for s in sources.iter_mut().filter(|s| params.is_active(s.module())) {
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
            let deadline = started + Duration::from_millis(st.params.interval_ms as u64);
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

#[cfg(test)]
mod tests {
    use super::Params;
    use busy_core::{Config, Module};

    #[test]
    fn params_follow_the_config() {
        let mut cfg = Config { interval_ms: 2000, ..Config::default() };
        for m in &mut cfg.modules {
            if m.module == Module::Disk {
                (m.taskbar, m.flyout) = (false, false);
            }
        }
        let p = Params::new(&cfg);
        assert_eq!(p.interval_ms, 2000);
        assert!(!p.is_active(Module::Disk));
        assert!(Module::ALL.iter().filter(|&&m| m != Module::Disk).all(|&m| p.is_active(m)));
    }
}
