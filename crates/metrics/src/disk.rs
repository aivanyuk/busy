use crate::pdh::{Counter, Query};
use crate::util::Every;
use busy_core::{DiskInfo, Module, Snapshot, Source, VolumeInfo};
use busy_win::from_wide;
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
};
use windows::Win32::System::WindowsProgramming::DRIVE_FIXED;
use windows::core::PCWSTR;

pub struct Disk {
    pdh: Option<Pdh>,
    reopen: Every,
    volumes: Vec<VolumeInfo>,
    vol_refresh: Every,
}

struct Pdh {
    q: Query,
    read: Counter,
    write: Counter,
    idle: Counter,
}

impl Pdh {
    fn open() -> Option<Self> {
        let q = Query::open()?;
        let add = |c: &str| q.add(&format!(r"\PhysicalDisk(*)\{c}"));
        let s = Self {
            read: add("Disk Read Bytes/sec")?,
            write: add("Disk Write Bytes/sec")?,
            idle: add("% Idle Time")?,
            q,
        };
        s.q.collect().then_some(s)
    }

    fn read(&self) -> Option<Vec<DiskInfo>> {
        if !self.q.collect() {
            return None;
        }
        // A handful of instances: a linear lookup beats building a map every tick.
        let (write, idle) = (self.write.array(0)?, self.idle.array(0)?);
        let get = |v: &[(String, f64)], name: &str| v.iter().find(|(n, _)| n == name).map(|&(_, x)| x);
        // Instances are "<disk#> <letters>" (e.g. "2 C: H:") plus "_Total".
        let mut disks: Vec<_> = self
            .read
            .array(0)?
            .into_iter()
            .filter(|(n, _)| n != "_Total")
            .map(|(name, r)| DiskInfo {
                read_bps: r,
                write_bps: get(&write, &name).unwrap_or(0.0),
                active_pct: get(&idle, &name).map_or(0.0, |i| (100.0 - i).clamp(0.0, 100.0) as f32),
                name,
            })
            .collect();
        disks.sort_by_key(|d| d.name.split(' ').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(u32::MAX));
        Some(disks)
    }
}

impl Disk {
    pub fn new() -> Self {
        Self { pdh: Pdh::open(), reopen: Every::default(), volumes: Vec::new(), vol_refresh: Every::default() }
    }
}

impl Source for Disk {
    fn module(&self) -> Module {
        Module::Disk
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        if self.pdh.is_none() && self.reopen.due(10) {
            self.pdh = Pdh::open();
        }
        match self.pdh.as_ref().map(Pdh::read) {
            Some(Some(d)) => snap.disks = d,
            Some(None) => {
                // Rebuild the query later in case the counter set was reset.
                self.pdh = None;
                self.reopen.arm();
            }
            None => {}
        }
        if self.vol_refresh.due(10) {
            self.volumes = volumes();
        }
        snap.volumes.clone_from(&self.volumes);
    }
}

fn volumes() -> Vec<VolumeInfo> {
    let mut buf = [0u16; 512];
    let n = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) } as usize;
    buf[..n.min(buf.len())]
        .split(|&c| c == 0)
        .filter(|r| !r.is_empty())
        .filter_map(|root| {
            let root: Vec<u16> = root.iter().copied().chain(Some(0)).collect();
            let p = PCWSTR(root.as_ptr());
            if unsafe { GetDriveTypeW(p) } != DRIVE_FIXED {
                return None;
            }
            let (mut total, mut free) = (0u64, 0u64);
            unsafe { GetDiskFreeSpaceExW(p, None, Some(&mut total), Some(&mut free)) }.ok()?;
            let mut label = [0u16; 261];
            let _ = unsafe { GetVolumeInformationW(p, Some(&mut label), None, None, None, None) };
            Some(VolumeInfo {
                mount: from_wide(&root).trim_end_matches('\\').to_owned(),
                label: from_wide(&label),
                total,
                free,
            })
        })
        .collect()
}
