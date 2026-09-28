#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::time::{Duration, Instant};

use busy_core::Snapshot;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

fn main() {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok().expect("CoInitializeEx") };
    let t = Instant::now();
    let mut sources = busy_sensors::sources();
    println!("construct: {:?}", t.elapsed());
    let mut snap = Snapshot::default();
    for i in 0..5 {
        snap = Snapshot::default();
        for s in &mut sources {
            let t = Instant::now();
            s.sample(&mut snap);
            println!("sample {i} {:?}: {:?}", s.module(), t.elapsed());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    println!("gpus: {:#?}", snap.gpus);
    println!("sensors: {:#?}", snap.sensors);
    println!("top.by_gpu: {:#?}", snap.top.by_gpu);
}
