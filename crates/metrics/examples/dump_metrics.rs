#![allow(clippy::expect_used, clippy::unwrap_used)]
use busy_core::Snapshot;
use std::time::{Duration, Instant};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

fn main() {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok().expect("CoInitializeEx");
    let t = Instant::now();
    let mut sources = busy_metrics::sources();
    println!("init: {:?}", t.elapsed());
    let n: u32 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(5);
    let mut snap = Snapshot::default();
    for i in 1..=n {
        std::thread::sleep(Duration::from_secs(1));
        snap = Snapshot::default();
        let times: Vec<_> = sources
            .iter_mut()
            .map(|s| {
                let t = Instant::now();
                s.sample(&mut snap);
                format!("{}={:?}", s.module().label(), t.elapsed())
            })
            .collect();
        println!("sample {i}: {}", times.join(" "));
    }
    println!("{snap:#?}");
}
