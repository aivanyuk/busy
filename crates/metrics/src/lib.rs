//! System metric collectors (CPU, memory, disk, network, battery, processes) built on Win32 APIs.

mod battery;
mod cpu;
mod disk;
mod memory;
mod network;
mod processes;
mod util;

use busy_core::Source;

/// Called on the sampler thread after `CoInitializeEx(COINIT_MULTITHREADED)`.
pub fn sources() -> Vec<Box<dyn Source>> {
    vec![
        Box::new(cpu::Cpu::new()),
        Box::new(memory::Memory::new()),
        Box::new(disk::Disk::new()),
        Box::new(network::Network::default()),
        Box::new(battery::Battery::default()),
        Box::new(processes::Processes::default()),
    ]
}
