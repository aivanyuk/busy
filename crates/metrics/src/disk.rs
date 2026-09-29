use crate::util::Every;
use busy_core::{DiskInfo, Module, Snapshot, Source, VolumeInfo};
use busy_win::from_wide;
use busy_win::pdh::{ArrayBuf, Counter, Query};
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
};
use windows::Win32::System::WindowsProgramming::DRIVE_FIXED;
use windows::core::PCWSTR;

pub struct Disk {
    pdh: Option<Pdh>,
    reopen: Every,
    /// Updated in place each sample so a disk's entry (and its name) is reused while its instance exists.
    disks: Vec<DiskInfo>,
    buf: ArrayBuf,
    volumes: Vec<VolumeInfo>,
    vol_refresh: Every,
}

struct Pdh {
    q: Query,
    read: Counter,
    write: Counter,
    idle: Counter,
}

/// PDH instances are "<disk#> <letters>" (e.g. "2 C: H:"); the number is stable while letters can change.
fn disk_number(name: &str) -> &str {
    name.split(' ').next().unwrap_or(name)
}

/// Moves the entry named `name` (reused from `disks[n..]`, else new) to index `n`, resets it to defaults except
/// for the name, and returns it. None only if `n` is past the end.
fn slot<'a>(disks: &'a mut Vec<DiskInfo>, n: usize, name: &str) -> Option<&'a mut DiskInfo> {
    match disks.get(n..)?.iter().position(|d| d.name == name) {
        Some(i) => disks.swap(n, n + i),
        None => {
            disks.push(DiskInfo { name: name.to_owned(), ..Default::default() });
            let last = disks.len() - 1;
            disks.swap(n, last);
        }
    }
    let d = disks.get_mut(n)?;
    *d = DiskInfo { name: std::mem::take(&mut d.name), ..Default::default() };
    Some(d)
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

    /// Refreshes `disks` from one collection; false (contents unspecified) if a counter could not be read.
    fn read(&self, disks: &mut Vec<DiskInfo>, buf: &mut ArrayBuf) -> bool {
        if !self.q.collect() {
            return false;
        }
        // The read counter defines the disk set; instances that vanished are dropped.
        let mut n = 0;
        let ok = self.read.each_double(0, buf, |name, r| {
            if name != "_Total"
                && let Some(d) = slot(disks, n, name)
            {
                d.read_bps = r;
                n += 1;
            }
        });
        disks.truncate(n);
        let mut set = |c: Counter, f: fn(&mut DiskInfo, f64)| {
            c.each_double(0, buf, |name, v| {
                if let Some(d) = disks.iter_mut().find(|d| d.name == name) {
                    f(d, v);
                }
            })
        };
        let ok = ok
            && set(self.write, |d, v| d.write_bps = v)
            && set(self.idle, |d, i| d.active_pct = (100.0 - i).clamp(0.0, 100.0) as f32);
        disks.sort_unstable_by_key(|d| disk_number(&d.name).parse::<u32>().unwrap_or(u32::MAX));
        ok
    }
}

impl Disk {
    pub fn new() -> Self {
        Self {
            pdh: Pdh::open(),
            reopen: Every::default(),
            disks: Vec::new(),
            buf: ArrayBuf::default(),
            volumes: Vec::new(),
            vol_refresh: Every::default(),
        }
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
        match self.pdh.as_ref().map(|p| p.read(&mut self.disks, &mut self.buf)) {
            Some(true) => snap.disks.clone_from(&self.disks),
            Some(false) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_numbers() {
        assert_eq!(disk_number("2 C: H:"), "2");
        assert_eq!(disk_number("0"), "0");
        assert_eq!(disk_number(""), "");
    }

    #[test]
    fn slots_reuse_entries_and_reset_values() {
        let d = |name: &str| DiskInfo { name: name.into(), read_bps: 1.0, active_pct: 5.0, ..Default::default() };
        let mut disks = vec![d("1 D:"), d("0 C:"), d("2 E:")];
        let names = ["0 C:", "3 F:", "1 D:"];
        for (n, name) in names.iter().enumerate() {
            let s = slot(&mut disks, n, name).unwrap();
            assert!(s.name == *name && s.read_bps == 0.0 && s.active_pct == 0.0);
        }
        disks.truncate(names.len());
        assert_eq!(disks.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), names);
        assert!(slot(&mut disks, 9, "x").is_none());
    }
}
