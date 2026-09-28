use crate::util::{from_wide, wide};
use windows::Win32::System::Performance::*;
use windows::core::PCWSTR;

/// Not exposed by the `windows` crate.
pub const NOCAP100: u32 = 0x8000;

pub struct Query(PDH_HQUERY);

#[derive(Clone, Copy)]
pub struct Counter(PDH_HCOUNTER);

fn valid(status: u32) -> bool {
    status == PDH_CSTATUS_VALID_DATA || status == PDH_CSTATUS_NEW_DATA
}

impl Query {
    pub fn open() -> Option<Self> {
        let mut h = PDH_HQUERY::default();
        (unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut h) } == 0).then_some(Self(h))
    }

    /// English (locale-independent) counter path, may contain a `(*)` wildcard.
    pub fn add(&self, path: &str) -> Option<Counter> {
        let w = wide(path);
        let mut c = PDH_HCOUNTER::default();
        (unsafe { PdhAddEnglishCounterW(self.0, PCWSTR(w.as_ptr()), 0, &mut c) } == 0).then_some(Counter(c))
    }

    pub fn collect(&self) -> bool {
        unsafe { PdhCollectQueryData(self.0) == 0 }
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.0) };
    }
}

impl Counter {
    pub fn value(self, flags: u32) -> Option<f64> {
        let mut v = PDH_FMT_COUNTERVALUE::default();
        let r = unsafe { PdhGetFormattedCounterValue(self.0, PDH_FMT(PDH_FMT_DOUBLE.0 | flags), None, &mut v) };
        (r == 0 && valid(v.CStatus)).then_some(unsafe { v.Anonymous.doubleValue })
    }

    /// Values of a wildcard counter as (instance, value); instances with invalid data are skipped.
    pub fn array(self, flags: u32) -> Option<Vec<(String, f64)>> {
        let fmt = PDH_FMT(PDH_FMT_DOUBLE.0 | flags);
        let (mut size, mut count) = (0u32, 0u32);
        if unsafe { PdhGetFormattedCounterArrayW(self.0, fmt, &mut size, &mut count, None) } != PDH_MORE_DATA {
            return None;
        }
        // u64 backing keeps the item array 8-byte aligned; names point into the same buffer.
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let items = buf.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        if unsafe { PdhGetFormattedCounterArrayW(self.0, fmt, &mut size, &mut count, Some(items)) } != 0 {
            return None;
        }
        let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
        Some(
            items
                .iter()
                .filter(|it| valid(it.FmtValue.CStatus))
                .map(|it| unsafe { (from_wide(it.szName.as_wide()), it.FmtValue.Anonymous.doubleValue) })
                .collect(),
        )
    }
}
