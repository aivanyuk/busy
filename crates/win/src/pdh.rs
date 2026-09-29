use crate::text::wide;
use windows::Win32::System::Performance::{
    PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W,
    PDH_FMT_DOUBLE, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCollectQueryData,
    PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::core::{Owned, PCWSTR};

/// Not exposed by the `windows` crate (pdh.h).
pub const PDH_FMT_NOCAP100: u32 = 0x8000;

/// A formatted value is usable only when its `CStatus` is one of these two success codes ("Checking PDH
/// Interface Return Values"). Everything else is a warning or error severity carrying garbage, e.g.
/// `PDH_CSTATUS_INVALID_DATA` or `PDH_CALC_NEGATIVE_VALUE` on a rate counter whose instance just restarted.
fn valid(status: u32) -> bool {
    status == PDH_CSTATUS_VALID_DATA || status == PDH_CSTATUS_NEW_DATA
}

/// A real-time PDH query, closed on drop together with its counters.
pub struct Query(Owned<PDH_HQUERY>);

/// A counter of a [`Query`]; PDH closes it with the query, so keep the two together (one struct per source).
#[derive(Clone, Copy)]
pub struct Counter(PDH_HCOUNTER);

/// Item buffer for wildcard reads, kept by the caller across samples so it is not reallocated each tick.
#[derive(Default)]
pub struct ArrayBuf(
    // u64 backing keeps the item array 8-byte aligned; instance names point into the same buffer.
    Vec<u64>,
);

impl Query {
    pub fn open() -> Option<Self> {
        let mut h = PDH_HQUERY::default();
        // SAFETY: `h` is a valid out-pointer; a null data source means real-time data.
        let r = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut h) };
        // SAFETY: on success we own the new query handle.
        (r == 0).then(|| Self(unsafe { Owned::new(h) }))
    }

    /// English (locale-independent) counter path, may contain a `(*)` wildcard.
    pub fn add(&self, path: &str) -> Option<Counter> {
        let w = wide(path);
        let mut c = PDH_HCOUNTER::default();
        // SAFETY: `w` is NUL-terminated and outlives the call; `c` is a valid out-pointer.
        let r = unsafe { PdhAddEnglishCounterW(*self.0, PCWSTR(w.as_ptr()), 0, &mut c) };
        (r == 0).then_some(Counter(c))
    }

    /// Takes a sample of every counter; rate counters need two before they have data.
    pub fn collect(&self) -> bool {
        // SAFETY: the query handle is open while `self` lives.
        unsafe { PdhCollectQueryData(*self.0) == 0 }
    }
}

impl Counter {
    /// Single value as `f64`; `flags` are extra `PDH_FMT_*` bits such as [`PDH_FMT_NOCAP100`].
    pub fn value(self, flags: u32) -> Option<f64> {
        let mut v = PDH_FMT_COUNTERVALUE::default();
        // SAFETY: `v` is a valid out-pointer.
        let r = unsafe { PdhGetFormattedCounterValue(self.0, PDH_FMT(PDH_FMT_DOUBLE.0 | flags), None, &mut v) };
        // SAFETY: PDH_FMT_DOUBLE selects the `doubleValue` member.
        (r == 0 && valid(v.CStatus)).then_some(unsafe { v.Anonymous.doubleValue })
    }

    /// Calls `f(instance, value)` for each instance of a wildcard counter with valid data. False if the array
    /// could not be read.
    pub fn each_double(self, flags: u32, buf: &mut ArrayBuf, mut f: impl FnMut(&str, f64)) -> bool {
        // SAFETY: PDH_FMT_DOUBLE selects the `doubleValue` member.
        self.each(PDH_FMT(PDH_FMT_DOUBLE.0 | flags), buf, |n, v| f(&n, unsafe { v.Anonymous.doubleValue }))
    }

    /// [`Self::each_double`] for a counter read as `i64` (byte counts).
    pub fn each_large(self, buf: &mut ArrayBuf, mut f: impl FnMut(&str, i64)) -> bool {
        // SAFETY: PDH_FMT_LARGE selects the `largeValue` member.
        self.each(PDH_FMT_LARGE, buf, |n, v| f(&n, unsafe { v.Anonymous.largeValue }))
    }

    /// Values of a wildcard counter as (instance, value), allocated per call.
    pub fn array(self, flags: u32) -> Option<Vec<(String, f64)>> {
        let mut out = Vec::new();
        let fmt = PDH_FMT(PDH_FMT_DOUBLE.0 | flags);
        // SAFETY: PDH_FMT_DOUBLE selects the `doubleValue` member.
        self.each(fmt, &mut ArrayBuf::default(), |n, v| out.push((n, unsafe { v.Anonymous.doubleValue })))
            .then_some(out)
    }

    fn each(self, fmt: PDH_FMT, buf: &mut ArrayBuf, mut f: impl FnMut(String, &PDH_FMT_COUNTERVALUE)) -> bool {
        let (mut size, mut count) = (0u32, 0u32);
        // SAFETY: a null buffer asks only for the size.
        if unsafe { PdhGetFormattedCounterArrayW(self.0, fmt, &mut size, &mut count, None) } != PDH_MORE_DATA {
            return false;
        }
        let items = &mut buf.0;
        items.resize(items.len().max((size as usize).div_ceil(8)), 0);
        let cap = items.len() * 8;
        let ptr = items.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        // SAFETY: `ptr` has at least `size` writable, 8-byte aligned bytes, and `size` tells the API so.
        if unsafe { PdhGetFormattedCounterArrayW(self.0, fmt, &mut size, &mut count, Some(ptr)) } != 0 {
            return false;
        }
        if (count as usize).saturating_mul(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>()) > cap {
            return false;
        }
        // SAFETY: PDH wrote `count` items at the start of the buffer, which holds them (checked above).
        let items = unsafe { std::slice::from_raw_parts(ptr, count as usize) };
        for it in items.iter().filter(|it| valid(it.FmtValue.CStatus)) {
            // SAFETY: `szName` is a NUL-terminated string PDH wrote into the same, still unmodified buffer.
            let w = unsafe { it.szName.as_wide() };
            // `from_utf16` first: `from_utf16_lossy` (and a hand-written decode loop) is compiled at our opt-level
            // and cost the GPU source (~600 engine instances) 1.5–2 ms a tick in debug builds.
            f(String::from_utf16(w).unwrap_or_else(|_| String::from_utf16_lossy(w)), &it.FmtValue);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use windows::Win32::System::Performance::{PDH_CALC_NEGATIVE_VALUE, PDH_CSTATUS_INVALID_DATA, PDH_NO_DATA};

    #[test]
    fn only_success_statuses_are_valid() {
        assert!(valid(PDH_CSTATUS_VALID_DATA) && valid(PDH_CSTATUS_NEW_DATA));
        for s in [PDH_CSTATUS_INVALID_DATA, PDH_CALC_NEGATIVE_VALUE, PDH_NO_DATA, 2] {
            assert!(!valid(s), "{s:#x}");
        }
    }

    #[test]
    fn reads_processor_time() {
        let q = Query::open().unwrap();
        let total = q.add(r"\Processor(_Total)\% Processor Time").unwrap();
        let each = q.add(r"\Processor(*)\% Processor Time").unwrap();
        assert!(q.add(r"\busy no such object(*)\nothing").is_none());
        assert!(q.collect());
        // A rate counter needs two collections, and under load an instance can still report invalid data.
        let a = (0..20)
            .find_map(|_| {
                std::thread::sleep(Duration::from_millis(100));
                let a = q.collect().then(|| each.array(0)).flatten()?;
                a.iter().any(|(n, _)| n == "_Total").then_some(a)
            })
            .unwrap();
        assert!(a.iter().all(|(n, v)| !n.is_empty() && (0.0..=100.0).contains(v)), "{a:?}");
        assert!(total.value(PDH_FMT_NOCAP100).is_some_and(|v| v >= 0.0));

        let mut buf = ArrayBuf::default();
        for _ in 0..2 {
            let mut names = Vec::new();
            assert!(each.each_large(&mut buf, |n, _| names.push(n.to_owned())));
            assert_eq!(names, a.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
        }
    }
}
