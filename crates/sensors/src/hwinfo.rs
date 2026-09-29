//! HWiNFO shared memory (`Global\HWiNFO_SENS_SM2`, only when "Shared Memory Support" is enabled).
//! Layout per HWiNFO's hwisenssm2.h (#pragma pack(1)):
//! header: sig u32 @0, version @4, revision @8, poll_time i64 @12,
//!         sensor off/size/count @20/24/28, reading off/size/count @32/36/40.
//! sensor:  id u32, instance u32, name_orig[128] @8, name_user[128] @136.
//! reading: type u32, sensor_index u32 @4, id u32 @8, label_orig[128] @12, label_user[128] @140,
//!          unit[16] @268, value/min/max/avg f64 @284.

use std::time::{Duration, Instant};

use busy_core::{SensorKind, SensorReading};
use windows::Win32::System::Memory::{
    FILE_MAP_READ, MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW,
    UnmapViewOfFile, VirtualQuery,
};
use windows::core::{Owned, PCWSTR, w};

use crate::{ansi, reading};

const SIG: u32 = u32::from_le_bytes(*b"HWiS");
const RETRY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct HwInfo {
    next_try: Option<Instant>,
}

impl HwInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&mut self) -> Vec<SensorReading> {
        if self.next_try.is_some_and(|t| Instant::now() < t) {
            return Vec::new();
        }
        let r = snapshot().and_then(|m| parse(&m)).unwrap_or_default();
        self.next_try = r.is_empty().then(|| Instant::now() + RETRY);
        r
    }
}

/// A mapped view, unmapped on drop. The `windows` crate has no `Free` for view addresses, so no `Owned`.
struct View(MEMORY_MAPPED_VIEW_ADDRESS);

impl View {
    fn map(name: PCWSTR) -> Option<Self> {
        // SAFETY: `name` is NUL-terminated (a `w!` literal); on success we own the returned handle.
        let h = unsafe { Owned::new(OpenFileMappingW(FILE_MAP_READ.0, false, name).ok()?) };
        // SAFETY: `h` is an open mapping handle; the view keeps the section alive after `h` closes.
        let v = unsafe { MapViewOfFile(*h, FILE_MAP_READ, 0, 0, 0) };
        (!v.Value.is_null()).then_some(Self(v))
    }
}

impl Drop for View {
    fn drop(&mut self) {
        // SAFETY: we own the view `map` created.
        unsafe { _ = UnmapViewOfFile(self.0) };
    }
}

/// Copies the whole mapping (HWiNFO writes concurrently; parse the copy).
fn snapshot() -> Option<Vec<u8>> {
    let view = View::map(w!(r"Global\HWiNFO_SENS_SM2"))?;
    let mut mbi = MEMORY_BASIC_INFORMATION::default();
    // SAFETY: `mbi` is a writable buffer of the size passed.
    let n = unsafe { VirtualQuery(Some(view.0.Value), &mut mbi, size_of::<MEMORY_BASIC_INFORMATION>()) };
    let len = if n == 0 { 0 } else { mbi.RegionSize.min(64 << 20) };
    let mut buf = vec![0u8; len];
    // SAFETY: the view's region holds at least `RegionSize` >= `len` readable bytes from its base.
    unsafe { std::ptr::copy_nonoverlapping(view.0.Value.cast::<u8>(), buf.as_mut_ptr(), len) };
    Some(buf)
}

fn parse(m: &[u8]) -> Option<Vec<SensorReading>> {
    let u32at = |b: &[u8], o: usize| b.get(o..o + 4).and_then(|x| x.try_into().ok()).map(u32::from_le_bytes);
    if u32at(m, 0)? != SIG {
        return None;
    }
    let h = |o| u32at(m, o).map(|v| v as usize);
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
        let (kind, value) = match (u32at(e, 0)?, unit.as_str()) {
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
        let hw = sensors.get(u32at(e, 4)? as usize).map_or("", String::as_str);
        out.push(reading("HWiNFO", hw, &label(&e[140..268], &e[12..140]), kind, value));
    }
    Some(out)
}
