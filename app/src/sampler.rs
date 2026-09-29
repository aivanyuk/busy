//! Background sampling thread. Sources are created and driven only on this thread.

use crate::sync::lock;
use busy_core::{Config, Module, Snapshot, Source, SourceOptions};
use busy_ui::history::Fresh;
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

const N: usize = Module::ALL.len();

/// The part of the config the sampler needs; `Copy`, so reading it under the lock never allocates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Params {
    /// Update interval per module (`Config::module_interval_ms`), indexed by `Module::index`.
    interval_ms: [u32; N],
    /// Indexed by `Module::index`.
    active: [bool; N],
    /// Handed to every source through `Source::configure` when it changes.
    sources: SourceOptions,
    /// Nothing is visible (session locked, display off): sample nothing until that ends.
    paused: bool,
}

/// How soon after the last sample a module that just became visible is sampled.
const PROMPT: Duration = Duration::from_millis(250);

impl Params {
    /// `open` is the module whose flyout is open.
    pub fn new(cfg: &Config, open: Option<Module>, paused: bool) -> Self {
        Self {
            interval_ms: Module::ALL.map(|m| cfg.module_interval_ms(m)),
            active: Module::ALL.map(|m| cfg.is_active(m, open)),
            sources: cfg.source_options(),
            paused,
        }
    }

    fn is_active(&self, m: Module) -> bool {
        !self.paused && self.active[m.index()]
    }

    fn interval(&self, m: Module) -> Duration {
        Duration::from_millis(self.interval_ms[m.index()] as u64)
    }
}

/// When each module was last sampled, and so which are due. Pure timing logic, driven with explicit `now`s.
#[derive(Default)]
struct Schedule {
    last: [Option<Instant>; N],
    /// The last time anything was sampled.
    tick: Option<Instant>,
}

impl Schedule {
    /// When `m` is next due: its interval after its last sample; a module never sampled (or just re-enabled)
    /// `PROMPT` after the last tick, so it fills in promptly without sampling everything twice in a row.
    fn due_at(&self, p: &Params, m: Module, now: Instant) -> Option<Instant> {
        if !p.is_active(m) {
            return None;
        }
        Some(match self.last[m.index()] {
            Some(t) => t + p.interval(m),
            None => self.tick.map_or(now, |t| t + PROMPT),
        })
    }

    /// Modules to sample at `now`. GPU and Sensors share readings (vendor sensors come from the GPU source,
    /// third-party GPU temperatures go into `gpus`), so when one is due both run, at the shorter interval.
    fn due(&self, p: &Params, now: Instant) -> Fresh {
        let mut due = Module::ALL.map(|m| self.due_at(p, m, now).is_some_and(|t| t <= now));
        let (gpu, sensors) = (Module::Gpu, Module::Sensors);
        if due[gpu.index()] || due[sensors.index()] {
            due[gpu.index()] = p.is_active(gpu);
            due[sensors.index()] = p.is_active(sensors);
        }
        due
    }

    /// The earliest time something is due; `None` while nothing is active.
    fn next(&self, p: &Params, now: Instant) -> Option<Instant> {
        Module::ALL.into_iter().filter_map(|m| self.due_at(p, m, now)).min()
    }

    fn sampled(&mut self, due: &Fresh, now: Instant) {
        for (last, _) in self.last.iter_mut().zip(due).filter(|(_, d)| **d) {
            *last = Some(now);
        }
        self.tick = Some(now);
    }

    /// Whether a module that stopped being sampled (not merely paused) still has readings to drop.
    fn has_inactive(&self, p: &Params) -> bool {
        (0..N).any(|i| !p.active[i] && self.last[i].is_some())
    }

    /// Forgets modules that stopped being sampled (not merely paused) and returns them, so their readings can
    /// be dropped and they count as new when they come back.
    fn forget_inactive(&mut self, p: &Params) -> Fresh {
        std::array::from_fn(|i| !p.active[i] && self.last[i].take().is_some())
    }
}

/// A snapshot and the modules it refreshed since the UI last took one.
type Update = (Snapshot, Fresh);

struct State {
    params: Params,
    latest: Option<Update>,
    stop: bool,
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
        let shared =
            Arc::new(Shared { state: Mutex::new(State { params, latest: None, stop: false }), cv: Condvar::new() });
        let hwnd = notify.0 as isize;
        let sh = shared.clone();
        let thread = std::thread::Builder::new().name("busy-sampler".into()).spawn(move || run(sh, hwnd, msg)).ok();
        Self { shared, thread }
    }

    pub fn set_params(&self, params: Params) {
        let mut st = lock(&self.shared.state);
        if std::mem::replace(&mut st.params, params) != params {
            self.shared.cv.notify_all();
        }
    }

    /// The latest snapshot (every module's last readings) and which modules changed since the previous `take`.
    pub fn take(&self) -> Option<Update> {
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
    // SAFETY: first COM call on this thread; balanced by CoUninitialize below when it succeeded.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let mut sources = build_sources();
    let mut configured = None;
    // Kept across ticks: a module that is not due keeps its last readings.
    let mut snap = Snapshot::default();
    let mut schedule = Schedule::default();
    loop {
        let params = {
            let mut st = lock(&shared.state);
            while st.params.paused && !st.stop {
                st = shared.cv.wait(st).unwrap_or_else(PoisonError::into_inner);
            }
            if st.stop {
                break;
            }
            st.params
        };
        if configured != Some(params.sources) {
            for s in &mut sources {
                s.configure(params.sources);
            }
            configured = Some(params.sources);
        }
        let now = Instant::now();
        let gone = schedule.forget_inactive(&params);
        let due = schedule.due(&params, now);
        let changed: Fresh = std::array::from_fn(|i| gone[i] || due[i]);
        for m in Module::ALL.into_iter().filter(|m| changed[m.index()]) {
            snap.clear(m);
        }
        for s in sources.iter_mut().filter(|s| due[s.module().index()]) {
            s.sample(&mut snap);
        }
        if due.contains(&true) {
            schedule.sampled(&due, now);
        }
        if changed.contains(&true) {
            let out = snap.clone();
            let old = {
                let mut st = lock(&shared.state);
                // The UI may not have taken the previous update yet: it still has to learn what that one refreshed.
                let fresh = st.latest.as_ref().map_or(changed, |(_, f)| std::array::from_fn(|i| f[i] || changed[i]));
                st.latest.replace((out, fresh))
            };
            // Freed outside the lock.
            drop(old);
            // SAFETY: PostMessageW only reads its arguments; a stale HWND makes it fail harmlessly.
            unsafe {
                let _ = PostMessageW(Some(HWND(hwnd as _)), msg, WPARAM(0), LPARAM(0));
            }
        }

        let mut st = lock(&shared.state);
        loop {
            // A disabled module's readings are dropped at once, not at its next due time.
            if st.stop || st.params.paused || schedule.has_inactive(&st.params) {
                break;
            }
            let now = Instant::now();
            st = match schedule.next(&st.params, now) {
                Some(t) if t <= now => break,
                Some(t) => shared.cv.wait_timeout(st, t - now).unwrap_or_else(PoisonError::into_inner).0,
                // Nothing active: sleep until the params change.
                None => shared.cv.wait(st).unwrap_or_else(PoisonError::into_inner),
            };
        }
    }
    drop(sources);
    if com {
        // SAFETY: balances the successful CoInitializeEx above, after every COM object (the sources) is dropped.
        unsafe { CoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use super::{N, PROMPT, Params, Schedule};
    use busy_core::{Config, Module};
    use std::time::{Duration, Instant};

    fn secs(s: f32) -> Duration {
        Duration::from_secs_f32(s)
    }

    fn set_interval(cfg: &mut Config, m: Module, s: Option<u32>) {
        cfg.modules.iter_mut().filter(|c| c.module == m).for_each(|c| c.interval_s = s);
    }

    fn due_list(s: &Schedule, p: &Params, now: Instant) -> Vec<Module> {
        let due = s.due(p, now);
        Module::ALL.into_iter().filter(|m| due[m.index()]).collect()
    }

    #[test]
    fn params_follow_the_config() {
        let mut cfg = Config { interval_ms: 2000, ..Config::default() };
        for m in &mut cfg.modules {
            if m.module == Module::Disk {
                (m.taskbar, m.flyout) = (false, false);
            }
        }
        set_interval(&mut cfg, Module::Memory, Some(5));
        cfg.opt_in.third_party_sensors = true;
        let p = Params::new(&cfg, Some(Module::Battery), false);
        assert_eq!(p.interval_ms[Module::Cpu.index()], 2000);
        assert_eq!(p.interval_ms[Module::Memory.index()], 5000);
        assert!(p.sources.third_party_sensors);
        let active: Vec<_> = Module::ALL.into_iter().filter(|&m| p.is_active(m)).collect();
        assert_eq!(active, vec![Module::Cpu, Module::Memory, Module::Network, Module::Gpu, Module::Battery]);
    }

    #[test]
    fn the_open_flyout_adds_what_it_shows() {
        let cfg = Config::default();
        let (closed, open) = (Params::new(&cfg, None, false), Params::new(&cfg, Some(Module::Disk), false));
        assert!(closed.is_active(Module::Cpu) && !closed.is_active(Module::Processes));
        assert!(open.is_active(Module::Disk) && open.is_active(Module::Processes) && !open.is_active(Module::Battery));
    }

    #[test]
    fn each_module_runs_at_its_own_interval() {
        let mut cfg = Config { interval_ms: 1000, ..Config::default() };
        set_interval(&mut cfg, Module::Memory, Some(3));
        let p = Params::new(&cfg, None, false);
        let t0 = Instant::now();
        let mut s = Schedule::default();
        // Everything active is due on the first tick.
        assert_eq!(due_list(&s, &p, t0), vec![Module::Cpu, Module::Memory, Module::Network, Module::Gpu]);
        s.sampled(&s.due(&p, t0), t0);
        assert_eq!(s.next(&p, t0), Some(t0 + secs(1.0)));
        assert!(due_list(&s, &p, t0 + secs(0.5)).is_empty());
        for t in [1.0, 2.0] {
            let now = t0 + secs(t);
            assert_eq!(due_list(&s, &p, now), vec![Module::Cpu, Module::Network, Module::Gpu], "{t}");
            s.sampled(&s.due(&p, now), now);
        }
        let all = vec![Module::Cpu, Module::Memory, Module::Network, Module::Gpu];
        assert_eq!(due_list(&s, &p, t0 + secs(3.0)), all);
    }

    #[test]
    fn gpu_and_sensors_run_together() {
        let mut cfg = Config { interval_ms: 5000, ..Config::default() };
        cfg.modules.iter_mut().filter(|c| c.module == Module::Sensors).for_each(|c| c.taskbar = true);
        set_interval(&mut cfg, Module::Sensors, Some(1));
        let p = Params::new(&cfg, None, false);
        let t0 = Instant::now();
        let mut s = Schedule::default();
        s.sampled(&s.due(&p, t0), t0);
        assert_eq!(due_list(&s, &p, t0 + secs(1.0)), vec![Module::Gpu, Module::Sensors]);
    }

    #[test]
    fn a_module_that_appears_is_sampled_promptly() {
        let cfg = Config { interval_ms: 10_000, ..Config::default() };
        let (closed, open) = (Params::new(&cfg, None, false), Params::new(&cfg, Some(Module::Disk), false));
        let t0 = Instant::now();
        let mut s = Schedule::default();
        s.sampled(&s.due(&closed, t0), t0);
        assert_eq!(s.next(&open, t0), Some(t0 + PROMPT));
        // Sensors pulls GPU along.
        let want = vec![Module::Disk, Module::Gpu, Module::Sensors, Module::Processes];
        assert_eq!(due_list(&s, &open, t0 + PROMPT), want);
    }

    #[test]
    fn disabled_modules_are_forgotten_but_paused_ones_are_not() {
        let cfg = Config::default();
        let disk = Some(Module::Disk);
        let (open, closed, paused) =
            (Params::new(&cfg, disk, false), Params::new(&cfg, None, false), Params::new(&cfg, disk, true));
        let t0 = Instant::now();
        let mut s = Schedule::default();
        s.sampled(&s.due(&open, t0), t0);
        assert!(!s.has_inactive(&paused));
        assert_eq!(s.forget_inactive(&paused), [false; N]);
        assert!(due_list(&s, &paused, t0 + secs(60.0)).is_empty());
        assert_eq!(s.next(&paused, t0), None);
        assert!(s.has_inactive(&closed));
        let gone = s.forget_inactive(&closed);
        let gone: Vec<_> = Module::ALL.into_iter().filter(|m| gone[m.index()]).collect();
        assert_eq!(gone, vec![Module::Disk, Module::Sensors, Module::Processes]);
        assert!(!s.has_inactive(&closed));
    }
}
