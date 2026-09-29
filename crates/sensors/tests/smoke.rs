//! Runs both sources against the real machine. Hardware-dependent fields are only range-checked.

use busy_core::{Module, Snapshot, SourceOptions};
use std::time::Duration;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

#[test]
fn sources_sample_without_panicking() {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok().unwrap();
    let mut sources = busy_sensors::sources();
    let modules: Vec<Module> = sources.iter().map(|s| s.module()).collect();
    assert_eq!(modules, [Module::Gpu, Module::Sensors]);

    // Defaults first (third-party tools off), then opted in: both paths must run clean.
    let mut opts = SourceOptions::default();
    let mut snap = Snapshot::default();
    for i in 0..4 {
        if i == 2 {
            opts.third_party_sensors = true;
        }
        snap = Snapshot::default();
        for s in &mut sources {
            s.configure(opts);
            s.sample(&mut snap);
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    for g in &snap.gpus {
        assert!((0.0..=100.0).contains(&g.util_pct), "{g:?}");
        assert!(g.engines.iter().all(|(_, v)| (0.0..=100.0).contains(v)));
        assert!(g.temp_c.is_none_or(|t| (-20.0..150.0).contains(&t)));
        let dv = g.driver_version.as_deref().unwrap_or("0.0.0.0");
        assert!(dv.split('.').count() == 4 && dv.split('.').all(|p| p.parse::<u16>().is_ok()), "{g:?}");
        assert!(g.feature_level.is_none_or(|(major, minor)| (9..=15).contains(&major) && minor <= 15), "{g:?}");
    }
    assert!(snap.sensors.iter().all(|r| r.value.is_finite()));
}
