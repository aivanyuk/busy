use std::time::{Duration, Instant};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, REG_ROUTINE_FLAGS, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::core::PCWSTR;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// UTF-16 up to the first NUL.
pub fn from_wide(s: &[u16]) -> String {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..n])
}

fn reg_raw(key: &str, val: &str, flags: REG_ROUTINE_FLAGS, buf: &mut [u16]) -> Option<usize> {
    let (k, v) = (wide(key), wide(val));
    let mut cb = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(k.as_ptr()),
            PCWSTR(v.as_ptr()),
            flags,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut cb),
        )
    };
    (r == ERROR_SUCCESS).then_some(cb as usize)
}

pub fn reg_string(key: &str, val: &str) -> Option<String> {
    let mut buf = [0u16; 256];
    let cb = reg_raw(key, val, RRF_RT_REG_SZ, &mut buf)?;
    Some(from_wide(&buf[..cb / 2]).trim().to_owned())
}

pub fn reg_dword(key: &str, val: &str) -> Option<u32> {
    let mut buf = [0u16; 2];
    reg_raw(key, val, RRF_RT_REG_DWORD, &mut buf)?;
    Some(buf[0] as u32 | (buf[1] as u32) << 16)
}

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
