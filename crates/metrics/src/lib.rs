//! System metric collectors (CPU, memory, disk, network, battery, processes) built on Win32 APIs.

mod cpu;
mod memory;
mod pdh;
mod util;

use busy_core::Source;

/// Called on the sampler thread after `CoInitializeEx(COINIT_MULTITHREADED)`.
pub fn sources() -> Vec<Box<dyn Source>> {
    vec![Box::new(cpu::Cpu::new()), Box::new(memory::Memory)]
}
