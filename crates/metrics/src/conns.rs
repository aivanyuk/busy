//! Open TCP connections per process (`GetExtendedTcpTable`): how the Network flyout ranks processes when the
//! per-process traffic trace is off or not permitted. Needs no rights; says nothing about traffic.

use std::collections::HashMap;
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_CONNECTIONS,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};

/// `dwOwningPid`'s offset in each row (iprtrmib.h / tcpmib.h, SDK 10.0.26100).
const PID_V4: usize = std::mem::offset_of!(MIB_TCPROW_OWNER_PID, dwOwningPid);
const PID_V6: usize = std::mem::offset_of!(MIB_TCP6ROW_OWNER_PID, dwOwningPid);
const _: () = assert!(
    size_of::<MIB_TCPROW_OWNER_PID>() == 24 && PID_V4 == 20 && size_of::<MIB_TCP6ROW_OWNER_PID>() == 56 && PID_V6 == 52
);

#[derive(Default)]
pub(crate) struct Connections {
    /// One table at a time, kept between samples; u32s for the table's alignment.
    buf: Vec<u32>,
}

impl Connections {
    /// Adds each process's open TCP connections (IPv4 and IPv6; listening sockets aren't connections) to `out`.
    /// False if neither table could be read.
    pub(crate) fn count(&mut self, out: &mut HashMap<u32, u64>) -> bool {
        let mut any = false;
        for (af, row, pid_at) in [
            (AF_INET, size_of::<MIB_TCPROW_OWNER_PID>(), PID_V4),
            (AF_INET6, size_of::<MIB_TCP6ROW_OWNER_PID>(), PID_V6),
        ] {
            if !self.read(af.0 as u32) {
                continue;
            }
            any = true;
            // SAFETY: u32 storage is valid as four times as many bytes.
            let bytes = unsafe { std::slice::from_raw_parts(self.buf.as_ptr().cast::<u8>(), self.buf.len() * 4) };
            // A connection in TIME_WAIT belongs to no process any more (pid 0).
            for pid in pids(bytes, row, pid_at).filter(|&p| p != 0) {
                *out.entry(pid).or_default() += 1;
            }
        }
        any
    }

    fn read(&mut self, af: u32) -> bool {
        // The table can grow between the size query and the read.
        for _ in 0..3 {
            let mut size = (self.buf.len() * 4) as u32;
            let ptr = (!self.buf.is_empty()).then(|| self.buf.as_mut_ptr().cast());
            // SAFETY: `ptr` (when given) points at `size` writable bytes; `size` is written back.
            let rc = unsafe { GetExtendedTcpTable(ptr, &mut size, false, af, TCP_TABLE_OWNER_PID_CONNECTIONS, 0) };
            match rc {
                r if r == NO_ERROR.0 => return true,
                r if r == ERROR_INSUFFICIENT_BUFFER.0 => self.buf.resize((size as usize + 4096).div_ceil(4), 0),
                _ => return false,
            }
        }
        false
    }
}

/// The owning pids in a `MIB_TCPTABLE_OWNER_PID` / `MIB_TCP6TABLE_OWNER_PID`: a u32 count, then `row`-byte rows
/// with the pid `pid_at` bytes in. Rows past the end of `buf` are dropped, whatever the count says.
fn pids(buf: &[u8], row: usize, pid_at: usize) -> impl Iterator<Item = u32> + '_ {
    let n = buf.first_chunk::<4>().map_or(0, |c| u32::from_ne_bytes(*c) as usize);
    let rows = buf.get(4..).unwrap_or_default();
    rows.chunks_exact(row)
        .take(n)
        .filter_map(move |r| r.get(pid_at..)?.first_chunk::<4>().map(|c| u32::from_ne_bytes(*c)))
}

#[cfg(test)]
mod tests {
    use super::{PID_V4, PID_V6, pids};

    fn table(n: u32, row: usize, pid_at: usize, rows: &[u32]) -> Vec<u8> {
        let mut b = n.to_ne_bytes().to_vec();
        for &pid in rows {
            let mut r = vec![0xAB; row];
            r[pid_at..pid_at + 4].copy_from_slice(&pid.to_ne_bytes());
            b.extend(r);
        }
        b
    }

    #[test]
    fn reads_the_owning_pids() {
        let v4 = table(3, 24, PID_V4, &[4, 1200, 0]);
        assert_eq!(pids(&v4, 24, PID_V4).collect::<Vec<_>>(), [4, 1200, 0]);
        let v6 = table(2, 56, PID_V6, &[880, 880]);
        assert_eq!(pids(&v6, 56, PID_V6).collect::<Vec<_>>(), [880, 880]);
    }

    #[test]
    fn never_reads_past_the_buffer() {
        // More rows claimed than present, a truncated last row, no rows, no count.
        let mut v4 = table(1000, 24, PID_V4, &[7, 8]);
        assert_eq!(pids(&v4, 24, PID_V4).collect::<Vec<_>>(), [7, 8]);
        v4.truncate(v4.len() - 1);
        assert_eq!(pids(&v4, 24, PID_V4).collect::<Vec<_>>(), [7]);
        assert_eq!(pids(&table(0, 24, PID_V4, &[7]), 24, PID_V4).count(), 0);
        assert_eq!(pids(&[1, 0], 24, PID_V4).count(), 0);
        assert_eq!(pids(&[], 56, PID_V6).count(), 0);
    }
}
