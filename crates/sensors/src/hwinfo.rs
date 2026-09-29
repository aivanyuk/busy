//! HWiNFO shared memory (`Global\HWiNFO_SENS_SM2`, only when "Shared Memory Support" is enabled).
//! Layout per HWiNFO's hwisenssm2.h (#pragma pack(1)):
//! header: sig u32 @0 ("HWiS" while active, "DEAD" once HWiNFO stopped), version @4, revision @8,
//!         poll_time i64 @12, sensor off/size/count @20/24/28, reading off/size/count @32/36/40.
//! sensor:  id u32, instance u32, name_orig[128] @8, name_user[128] @136.
//! reading: type u32, sensor_index u32 @4, id u32 @8, label_orig[128] @12, label_user[128] @140,
//!          unit[16] @268, value/min/max/avg f64 @284.
//!
//! The view stays mapped between samples; each sample copies only the header-declared extent. A mapping
//! whose signature is no longer "HWiS", or whose poll time has not moved for `STALE` (HWiNFO killed), is
//! dropped and reopened after `RETRY`, which also picks up a new section after HWiNFO restarts.

use std::time::{Duration, Instant};

use busy_core::{SensorKind, SensorReading};
use windows::Win32::System::Memory::{
    FILE_MAP_READ, MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW,
    UnmapViewOfFile, VirtualQuery,
};
use windows::core::{Owned, w};

use crate::{ansi, reading};

const SIG: u32 = u32::from_le_bytes(*b"HWiS");
const HEADER: usize = 44;
const RETRY: Duration = Duration::from_secs(5);
/// HWiNFO polls every 2 s by default; a poll time frozen this long means it is gone.
const STALE: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct HwInfo {
    next_try: Option<Instant>,
    view: Option<View>,
    /// Copy of the used part of the view, reused across samples.
    buf: Vec<u8>,
    /// Last poll time seen, and when it last changed.
    poll: (i64, Option<Instant>),
}

impl HwInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&mut self) -> Vec<SensorReading> {
        if self.view.is_none() {
            if self.next_try.is_some_and(|t| Instant::now() < t) {
                return Vec::new();
            }
            self.view = View::open();
            self.poll = (0, None);
        }
        let r = self.copy().then(|| parse(&self.buf)).flatten().unwrap_or_default();
        if r.is_empty() {
            self.view = None;
            self.next_try = Some(Instant::now() + RETRY);
        }
        r
    }

    /// Copies the header-declared extent of the view into `buf` (HWiNFO writes concurrently; parse the
    /// copy). False once HWiNFO has stopped or its poll time froze.
    fn copy(&mut self) -> bool {
        let Some(v) = &self.view else { return false };
        let mut header = [0u8; HEADER];
        if !v.read(0, &mut header) || u32_at(&header, 0) != Some(SIG) {
            return false;
        }
        let poll = i64::from_le_bytes(header[12..20].try_into().unwrap_or_default());
        let now = Instant::now();
        match self.poll {
            (p, Some(at)) if p == poll && now.duration_since(at) >= STALE => return false,
            (p, Some(_)) if p == poll => {}
            _ => self.poll = (poll, Some(now)),
        }
        let Some(len) = extent(&header).filter(|&n| n <= v.len) else { return false };
        self.buf.resize(len, 0);
        v.read(0, &mut self.buf)
    }
}

/// `Global\HWiNFO_SENS_SM2` mapped read-only, unmapped on drop; the view keeps the section alive after the
/// handle is closed. The `windows` crate has no `Free` for view addresses, so no `Owned`.
struct View {
    addr: MEMORY_MAPPED_VIEW_ADDRESS,
    len: usize,
}

impl View {
    fn open() -> Option<Self> {
        let name = w!(r"Global\HWiNFO_SENS_SM2");
        // SAFETY: `name` is NUL-terminated (a `w!` literal); on success we own the returned handle.
        let h = unsafe { Owned::new(OpenFileMappingW(FILE_MAP_READ.0, false, name).ok()?) };
        // SAFETY: `h` is an open mapping handle; the view keeps the section alive after `h` closes.
        let addr = unsafe { MapViewOfFile(*h, FILE_MAP_READ, 0, 0, 0) };
        if addr.Value.is_null() {
            return None;
        }
        // Owned from here, so it is unmapped whatever happens next.
        let mut view = Self { addr, len: 0 };
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: `mbi` is a writable buffer of the size passed.
        let n = unsafe { VirtualQuery(Some(view.addr.Value), &mut mbi, size_of::<MEMORY_BASIC_INFORMATION>()) };
        view.len = if n == 0 { 0 } else { mbi.RegionSize.min(64 << 20) };
        Some(view)
    }

    /// Copies `out.len()` bytes from `off`; false if that range is not inside the view.
    fn read(&self, off: usize, out: &mut [u8]) -> bool {
        if off.checked_add(out.len()).is_none_or(|end| end > self.len) {
            return false;
        }
        // SAFETY: `off..off + out.len()` lies within the `len` mapped, readable bytes checked above.
        unsafe { std::ptr::copy_nonoverlapping(self.addr.Value.cast::<u8>().add(off), out.as_mut_ptr(), out.len()) };
        true
    }
}

impl Drop for View {
    fn drop(&mut self) {
        // SAFETY: we own the view `open` mapped.
        unsafe { _ = UnmapViewOfFile(self.addr) };
    }
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).and_then(|x| x.try_into().ok()).map(u32::from_le_bytes)
}

/// Bytes the header says are in use: through the end of the sensor and reading arrays.
fn extent(header: &[u8]) -> Option<usize> {
    let h = |o| u32_at(header, o).map(|v| v as usize);
    let end = |off: usize, size: usize, n: usize| off.checked_add(size.checked_mul(n)?);
    let sensors = end(h(20)?, h(24)?, h(28)?)?;
    let readings = end(h(32)?, h(36)?, h(40)?)?;
    Some(sensors.max(readings).max(HEADER))
}

fn parse(m: &[u8]) -> Option<Vec<SensorReading>> {
    if u32_at(m, 0)? != SIG {
        return None;
    }
    let h = |o| u32_at(m, o).map(|v| v as usize);
    let (s_off, s_size, s_n, r_off, r_size, r_n) = (h(20)?, h(24)?, h(28)?, h(32)?, h(36)?, h(40)?);
    if s_size < 264 || r_size < 316 || s_n > 4096 || r_n > 65536 {
        return None;
    }
    let elem = |off: usize, size: usize, i: usize, need: usize| {
        m.get(off.checked_add(i.checked_mul(size)?)?..).and_then(|e| e.get(..need))
    };
    let label = |user: &[u8], orig: &[u8]| Some(ansi(user)).filter(|s| !s.is_empty()).unwrap_or_else(|| ansi(orig));

    let sensors: Vec<String> = (0..s_n)
        .map(|i| elem(s_off, s_size, i, 264).map_or_else(String::new, |e| label(&e[136..264], &e[8..136])))
        .collect();

    let mut out = Vec::new();
    for i in 0..r_n {
        let Some(e) = elem(r_off, r_size, i, 316) else { break };
        let unit = ansi(&e[268..284]);
        let value = f64::from_le_bytes(e[284..292].try_into().ok()?) as f32;
        let (kind, value) = match (u32_at(e, 0)?, unit.as_str()) {
            (1, u) if u.ends_with('F') => (SensorKind::Temperature, (value - 32.0) * 5.0 / 9.0),
            (1, _) => (SensorKind::Temperature, value),
            (2, "V") => (SensorKind::Voltage, value),
            (3, "RPM") => (SensorKind::Fan, value),
            (4, "A") => (SensorKind::Other, value),
            (5, "W") => (SensorKind::Power, value),
            (6, "MHz") => (SensorKind::Clock, value),
            (7, "%") => (SensorKind::Load, value),
            _ => continue,
        };
        if !value.is_finite() {
            continue;
        }
        let hw = sensors.get(u32_at(e, 4)? as usize).map_or("", String::as_str);
        out.push(reading("HWiNFO", hw, &label(&e[140..268], &e[12..140]), kind, value));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{HEADER, extent};

    fn header(fields: [u32; 6]) -> Vec<u8> {
        let mut h = vec![0u8; HEADER];
        for (i, v) in fields.iter().enumerate() {
            h[20 + i * 4..24 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        h
    }

    #[test]
    fn extent_covers_both_arrays() {
        assert_eq!(extent(&header([44, 264, 2, 572, 316, 3])), Some(572 + 316 * 3));
        assert_eq!(extent(&header([2000, 264, 2, 44, 316, 1])), Some(2000 + 264 * 2));
        assert_eq!(extent(&header([0, 0, 0, 0, 0, 0])), Some(HEADER));
    }

    #[test]
    fn extent_rejects_truncated_headers_and_never_wraps() {
        assert_eq!(extent(&header([44, 264, 2, 572, 316, 3])[..40]), None);
        assert_eq!(extent(&header([u32::MAX, u32::MAX, u32::MAX, 0, 0, 0])).map(|n| n > 1 << 40), Some(true));
    }
}
