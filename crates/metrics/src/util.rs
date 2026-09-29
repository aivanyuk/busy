use std::time::{Duration, Instant};
use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};

pub fn perf_info() -> Option<PERFORMANCE_INFORMATION> {
    let mut pi = PERFORMANCE_INFORMATION::default();
    unsafe { GetPerformanceInfo(&mut pi, size_of::<PERFORMANCE_INFORMATION>() as u32) }.ok()?;
    Some(pi)
}

/// Rate-limits refreshes of slow-changing data.
#[derive(Default)]
pub struct Every(Option<Instant>);

impl Every {
    pub fn due(&mut self, secs: u64) -> bool {
        let now = Instant::now();
        let due = self.0.is_none_or(|t| now - t >= Duration::from_secs(secs));
        if due {
            self.0 = Some(now);
        }
        due
    }

    /// Restart the period from now.
    pub fn arm(&mut self) {
        self.0 = Some(Instant::now());
    }
}

/// Seconds elapsed since the previous call (None on first call).
#[derive(Default)]
pub struct Clock(Option<Instant>);

impl Clock {
    pub fn tick(&mut self) -> Option<f64> {
        let now = Instant::now();
        let dt = self.0.map(|t| (now - t).as_secs_f64()).filter(|&d| d > 0.0);
        self.0 = Some(now);
        dt
    }
}
