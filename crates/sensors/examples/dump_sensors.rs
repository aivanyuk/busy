#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use busy_core::Snapshot;
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
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok().expect("CoInitializeEx") };
    let t = Instant::now();
    let mut sources = busy_sensors::sources();
    println!("construct: {:?}", t.elapsed());
    let n: u32 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(5);
    let mut snap = Snapshot::default();
    for i in 0..n {
        snap = Snapshot::default();
        for s in &mut sources {
            let (t, a) = (Instant::now(), ALLOCS.load(Ordering::Relaxed));
            s.sample(&mut snap);
            let (t, a) = (t.elapsed(), ALLOCS.load(Ordering::Relaxed) - a);
            println!("sample {i} {:?}: {t:?} {a} allocs", s.module());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    println!("gpus: {:#?}", snap.gpus);
    println!("sensors: {:#?}", snap.sensors);
    println!("top.by_gpu: {:#?}", snap.top.by_gpu);
}
