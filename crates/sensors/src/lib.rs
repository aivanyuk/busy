//! GPU metrics (DXGI + PDH + D3DKMT + NVML/ADL) and hardware sensors (LibreHardwareMonitor / HWiNFO).

mod adl;
mod gpu;
mod hwinfo;
mod lhm;
mod nvml;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use busy_core::{GpuInfo, Module, SensorKind, SensorReading, Snapshot, Source, SourceOptions};

/// Called on the sampler thread after `CoInitializeEx(COINIT_MULTITHREADED)`.
pub fn sources() -> Vec<Box<dyn Source>> {
    let shared = Rc::new(RefCell::new(Shared::default()));
    vec![Box::new(gpu::GpuSource::new(shared.clone())), Box::new(SensorsSource::new(shared))]
}

/// Vendor GPU readings handed from the GPU source to the sensors source (same thread, same sample).
#[derive(Default)]
pub(crate) struct Shared {
    vendor: Vec<SensorReading>,
    at: Option<Instant>,
}

pub(crate) fn reading(source: &str, hardware: &str, name: &str, kind: SensorKind, value: f32) -> SensorReading {
    SensorReading { source: source.into(), hardware: hardware.into(), name: name.into(), kind, value }
}

/// Latin-1 decode of a NUL-terminated byte buffer (HWiNFO ANSI strings; `°` is 0xB0 in cp1252 too).
pub(crate) fn ansi(b: &[u8]) -> String {
    b.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect::<String>().trim().to_owned()
}

struct SensorsSource {
    shared: Rc<RefCell<Shared>>,
    /// Only exists while `third_party_sensors` is on, so nothing can touch LHM/HWiNFO otherwise; dropping it
    /// releases the WMI connection.
    tools: Option<ThirdParty>,
}

/// Readers of data published by other programs the user installed.
struct ThirdParty {
    lhm: lhm::Lhm,
    hwinfo: hwinfo::HwInfo,
}

impl SensorsSource {
    fn new(shared: Rc<RefCell<Shared>>) -> Self {
        Self { shared, tools: None }
    }
}

impl Source for SensorsSource {
    fn module(&self) -> Module {
        Module::Sensors
    }

    fn configure(&mut self, opts: SourceOptions) {
        match (opts.third_party_sensors, self.tools.is_some()) {
            // Construction is free: LHM connects lazily and HWiNFO is opened on read.
            (true, false) => self.tools = Some(ThirdParty { lhm: lhm::Lhm::new(), hwinfo: hwinfo::HwInfo::new() }),
            (false, true) => self.tools = None,
            _ => {}
        }
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        // One third-party source only (avoids duplicates); LHM preferred.
        let mut out = Vec::new();
        if let Some(t) = &mut self.tools {
            out = t.lhm.read();
            if out.is_empty() {
                out = t.hwinfo.read();
            }
        }
        if out.is_empty() {
            let sh = self.shared.borrow();
            if sh.at.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
                out = sh.vendor.clone();
            }
        } else {
            fill_gpus(&mut snap.gpus, &out);
        }
        snap.sensors.extend(out);
    }
}

fn fill_gpus(gpus: &mut [GpuInfo], rs: &[SensorReading]) {
    for g in gpus {
        let name = g.name.to_lowercase();
        let mine = || rs.iter().filter(|r| r.hardware.to_lowercase().contains(&name));
        let temps = || mine().filter(|r| r.kind == SensorKind::Temperature);
        let is_hot = |r: &&SensorReading| {
            let n = r.name.to_lowercase();
            n.contains("hot spot") || n.contains("hotspot")
        };
        if g.hotspot_c.is_none() {
            g.hotspot_c = temps().find(is_hot).map(|r| r.value);
        }
        if g.temp_c.is_none() {
            g.temp_c = temps()
                .filter(|r| !is_hot(r))
                .find(|r| {
                    let n = r.name.to_lowercase();
                    n.contains("core") || n.contains("gpu temperature") || n == "gpu"
                })
                .or_else(|| temps().find(|r| !is_hot(r)))
                .map(|r| r.value);
        }
        if g.fan_rpm.is_none() {
            g.fan_rpm = mine().find(|r| r.kind == SensorKind::Fan).map(|r| r.value as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_party_tools_exist_only_while_opted_in() {
        let mut s = SensorsSource::new(Rc::default());
        assert!(s.tools.is_none());
        s.configure(SourceOptions::default());
        assert!(s.tools.is_none());
        s.configure(SourceOptions { third_party_sensors: true });
        assert!(s.tools.is_some());
        s.configure(SourceOptions { third_party_sensors: false });
        assert!(s.tools.is_none());
    }
}
