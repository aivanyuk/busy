use crate::util::perf_info;
use busy_core::{MemInfo, Module, Snapshot, Source};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

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
        snap.memory = Some(MemInfo {
            total: ms.ullTotalPhys,
            used: ms.ullTotalPhys - ms.ullAvailPhys,
            available: ms.ullAvailPhys,
            // ullTotalPageFile is the commit limit, ullAvailPageFile what remains of it.
            commit_used: pi.map_or(ms.ullTotalPageFile - ms.ullAvailPageFile, |p| p.CommitTotal as u64 * page),
            commit_limit: pi.map_or(ms.ullTotalPageFile, |p| p.CommitLimit as u64 * page),
            cached: pi.map(|p| p.SystemCache as u64 * page),
            compressed: None,
        });
    }
}
