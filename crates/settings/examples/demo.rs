#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Standalone host for the settings window, with fixed synthetic readings for the preview and the machine
//! lists. `BUSY_FORCE_DARK=1` forces dark mode. Applied configs are printed, not saved.
// link.exe's manifest schema predates <dpiAwareness> and warns (81010002); the element is still embedded.
#![allow(linker_messages)]

use busy_core::*;
use busy_settings::Host;
use busy_ui::history::History;
use std::cell::RefCell;
use std::rc::Rc;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, TranslateMessage};

struct Demo {
    snap: Snapshot,
    hist: RefCell<History>,
}

impl Host for Demo {
    fn apply(&self, cfg: Config) {
        println!("{cfg:#?}");
        self.hist.borrow_mut().resize(&cfg);
    }

    fn shown(&self, modules: &[Module]) {
        println!("shown {modules:?}");
    }

    fn with_data(&self, f: &mut dyn FnMut(&Snapshot, &History)) {
        f(&self.snap, &self.hist.borrow());
    }
}

fn snapshot(t: f32) -> Snapshot {
    let wave = |base: f32, amp: f32| base + amp * (t * 0.7).sin();
    let temp = |hardware: &str, name: &str, value| SensorReading {
        source: "demo".into(),
        hardware: hardware.into(),
        name: name.into(),
        kind: SensorKind::Temperature,
        value,
    };
    Snapshot {
        cpu: Some(CpuInfo {
            total: wave(35.0, 20.0),
            per_core: (0..16).map(|i| wave(20.0 + i as f32 * 4.0, 15.0)).collect(),
            ..Default::default()
        }),
        memory: Some(MemInfo { total: 32 << 30, used: 13 << 30, ..Default::default() }),
        disks: vec![DiskInfo { read_bps: 2.4e6, write_bps: 3.1e5, ..Default::default() }],
        volumes: vec![
            VolumeInfo {
                mount: "C:".into(),
                label: "Windows".into(),
                total: 1 << 40,
                free: 3 << 38,
                disk_index: Some(0),
                is_system: true,
            },
            VolumeInfo {
                mount: "E:".into(),
                label: "Backup".into(),
                total: 2 << 40,
                free: 1 << 40,
                disk_index: Some(1),
                is_system: false,
            },
        ],
        net: Some(NetInfo {
            rx_bps: wave(3.2e6, 1.5e6) as f64,
            tx_bps: wave(2.4e5, 1e5) as f64,
            interfaces: vec![NetIf {
                name: "Wi-Fi".into(),
                connected: true,
                kind: NetKind::Wifi,
                ..Default::default()
            }],
            ..Default::default()
        }),
        gpus: vec![GpuInfo { name: "Demo GPU".into(), util_pct: wave(12.0, 8.0), ..Default::default() }],
        battery: Some(BatteryInfo { percent: 76.0, secs_remaining: Some(9000), ..Default::default() }),
        sensors: vec![temp("CPU", "Package", wave(58.0, 6.0)), temp("NVMe", "Composite", 41.0)],
        ..Default::default()
    }
}

fn main() {
    let cfg = Config::load();
    let mut hist = History::new(&cfg);
    let fresh = [true; Module::ALL.len()];
    for i in 0..40 {
        hist.push(&snapshot(i as f32), &fresh, &cfg);
    }
    let demo = Rc::new(Demo { snap: snapshot(40.0), hist: RefCell::new(hist) });
    busy_settings::open(HWND::default(), &cfg, demo.clone(), None);
    // What the app does after each sample: fills the drive, adapter and sensor lists.
    busy_settings::refresh(&demo.snap, &demo.hist.borrow());
    let mut msg = MSG::default();
    while busy_settings::is_open() && unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        if !busy_settings::is_dialog_message(&msg) {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
