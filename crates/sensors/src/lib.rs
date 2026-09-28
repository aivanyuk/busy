//! GPU metrics (DXGI + PDH + D3DKMT).

mod gpu;

use busy_core::Source;

/// Called on the sampler thread after `CoInitializeEx(COINIT_MULTITHREADED)`.
pub fn sources() -> Vec<Box<dyn Source>> {
    vec![Box::new(gpu::GpuSource::new())]
}
