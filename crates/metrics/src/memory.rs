use crate::util::perf_info;
use busy_core::{MemInfo, Module, Snapshot, Source};
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/// SYSTEM_MEMORY_LIST_INFORMATION (phnt `ntexapi.h`), page counts. Not in the `windows` crate.
#[repr(C)]
#[derive(Default)]
struct MemoryLists {
    zero: usize,
    free: usize,
    modified: usize,
    _modified_no_write: usize,
    _bad: usize,
    standby_by_priority: [usize; 8],
    _repurposed_by_priority: [usize; 8],
    _modified_page_file: usize,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<MemoryLists>() == 176 && std::mem::offset_of!(MemoryLists, standby_by_priority) == 40);

const SYSTEM_MEMORY_LIST_INFORMATION: SYSTEM_INFORMATION_CLASS = SYSTEM_INFORMATION_CLASS(80);

/// Page-list sizes in pages. Querying works unelevated (verified on Windows 11 26200, ~1 µs).
fn memory_lists() -> Option<MemoryLists> {
    let mut m = MemoryLists::default();
    let mut ret = 0u32;
    let size = size_of::<MemoryLists>() as u32;
    // SAFETY: `m` is a writable, owned buffer of exactly `size` bytes, and `size` tells the kernel so.
    let st = unsafe { NtQuerySystemInformation(SYSTEM_MEMORY_LIST_INFORMATION, (&raw mut m).cast(), size, &mut ret) };
    // A shorter answer (different layout) would leave fields unset; take only a full one.
    (st.is_ok() && ret == size).then_some(m)
}

pub struct Memory;

impl Source for Memory {
    fn module(&self) -> Module {
        Module::Memory
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        let mut ms = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        if unsafe { GlobalMemoryStatusEx(&mut ms) }.is_err() {
            return;
        }
        let pi = perf_info();
        let page = pi.map_or(4096, |p| p.PageSize as u64);
        let lists = memory_lists();
        let pages = |f: fn(&MemoryLists) -> usize| lists.as_ref().map(|l| (f(l) as u64).saturating_mul(page));
        snap.memory = Some(MemInfo {
            total: ms.ullTotalPhys,
            used: ms.ullTotalPhys - ms.ullAvailPhys,
            available: ms.ullAvailPhys,
            // ullTotalPageFile is the commit limit, ullAvailPageFile what remains of it.
            commit_used: pi.map_or(ms.ullTotalPageFile - ms.ullAvailPageFile, |p| p.CommitTotal as u64 * page),
            commit_limit: pi.map_or(ms.ullTotalPageFile, |p| p.CommitLimit as u64 * page),
            cached: pi.map(|p| p.SystemCache as u64 * page),
            compressed: None,
            modified: pages(|l| l.modified),
            standby: pages(|l| l.standby_by_priority.iter().fold(0, |a, &b| a.saturating_add(b))),
            free: pages(|l| l.free.saturating_add(l.zero)),
        });
    }
}
