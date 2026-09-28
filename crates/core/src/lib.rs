//! Shared types between collectors (`busy-metrics`, `busy-sensors`) and UI (`busy`, `busy-settings`).

/// A data collector. Constructed and driven exclusively on the sampler thread,
/// which has called `CoInitializeEx(COINIT_MULTITHREADED)` beforehand.
/// Rates (bytes/s, %) are computed internally from deltas between calls;
/// the first call may leave rate fields at 0.
pub trait Source {
    fn module(&self) -> Module;
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
    pub used: u64,
    pub available: u64,
    pub commit_used: u64,
    pub commit_limit: u64,
    pub cached: Option<u64>,
    pub compressed: Option<u64>,
}

/// Physical disk throughput.
#[derive(Clone, Debug, Default)]
pub struct DiskInfo {
    pub name: String,
    pub read_bps: f64,
    pub write_bps: f64,
    pub active_pct: f32,
}

#[derive(Clone, Debug, Default)]
pub struct VolumeInfo {
    pub mount: String,
    pub label: String,
    pub total: u64,
    pub free: u64,
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
