#![allow(clippy::expect_used, clippy::unwrap_used)]
use busy_core::{Snapshot, SourceOptions};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

/// Counts heap allocations (and reallocations), so a sample's cost shows its allocations too.
struct Counting;

static ALLOCS: AtomicU64 = AtomicU64::new(0);

// SAFETY: forwards every call unchanged to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as ours.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as ours.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as ours.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

fn main() {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok().expect("CoInitializeEx");
    let t = Instant::now();
    let mut sources = busy_metrics::sources();
    println!("init: {:?}", t.elapsed());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let n: u32 = args.iter().find_map(|a| a.parse().ok()).unwrap_or(5);
    // `--net`: rank processes by network use, as while the Network flyout is open; `--trace`: by traffic, as with
    // the opt-in on (needs administrator rights).
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let opts = SourceOptions {
        network_processes: has("--net") || has("--trace"),
        process_network: has("--trace"),
        ..SourceOptions::default()
    };
    for s in &mut sources {
        s.configure(opts);
    }
    let mut snap = Snapshot::default();
    for i in 1..=n {
        std::thread::sleep(Duration::from_secs(1));
        snap = Snapshot::default();
        let times: Vec<_> = sources
            .iter_mut()
            .map(|s| {
                let (t, a) = (Instant::now(), ALLOCS.load(Ordering::Relaxed));
                s.sample(&mut snap);
                let (t, a) = (t.elapsed(), ALLOCS.load(Ordering::Relaxed) - a);
                format!("{:?}={t:?}/{a}", s.module())
            })
            .collect();
        println!("sample {i}: {}", times.join(" "));
    }
    println!("{snap:#?}");
}
