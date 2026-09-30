//! Shared types between collectors (`busy-metrics`, `busy-sensors`) and UI (`busy`, `busy-settings`).

mod config;
mod migrate;
mod opt_in;
mod options;
pub mod release;
pub use config::*;
pub use migrate::CONFIG_VERSION;
pub use opt_in::*;
pub use options::*;

/// A data collector. Constructed and driven exclusively on the sampler thread,
/// which has called `CoInitializeEx(COINIT_MULTITHREADED)` beforehand.
/// Rates (bytes/s, %) are computed internally from deltas between calls;
/// the first call may leave rate fields at 0.
pub trait Source {
    fn module(&self) -> Module;
    /// Called before the first `sample` and whenever the options change, whether or not the module is active.
    /// For sources that gate or tune themselves on a setting (e.g. an opt-in reader). Must be cheap (defer I/O
    /// to the next `sample`) and must not panic; the default ignores the options.
    fn configure(&mut self, _opts: SourceOptions) {}
    /// Fill this source's part of `snap`. Must not panic; on failure leave fields untouched.
    fn sample(&mut self, snap: &mut Snapshot);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Module {
    Cpu,
    Memory,
    Disk,
    Network,
    Gpu,
    Battery,
    Sensors,
    Processes,
}

impl Module {
    pub const ALL: [Module; 8] = [
        Module::Cpu,
        Module::Memory,
        Module::Disk,
        Module::Network,
        Module::Gpu,
        Module::Battery,
        Module::Sensors,
        Module::Processes,
    ];

    /// Modules whose data this module's flyout shows besides its own (design `fly()`): top processes for CPU,
    /// Memory and Disk (GPU's per-process list comes from the GPU source), temperatures for CPU and Disk.
    pub fn flyout_needs(self) -> &'static [Module] {
        match self {
            Module::Cpu | Module::Disk => &[Module::Processes, Module::Sensors],
            Module::Memory => &[Module::Processes],
            _ => &[],
        }
    }

    /// Position in `Module::ALL`, for per-module tables (`[T; Module::ALL.len()]`).
    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            Module::Cpu => "CPU",
            Module::Memory => "Memory",
            Module::Disk => "Disk",
            Module::Network => "Network",
            Module::Gpu => "GPU",
            Module::Battery => "Battery",
            Module::Sensors => "Sensors",
            Module::Processes => "Processes",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub cpu: Option<CpuInfo>,
    pub memory: Option<MemInfo>,
    pub disks: Vec<DiskInfo>,
    pub volumes: Vec<VolumeInfo>,
    pub net: Option<NetInfo>,
    pub gpus: Vec<GpuInfo>,
    pub battery: Option<BatteryInfo>,
    pub sensors: Vec<SensorReading>,
    pub top: TopProcesses,
}

impl Snapshot {
    /// Drops the readings `m`'s source writes, so a snapshot kept across ticks can be refilled one module at a
    /// time and a module that stopped being sampled shows nothing rather than stale data. The GPU source also
    /// owns per-process GPU usage (`top.by_gpu`); Processes owns the other process lists.
    pub fn clear(&mut self, m: Module) {
        match m {
            Module::Cpu => self.cpu = None,
            Module::Memory => self.memory = None,
            Module::Disk => {
                self.disks.clear();
                self.volumes.clear();
            }
            Module::Network => self.net = None,
            Module::Gpu => {
                self.gpus.clear();
                self.top.by_gpu.clear();
            }
            Module::Battery => self.battery = None,
            Module::Sensors => self.sensors.clear(),
            Module::Processes => {
                self.top.by_cpu.clear();
                self.top.by_mem.clear();
                self.top.by_disk.clear();
            }
        }
    }
}

/// Percentages are 0..=100.
#[derive(Clone, Debug, Default)]
pub struct CpuInfo {
    pub name: String,
    pub total: f32,
    pub user: f32,
    pub kernel: f32,
    pub per_core: Vec<f32>,
    pub freq_mhz: Option<u32>,
    pub logical_cores: u32,
    pub physical_cores: u32,
    pub processes: u32,
    pub threads: u32,
    pub handles: u32,
    pub uptime_secs: u64,
}

/// Bytes.
#[derive(Clone, Debug, Default)]
pub struct MemInfo {
    pub total: u64,
    /// `total - available`; includes the modified list (see [`MemInfo::in_use`]).
    pub used: u64,
    pub available: u64,
    pub commit_used: u64,
    pub commit_limit: u64,
    pub cached: Option<u64>,
    /// RAM held by the memory compression store (Task Manager "Compressed"; Memory Compression working set).
    pub compressed: Option<u64>,
    /// Modified page list: dirty pages not yet written out (Task Manager "Modified").
    pub modified: Option<u64>,
    /// Standby list, all priorities (Task Manager "Standby"); part of `available`.
    pub standby: Option<u64>,
    /// Free + zeroed page lists (Task Manager "Free"); part of `available`.
    pub free: Option<u64>,
    /// Kernel paged / non-paged pool (Task Manager "Paged pool", "Non-paged pool").
    pub paged_pool: Option<u64>,
    pub nonpaged_pool: Option<u64>,
    /// Installed RAM not visible to Windows (firmware, iGPU carve-out): installed − `total`.
    pub hardware_reserved: Option<u64>,
}

impl MemInfo {
    /// Task Manager "In use": `total` minus modified, standby and free, so the four segments sum to `total`.
    pub fn in_use(&self) -> Option<u64> {
        let lists = self.modified?.saturating_add(self.standby?).saturating_add(self.free?);
        Some(self.total.saturating_sub(lists))
    }
}

/// Physical disk throughput.
#[derive(Clone, Debug, Default)]
pub struct DiskInfo {
    pub name: String,
    pub read_bps: f64,
    pub write_bps: f64,
    pub active_pct: f32,
    /// Mean time per transfer over the last interval, ms (Task Manager "Average response time").
    pub avg_response_ms: Option<f32>,
    /// Bytes read / written since the disk was first sampled (busy start or hot-plug).
    pub read_total: Option<u64>,
    pub written_total: Option<u64>,
    /// Physical disk number (`\\.\PhysicalDriveN`), the leading number of `name`.
    pub index: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct VolumeInfo {
    /// Drive letter with colon, e.g. "C:".
    pub mount: String,
    pub label: String,
    pub total: u64,
    pub free: u64,
    /// `DiskInfo.index` of the physical disk holding this volume (the lowest one if it spans several).
    pub disk_index: Option<u32>,
    /// Holds the Windows directory.
    pub is_system: bool,
}

#[derive(Clone, Debug, Default)]
pub struct NetInfo {
    /// Sum over physical/connected interfaces (excludes loopback/virtual where detectable).
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_total: u64,
    pub tx_total: u64,
    pub interfaces: Vec<NetIf>,
}

#[derive(Clone, Debug, Default)]
pub struct NetIf {
    pub name: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub ipv4: Vec<String>,
    pub link_speed_bps: u64,
    pub connected: bool,
    pub kind: NetKind,
    /// Set for a connected `NetKind::Wifi` interface. Read on connect and every 60 s; `rssi_dbm` every 5 s.
    pub wifi: Option<WifiInfo>,
}

/// Medium of a network interface, for picking the flyout's interface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NetKind {
    /// Physical Ethernet NIC. Virtual Ethernet (Hyper-V vEthernet, TAP) is `Other`.
    Ethernet,
    Wifi,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WifiInfo {
    pub ssid: String,
    /// Driver-reported link quality 0..=100 (netsh "Signal"); not derived from RSSI.
    pub signal_pct: u8,
    /// Received signal strength, dBm (negative, e.g. −52).
    pub rssi_dbm: Option<i32>,
    /// Center frequency of the associated channel, MHz (2.4 GHz: 2412–2484, 5 GHz: 5160–5885, 6 GHz: 5955–7115).
    pub channel_mhz: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct GpuInfo {
    pub name: String,
    pub luid: u64,
    pub util_pct: f32,
    /// e.g. ("3D", 42.0), ("Copy", 1.0), ("VideoDecode", 0.0)
    pub engines: Vec<(String, f32)>,
    pub vram_used: u64,
    pub vram_total: u64,
    pub shared_used: u64,
    pub temp_c: Option<f32>,
    pub hotspot_c: Option<f32>,
    pub fan_rpm: Option<u32>,
    pub fan_pct: Option<f32>,
    pub power_w: Option<f32>,
    pub core_clock_mhz: Option<u32>,
    pub mem_clock_mhz: Option<u32>,
    /// User-mode driver version as Task Manager shows it, e.g. "32.0.16.1692".
    pub driver_version: Option<String>,
    /// Highest Direct3D feature level as (major, minor), e.g. (12, 2) for FL 12_2.
    pub feature_level: Option<(u8, u8)>,
}

#[derive(Clone, Debug, Default)]
pub struct BatteryInfo {
    pub percent: f32,
    pub charging: bool,
    pub ac_online: bool,
    pub secs_remaining: Option<u32>,
    /// Positive = charging, negative = discharging.
    pub rate_mw: Option<i32>,
    pub full_capacity_mwh: Option<u32>,
    pub design_capacity_mwh: Option<u32>,
    pub cycle_count: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorKind {
    Temperature,
    Fan,
    Power,
    Voltage,
    Clock,
    Load,
    Other,
}

#[derive(Clone, Debug)]
pub struct SensorReading {
    /// "LibreHardwareMonitor", "HWiNFO", "NVML", "ADL", "ACPI"
    pub source: String,
    pub hardware: String,
    pub name: String,
    pub kind: SensorKind,
    /// °C, RPM, W, V, MHz, %
    pub value: f32,
}

#[derive(Clone, Debug, Default)]
pub struct TopProcesses {
    pub by_cpu: Vec<ProcEntry>,
    pub by_mem: Vec<ProcEntry>,
    pub by_disk: Vec<ProcEntry>,
    pub by_gpu: Vec<ProcEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct ProcEntry {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub mem_bytes: u64,
    pub io_bps: f64,
    pub gpu_pct: f32,
}

pub const TOP_N: usize = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_the_position_in_all() {
        for (i, m) in Module::ALL.into_iter().enumerate() {
            assert_eq!(m.index(), i, "{m:?}");
        }
    }

    #[test]
    fn clear_drops_only_that_module() {
        let proc = || vec![ProcEntry::default()];
        let full = || Snapshot {
            cpu: Some(CpuInfo::default()),
            memory: Some(MemInfo::default()),
            disks: vec![DiskInfo::default()],
            volumes: vec![VolumeInfo::default()],
            net: Some(NetInfo::default()),
            gpus: vec![GpuInfo::default()],
            battery: Some(BatteryInfo::default()),
            sensors: vec![SensorReading {
                source: String::new(),
                hardware: String::new(),
                name: String::new(),
                kind: SensorKind::Other,
                value: 0.0,
            }],
            top: TopProcesses { by_cpu: proc(), by_mem: proc(), by_disk: proc(), by_gpu: proc() },
        };
        // What each module's readings look like: (cpu, memory, disks+volumes, net, gpus+by_gpu, battery, sensors,
        // by_cpu+by_mem+by_disk).
        let shape = |s: &Snapshot| {
            [
                s.cpu.is_some(),
                s.memory.is_some(),
                !s.disks.is_empty() && !s.volumes.is_empty(),
                s.net.is_some(),
                !s.gpus.is_empty() && !s.top.by_gpu.is_empty(),
                s.battery.is_some(),
                !s.sensors.is_empty(),
                !s.top.by_cpu.is_empty() && !s.top.by_mem.is_empty() && !s.top.by_disk.is_empty(),
            ]
        };
        for (i, m) in Module::ALL.into_iter().enumerate() {
            let mut s = full();
            s.clear(m);
            let mut want = [true; Module::ALL.len()];
            want[i] = false;
            assert_eq!(shape(&s), want, "{m:?}");
            // Nothing of the module is left, not just part of it.
            let partial = [s.disks.is_empty() != s.volumes.is_empty(), s.gpus.is_empty() != s.top.by_gpu.is_empty()];
            assert_eq!(partial, [false, false], "{m:?}");
        }
    }

    #[test]
    fn mem_in_use() {
        let m = MemInfo { total: 100, modified: Some(5), standby: Some(30), free: Some(20), ..Default::default() };
        assert_eq!(m.in_use(), Some(45));
        assert_eq!(MemInfo { free: None, ..m.clone() }.in_use(), None);
        assert_eq!(MemInfo { free: Some(u64::MAX), ..m }.in_use(), Some(0));
    }
}
