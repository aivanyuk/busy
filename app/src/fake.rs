//! Random-walk data source filling every Snapshot field; selected with `BUSY_FAKE=1` for UI work on machines
//! without a GPU, battery or sensors.

use busy_core::*;

pub fn sources() -> Vec<Box<dyn Source>> {
    Module::ALL.iter().enumerate().map(|(i, &m)| Box::new(Fake::new(m, i as u64 + 1)) as Box<dyn Source>).collect()
}

struct Fake {
    module: Module,
    rng: u64,
    walks: Vec<f32>,
    ticks: u64,
    totals: [u64; 2],
}

impl Fake {
    fn new(module: Module, seed: u64) -> Self {
        Self {
            module,
            rng: 0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0xA24B_AED4_963E_E407),
            walks: Vec::new(),
            ticks: 0,
            totals: [0; 2],
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Bounded random walk #`i` in `lo..=hi` moving at most `step` per tick.
    fn walk(&mut self, i: usize, lo: f32, hi: f32, step: f32) -> f32 {
        if self.walks.len() <= i {
            self.walks.resize(i + 1, f32::NAN);
        }
        let r = self.rand();
        let cur = if self.walks[i].is_nan() { lo + (hi - lo) * r } else { self.walks[i] };
        let v = (cur + (self.rand() - 0.5) * 2.0 * step).clamp(lo, hi);
        self.walks[i] = v;
        v
    }

    /// Bursty rate: mostly low, occasional spikes.
    fn burst(&mut self, i: usize, base: f32, peak: f32) -> f64 {
        let b = self.walk(i, 0.0, base, base * 0.3);
        let spike = if self.rand() > 0.85 { self.rand() * peak } else { 0.0 };
        (b + spike) as f64
    }
}

const GB: u64 = 1 << 30;
const MB: f32 = 1024.0 * 1024.0;

impl Source for Fake {
    fn module(&self) -> Module {
        self.module
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        self.ticks += 1;
        match self.module {
            Module::Cpu => {
                let per_core: Vec<f32> = (0..16).map(|i| self.walk(i, 0.0, 100.0, 18.0)).collect();
                let total = per_core.iter().sum::<f32>() / per_core.len() as f32;
                let kernel = total * self.walk(20, 0.15, 0.35, 0.05);
                snap.cpu = Some(CpuInfo {
                    name: "Fake Ryzen 9 7950X 16-Core Processor".into(),
                    total,
                    user: total - kernel,
                    kernel,
                    per_core,
                    freq_mhz: Some(self.walk(21, 3000.0, 5400.0, 300.0) as u32),
                    logical_cores: 16,
                    physical_cores: 8,
                    processes: self.walk(22, 280.0, 340.0, 3.0) as u32,
                    threads: self.walk(23, 3800.0, 4600.0, 40.0) as u32,
                    handles: self.walk(24, 120_000.0, 140_000.0, 900.0) as u32,
                    uptime_secs: 3 * 86400 + 4 * 3600 + self.ticks,
                });
            }
            Module::Memory => {
                let total = 32 * GB;
                let used = (self.walk(0, 0.35, 0.9, 0.02) * total as f32) as u64;
                snap.memory = Some(MemInfo {
                    total,
                    used,
                    available: total - used,
                    commit_used: used + 4 * GB,
                    commit_limit: total + 8 * GB,
                    cached: Some((self.walk(1, 3.0, 9.0, 0.2) * GB as f32) as u64),
                    compressed: Some((self.walk(2, 0.1, 1.5, 0.05) * GB as f32) as u64),
                    ..Default::default()
                });
            }
            Module::Disk => {
                snap.disks = ["Samsung SSD 990 PRO 2TB", "WDC WD40EFRX-68N32N0"]
                    .iter()
                    .enumerate()
                    .map(|(i, n)| {
                        let (r, w) =
                            (self.burst(i * 3, 2.0 * MB, 400.0 * MB), self.burst(i * 3 + 1, 1.0 * MB, 150.0 * MB));
                        DiskInfo {
                            name: (*n).into(),
                            read_bps: r,
                            write_bps: w,
                            active_pct: self.walk(i * 3 + 2, 0.0, 60.0, 10.0),
                        }
                    })
                    .collect();
                snap.volumes = vec![
                    VolumeInfo { mount: "C:".into(), label: "Windows".into(), total: 1862 * GB, free: 713 * GB },
                    VolumeInfo { mount: "D:".into(), label: "Data".into(), total: 3726 * GB, free: 402 * GB },
                ];
            }
            Module::Network => {
                let (eth_rx, eth_tx) =
                    (self.burst(0, 400.0 * 1024.0, 60.0 * MB), self.burst(1, 60.0 * 1024.0, 8.0 * MB));
                let (wifi_rx, wifi_tx) = (self.burst(2, 20.0 * 1024.0, MB), self.burst(3, 5.0 * 1024.0, 0.2 * MB));
                let (rx, tx) = (eth_rx + wifi_rx, eth_tx + wifi_tx);
                self.totals[0] += rx as u64;
                self.totals[1] += tx as u64;
                snap.net = Some(NetInfo {
                    rx_bps: rx,
                    tx_bps: tx,
                    rx_total: 18 * GB + self.totals[0],
                    tx_total: 2 * GB + self.totals[1],
                    interfaces: vec![
                        NetIf {
                            name: "Ethernet".into(),
                            rx_bps: eth_rx,
                            tx_bps: eth_tx,
                            ipv4: vec!["192.168.1.23".into()],
                            link_speed_bps: 2_500_000_000,
                            connected: true,
                        },
                        NetIf {
                            name: "Wi-Fi".into(),
                            rx_bps: wifi_rx,
                            tx_bps: wifi_tx,
                            ipv4: vec!["10.0.0.57".into()],
                            link_speed_bps: 866_000_000,
                            connected: true,
                        },
                    ],
                });
            }
            Module::Gpu => {
                let util = self.walk(0, 0.0, 100.0, 15.0);
                let igpu = self.walk(1, 0.0, 30.0, 5.0);
                snap.gpus = vec![
                    GpuInfo {
                        name: "Fake GeForce RTX 4080".into(),
                        luid: 1,
                        util_pct: util,
                        engines: vec![
                            ("3D".into(), util),
                            ("Copy".into(), self.walk(2, 0.0, 10.0, 2.0)),
                            ("Video Decode".into(), self.walk(3, 0.0, 25.0, 5.0)),
                            ("Compute".into(), self.walk(4, 0.0, 40.0, 8.0)),
                        ],
                        vram_used: (self.walk(5, 2.0, 12.0, 0.3) * GB as f32) as u64,
                        vram_total: 16 * GB,
                        shared_used: 300 << 20,
                        temp_c: Some(self.walk(6, 38.0, 78.0, 2.0)),
                        hotspot_c: Some(self.walk(7, 45.0, 90.0, 2.0)),
                        fan_rpm: Some(self.walk(8, 800.0, 2200.0, 60.0) as u32),
                        fan_pct: Some(self.walk(9, 30.0, 80.0, 3.0)),
                        power_w: Some(self.walk(10, 20.0, 320.0, 25.0)),
                        core_clock_mhz: Some(self.walk(11, 210.0, 2800.0, 200.0) as u32),
                        mem_clock_mhz: Some(11200),
                    },
                    GpuInfo {
                        name: "Fake Radeon Graphics".into(),
                        luid: 2,
                        util_pct: igpu,
                        engines: vec![("3D".into(), igpu)],
                        vram_used: 256 << 20,
                        vram_total: 512 << 20,
                        shared_used: (self.walk(12, 0.2, 1.5, 0.1) * GB as f32) as u64,
                        ..Default::default()
                    },
                ];
            }
            Module::Battery => {
                let pct = self.walk(0, 20.0, 100.0, 0.5);
                let charging = self.walk(1, 0.0, 1.0, 0.05) > 0.5;
                snap.battery = Some(BatteryInfo {
                    percent: pct,
                    charging,
                    ac_online: charging,
                    secs_remaining: (!charging).then_some((pct * 180.0) as u32),
                    rate_mw: Some(if charging { 25_000 } else { -(self.walk(2, 6000.0, 18000.0, 800.0) as i32) }),
                    full_capacity_mwh: Some(52_300),
                    design_capacity_mwh: Some(57_000),
                    cycle_count: Some(214),
                });
            }
            Module::Sensors => {
                let s = |hardware: &str, name: &str, kind, v| SensorReading {
                    source: "Fake".into(),
                    hardware: hardware.into(),
                    name: name.into(),
                    kind,
                    value: v,
                };
                let cpu = "Fake Ryzen 9 7950X";
                let mut out = vec![
                    s(cpu, "CPU Package", SensorKind::Temperature, 0.0),
                    s(cpu, "Core (Tctl/Tdie)", SensorKind::Temperature, 0.0),
                    s(cpu, "CPU Package Power", SensorKind::Power, 0.0),
                    s("Motherboard", "CPU Fan", SensorKind::Fan, 0.0),
                    s("Motherboard", "System Fan #1", SensorKind::Fan, 0.0),
                    s("Motherboard", "Vcore", SensorKind::Voltage, 0.0),
                    s("Samsung SSD 990 PRO", "Temperature", SensorKind::Temperature, 0.0),
                ];
                let ranges = [
                    (40.0, 88.0, 3.0),
                    (42.0, 92.0, 3.0),
                    (15.0, 160.0, 12.0),
                    (600.0, 1800.0, 50.0),
                    (500.0, 1100.0, 30.0),
                    (0.9, 1.35, 0.02),
                    (35.0, 55.0, 0.5),
                ];
                for (i, (r, (lo, hi, st))) in out.iter_mut().zip(ranges).enumerate() {
                    r.value = self.walk(i, lo, hi, st);
                }
                snap.sensors = out;
            }
            Module::Processes => {
                let names = [
                    "chrome.exe",
                    "Code.exe",
                    "explorer.exe",
                    "rustc.exe",
                    "Teams.exe",
                    "svchost.exe",
                    "dwm.exe",
                    "firefox.exe",
                ];
                let procs: Vec<ProcEntry> = names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| ProcEntry {
                        pid: 1000 + i as u32 * 4,
                        name: (*n).into(),
                        cpu_pct: self.walk(i * 4, 0.0, 30.0, 4.0),
                        mem_bytes: (self.walk(i * 4 + 1, 50.0, 2500.0, 40.0) * MB) as u64,
                        io_bps: self.burst(i * 4 + 2, 100.0 * 1024.0, 20.0 * MB),
                        gpu_pct: self.walk(i * 4 + 3, 0.0, 20.0, 3.0),
                    })
                    .collect();
                let top = |key: fn(&ProcEntry) -> f64| {
                    let mut v = procs.clone();
                    v.sort_by(|a, b| key(b).total_cmp(&key(a)));
                    v.truncate(TOP_N);
                    v
                };
                snap.top = TopProcesses {
                    by_cpu: top(|p| p.cpu_pct as f64),
                    by_mem: top(|p| p.mem_bytes as f64),
                    by_disk: top(|p| p.io_bps),
                    by_gpu: top(|p| p.gpu_pct as f64),
                };
            }
        }
    }
}
