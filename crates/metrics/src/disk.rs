use crate::util::Every;
use busy_core::{DiskInfo, Module, Snapshot, Source, VolumeInfo};
use busy_win::from_wide;
use busy_win::pdh::{ArrayBuf, Counter, Query};
use std::collections::HashMap;
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
};
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
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
    /// Drive of the Windows directory ("C:"), fixed for the session.
    system: Option<String>,
    /// Since-boot (read, written) bytes per disk number when the disk was first seen.
    base: HashMap<u32, (u64, u64)>,
}

struct Pdh {
    q: Query,
    read: Counter,
    write: Counter,
    idle: Counter,
    response: Counter,
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
    *d = DiskInfo { index: disk_number(&d.name).parse().ok(), name: std::mem::take(&mut d.name), ..Default::default() };
    Some(d)
}

/// Sets each volume's physical disk from the letters in the PDH instance names. A volume spanning disks is
/// listed under each of them; the lowest disk number wins (`disks` is sorted by number).
fn map_volumes(disks: &[DiskInfo], volumes: &mut [VolumeInfo]) {
    for v in volumes {
        v.disk_index = disks
            .iter()
            .find(|d| d.name.split(' ').skip(1).any(|l| l.eq_ignore_ascii_case(&v.mount)))
            .and_then(|d| d.index);
    }
}

/// Turns the since-boot counts in `d` into counts since the disk was first seen.
fn since_start(base: &mut HashMap<u32, (u64, u64)>, d: &mut DiskInfo) {
    let (Some(r), Some(w), Some(n)) = (d.read_total, d.written_total, d.index) else {
        (d.read_total, d.written_total) = (None, None);
        return;
    };
    let b = base.entry(n).or_insert((r, w));
    // A smaller count means another disk now has this number.
    if r < b.0 || w < b.1 {
        *b = (r, w);
    }
    d.read_total = Some(r - b.0);
    d.written_total = Some(w - b.1);
}

impl Pdh {
    fn open() -> Option<Self> {
        let q = Query::open()?;
        let add = |c: &str| q.add(&format!(r"\PhysicalDisk(*)\{c}"));
        let s = Self {
            read: add("Disk Read Bytes/sec")?,
            write: add("Disk Write Bytes/sec")?,
            idle: add("% Idle Time")?,
            response: add("Avg. Disk sec/Transfer")?,
            q,
        };
        s.q.collect().then_some(s)
    }

    /// Refreshes `disks` from one collection; false (contents unspecified) if a counter could not be read.
    /// Totals are cumulative since boot.
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
            && set(self.idle, |d, i| d.active_pct = (100.0 - i).clamp(0.0, 100.0) as f32)
            && set(self.response, |d, s| d.avg_response_ms = Some((s * 1000.0) as f32));
        // The raw value of a bytes/sec counter is the running byte count; unreadable leaves the totals None.
        for (c, f) in [
            (self.read, (|d, v| d.read_total = Some(v)) as fn(&mut DiskInfo, u64)),
            (self.write, |d, v| d.written_total = Some(v)),
        ] {
            c.each_raw(buf, |name, v| {
                if let (Some(d), Ok(v)) = (disks.iter_mut().find(|d| d.name == name), u64::try_from(v)) {
                    f(d, v);
                }
            });
        }
        disks.sort_unstable_by_key(|d| d.index.unwrap_or(u32::MAX));
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
            base: HashMap::new(),
            system: system_drive(),
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
        if self.vol_refresh.due(10) {
            self.volumes = volumes(self.system.as_deref());
        }
        match self.pdh.as_ref().map(|p| p.read(&mut self.disks, &mut self.buf)) {
            Some(true) => {
                self.disks.iter_mut().for_each(|d| since_start(&mut self.base, d));
                // Re-applied on every read: drive letters can move between disks.
                map_volumes(&self.disks, &mut self.volumes);
                snap.disks.clone_from(&self.disks);
            }
            Some(false) => {
                // Rebuild the query later in case the counter set was reset.
                self.pdh = None;
                self.reopen.arm();
            }
            None => {}
        }
        snap.volumes.clone_from(&self.volumes);
    }
}

/// "C:" for a Windows directory of `C:\Windows`.
fn system_drive() -> Option<String> {
    let mut win = [0u16; 261];
    // SAFETY: `win` is a writable buffer whose length the slice parameter passes.
    let n = unsafe { GetSystemWindowsDirectoryW(Some(&mut win)) } as usize;
    // A result longer than the buffer is the required size, not a path.
    let dir = from_wide(win.get(..n)?);
    Some(dir.get(..2)?.to_owned()).filter(|d| d.ends_with(':'))
}

fn volumes(system: Option<&str>) -> Vec<VolumeInfo> {
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
            let mount = from_wide(&root).trim_end_matches('\\').to_owned();
            Some(VolumeInfo {
                is_system: system.is_some_and(|s| s.eq_ignore_ascii_case(&mount)),
                mount,
                label: from_wide(&label),
                total,
                free,
                // Filled from the PDH instance names on each successful disk read.
                disk_index: None,
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

    #[test]
    fn totals_since_first_seen() {
        let mut base = HashMap::new();
        let mut at = |name: &str, r, w| {
            let index = disk_number(name).parse().ok();
            let mut d = DiskInfo { name: name.into(), read_total: r, written_total: w, index, ..Default::default() };
            since_start(&mut base, &mut d);
            (d.read_total, d.written_total)
        };
        assert_eq!(at("1 C:", Some(1000), Some(500)), (Some(0), Some(0)));
        // Letters changed, same disk number: keeps counting.
        assert_eq!(at("1 C: D:", Some(1500), Some(700)), (Some(500), Some(200)));
        // Counts went backwards: another disk took the number, restart from it.
        assert_eq!(at("1 E:", Some(10), Some(20)), (Some(0), Some(0)));
        assert_eq!(at("1 E:", None, Some(30)), (None, None));
        assert_eq!(at("x E:", Some(10), Some(20)), (None, None));
    }

    #[test]
    fn volumes_map_to_disks() {
        let disk =
            |name: &str| DiskInfo { name: name.into(), index: disk_number(name).parse().ok(), ..Default::default() };
        // Sorted by number, as Pdh::read leaves them; "D:" spans disks 0 and 2.
        let disks = [disk("0 D:"), disk("1"), disk("2 C: H: D:"), disk("x E:")];
        let mut vols: Vec<_> = ["C:", "c:", "D:", "E:", "H:", "Z:", "H"]
            .map(|m| VolumeInfo { mount: m.into(), ..Default::default() })
            .into();
        map_volumes(&disks, &mut vols);
        let got: Vec<_> = vols.iter().map(|v| v.disk_index).collect();
        assert_eq!(got, [Some(2), Some(2), Some(0), None, Some(2), None, None]);
    }

    #[test]
    fn system_drive_is_a_letter() {
        assert!(system_drive().is_some_and(|d| d.len() == 2 && d.ends_with(':')));
    }
}
