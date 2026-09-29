use crate::util::perf_info;
use busy_core::{MemInfo, Module, Snapshot, Source};
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS};
use windows::Win32::System::SystemInformation::{
    GetPhysicallyInstalledSystemMemory, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};

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

/// SM_MEM_COMPRESSION_INFO_REQUEST (phnt `ntexapi.h`), request version 3. Sizes in bytes.
#[repr(C)]
#[derive(Default)]
struct CompressionInfo {
    /// Low 8 bits: request version; the rest is spare.
    version: u32,
    pid: u32,
    working_set: usize,
    _data_compressed: usize,
    _compressed_size: usize,
    _unique_data_compressed: usize,
}

/// SYSTEM_STORE_INFORMATION (phnt `ntexapi.h`).
#[repr(C)]
struct StoreInformation {
    version: u32,
    class: u32,
    data: *mut CompressionInfo,
    length: u32,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<CompressionInfo>() == 40
        && std::mem::offset_of!(CompressionInfo, working_set) == 8
        && size_of::<StoreInformation>() == 24
        && std::mem::offset_of!(StoreInformation, length) == 16
);

const SYSTEM_STORE_INFORMATION: SYSTEM_INFORMATION_CLASS = SYSTEM_INFORMATION_CLASS(109);
const MEM_COMPRESSION_INFO_REQUEST: u32 = 22;

/// RAM held by the compression store, i.e. the "Memory Compression" process working set, which unelevated
/// OpenProcess cannot read (access denied). Works unelevated, ~10 µs; None when compression is disabled.
fn compressed() -> Option<u64> {
    let mut info = CompressionInfo { version: 3, ..Default::default() };
    let mut req = StoreInformation {
        version: 1,
        class: MEM_COMPRESSION_INFO_REQUEST,
        data: &raw mut info,
        length: size_of::<CompressionInfo>() as u32,
    };
    let size = size_of::<StoreInformation>() as u32;
    // SAFETY: `req` is `size` bytes and points at `info`, whose true size it states; both outlive the call.
    let st = unsafe {
        NtQuerySystemInformation(SYSTEM_STORE_INFORMATION, (&raw mut req).cast(), size, std::ptr::null_mut())
    };
    (st.is_ok() && info.pid != 0).then_some(info.working_set as u64)
}

pub struct Memory {
    /// Installed RAM per the SMBIOS memory tables; fixed for the session.
    installed: Option<u64>,
}

impl Memory {
    pub fn new() -> Self {
        let mut kb = 0u64;
        // SAFETY: `kb` is a valid out-pointer for the duration of the call.
        let ok = unsafe { GetPhysicallyInstalledSystemMemory(&mut kb) }.is_ok();
        Self { installed: ok.then(|| kb.saturating_mul(1024)) }
    }
}

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
            compressed: compressed(),
            modified: pages(|l| l.modified),
            standby: pages(|l| l.standby_by_priority.iter().fold(0, |a, &b| a.saturating_add(b))),
            free: pages(|l| l.free.saturating_add(l.zero)),
            paged_pool: pi.map(|p| (p.KernelPaged as u64).saturating_mul(page)),
            nonpaged_pool: pi.map(|p| (p.KernelNonpaged as u64).saturating_mul(page)),
            // Without SMBIOS memory tables (some VMs) installed can be below total: report nothing, not 0.
            hardware_reserved: self.installed.and_then(|i| i.checked_sub(ms.ullTotalPhys)),
        });
    }
}
