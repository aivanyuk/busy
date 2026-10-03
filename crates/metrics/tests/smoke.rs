//! Runs every source against the real machine: no panics, sane ranges.

use busy_core::{Module, NetKind, NetRank, Snapshot, SourceOptions, TOP_N};
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
    for s in &mut sources {
        s.configure(SourceOptions { network_processes: true, ..SourceOptions::default() });
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
    for d in &snap.disks {
        assert!((0.0..=100.0).contains(&d.active_pct), "{d:?}");
        assert!(d.avg_response_ms.is_none_or(|ms| ms.is_finite() && ms >= 0.0), "{d:?}");
        // Since-start totals stay far below a since-boot count after two samples 300 ms apart.
        assert!(d.read_total.is_none_or(|b| b < 1 << 40) && d.written_total.is_none_or(|b| b < 1 << 40), "{d:?}");
    }
    assert!(snap.volumes.iter().all(|v| v.free <= v.total));
    assert!(snap.volumes.iter().filter(|v| v.is_system).count() <= 1);
    for v in &snap.volumes {
        assert!(v.disk_index.is_none_or(|i| snap.disks.iter().any(|d| d.index == Some(i))), "{v:?}");
    }
    let net = snap.net.expect("net");
    for i in &net.interfaces {
        let Some(w) = &i.wifi else { continue };
        assert!(i.connected && i.kind == NetKind::Wifi, "{i:?}");
        assert!(w.signal_pct <= 100 && w.rssi_dbm.is_none_or(|r| (-120..=0).contains(&r)), "{i:?}");
        assert!(w.channel_mhz.is_none_or(|f| (2400..=7200).contains(&f)), "{i:?}");
    }
    assert!(!snap.top.by_mem.is_empty());
    assert!(snap.top.by_cpu.iter().all(|p| (0.0..=100.0).contains(&p.cpu_pct)));
    // A runner may have no open connection at all; whatever is listed is live and in order.
    let net = &snap.top.by_net;
    assert_eq!(snap.top.net_rank, NetRank::Connections);
    assert!(net.len() <= TOP_N && net.iter().all(|p| p.connections > 0 && !p.name.is_empty()), "{net:?}");
    assert!(net.windows(2).all(|w| w[0].connections >= w[1].connections), "{net:?}");
}
