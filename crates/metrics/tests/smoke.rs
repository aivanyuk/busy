//! Runs every source against the real machine: no panics, sane ranges.

use busy_core::{Module, Snapshot};
use std::time::Duration;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

#[test]
fn sources_sample_without_panicking() {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok().unwrap();
    let mut sources = busy_metrics::sources();
    let modules: Vec<Module> = sources.iter().map(|s| s.module()).collect();
    for m in [Module::Cpu, Module::Memory, Module::Disk, Module::Network, Module::Battery, Module::Processes] {
        assert!(modules.contains(&m), "missing source for {m:?}");
    }

    let mut snap = Snapshot::default();
    for _ in 0..2 {
        snap = Snapshot::default();
        for s in &mut sources {
            s.sample(&mut snap);
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    let cpu = snap.cpu.expect("cpu");
    assert!((0.0..=100.0).contains(&cpu.total));
    assert!(cpu.logical_cores > 0 && cpu.per_core.len() == cpu.logical_cores as usize);
    let mem = snap.memory.expect("memory");
    assert!(mem.total > 0 && mem.used <= mem.total);
    let parts =
        [mem.modified, mem.standby, mem.free, mem.paged_pool, mem.nonpaged_pool, mem.hardware_reserved, mem.compressed];
    for list in parts.into_iter().flatten() {
        assert!(list <= mem.total, "{mem:?}");
    }
    assert!(mem.in_use().is_none_or(|u| u > 0 && u <= mem.total), "{mem:?}");
    assert!(snap.volumes.iter().all(|v| v.free <= v.total));
    assert!(!snap.top.by_mem.is_empty());
    assert!(snap.top.by_cpu.iter().all(|p| (0.0..=100.0).contains(&p.cpu_pct)));
}
