//! Totals sum interfaces that are connected (oper up + media connected), not loopback, not NDIS filter (LWF)
//! rows, and flagged HardwareInterface. Virtual adapters (Hyper-V/WSL vEthernet, vSwitch, VPN/TAP, tunnels) are
//! excluded because their traffic also crosses a physical NIC and would be double counted. If no hardware
//! interface is connected, all connected non-filter, non-loopback interfaces are summed instead.
//! Listed: connected interfaces that are hardware or have an IPv4 address, plus disconnected physical NICs.
//!
//! GetIfTable2 costs several ms, so it only (re)discovers candidates every 60 s, or sooner when the 10 s
//! IPv4 address check sees an interface gain an address; each sample polls those with GetIfEntry2
//! (~tens of µs each).

use crate::util::{Clock, Every};
use busy_core::{Module, NetIf, NetInfo, Snapshot, Source};
use busy_win::from_wide;
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::NetworkManagement::IpHelper::*;
use windows::Win32::NetworkManagement::Ndis::{IfOperStatusUp, MediaConnectStateConnected, NET_LUID_LH};
use windows::Win32::Networking::WinSock::AF_INET;

// MIB_IF_ROW2.InterfaceAndOperStatusFlags bits.
const HARDWARE: u8 = 1;
const FILTER: u8 = 2;
const CONNECTOR: u8 = 4;

#[derive(Default)]
pub struct Network {
    clock: Clock,
    refresh: Every,
    rediscover: Every,
    candidates: Vec<NET_LUID_LH>,
    known: HashSet<u64>,
    addrs: HashMap<u64, Vec<String>>,
    prev: HashMap<u64, (u64, u64)>,
    rx_total: u64,
    tx_total: u64,
}

struct Row {
    luid: u64,
    name: String,
    rx: u64,
    tx: u64,
    speed: u64,
    flags: u8,
    connected: bool,
}

impl Source for Network {
    fn module(&self) -> Module {
        Module::Network
    }

    fn sample(&mut self, snap: &mut Snapshot) {
        if self.refresh.due(10) {
            self.addrs = ipv4_addrs();
            // Rediscover when an interface gains an address (VPN up, NIC plugged in) since the last discovery.
            let new_if = self.addrs.keys().any(|l| !self.known.contains(l));
            if (new_if || self.rediscover.due(60))
                && let Some(c) = candidates()
            {
                self.rediscover.arm();
                self.known = c.iter().map(|l| unsafe { l.Value }).chain(self.addrs.keys().copied()).collect();
                self.candidates = c;
            }
        }
        let dt = self.clock.tick();
        let mut prev = std::mem::take(&mut self.prev);
        let rows: Vec<_> = self
            .candidates
            .iter()
            .filter_map(|&luid| {
                let r = entry(luid)?;
                self.prev.insert(r.luid, (r.rx, r.tx));
                // New interface or counter reset yields 0 for this interval.
                let (drx, dtx) = prev
                    .remove(&r.luid)
                    .map_or((0, 0), |(prx, ptx)| (r.rx.saturating_sub(prx), r.tx.saturating_sub(ptx)));
                Some(Row { rx: drx, tx: dtx, ..r })
            })
            .collect();
        let any_hw = rows.iter().any(|r| r.connected && r.flags & HARDWARE != 0);
        let (mut drx, mut dtx) = (0, 0);
        for r in rows.iter().filter(|r| r.connected && (r.flags & HARDWARE != 0 || !any_hw)) {
            drx += r.rx;
            dtx += r.tx;
        }
        self.rx_total += drx;
        self.tx_total += dtx;
        let rate = |b: u64| dt.map_or(0.0, |dt| b as f64 / dt);
        let mut interfaces: Vec<_> = rows
            .into_iter()
            .filter_map(|r| {
                let ipv4 = self.addrs.get(&r.luid).cloned().unwrap_or_default();
                let hw = r.flags & HARDWARE != 0;
                let show = if r.connected { hw || !ipv4.is_empty() } else { hw && r.flags & CONNECTOR != 0 };
                show.then(|| NetIf {
                    rx_bps: rate(r.rx),
                    tx_bps: rate(r.tx),
                    ipv4,
                    link_speed_bps: r.speed,
                    connected: r.connected,
                    name: r.name,
                })
            })
            .collect();
        interfaces.sort_by_key(|i| !i.connected);
        snap.net = Some(NetInfo {
            rx_bps: rate(drx),
            tx_bps: rate(dtx),
            rx_total: self.rx_total,
            tx_total: self.tx_total,
            interfaces,
        });
    }
}

fn row(r: &MIB_IF_ROW2) -> Row {
    let speed = r.ReceiveLinkSpeed.max(r.TransmitLinkSpeed);
    Row {
        luid: unsafe { r.InterfaceLuid.Value },
        name: from_wide(&r.Alias),
        rx: r.InOctets,
        tx: r.OutOctets,
        speed: if speed == u64::MAX { 0 } else { speed },
        flags: r.InterfaceAndOperStatusFlags._bitfield,
        connected: r.OperStatus == IfOperStatusUp && r.MediaConnectState == MediaConnectStateConnected,
    }
}

fn entry(luid: NET_LUID_LH) -> Option<Row> {
    let mut r = MIB_IF_ROW2 { InterfaceLuid: luid, ..Default::default() };
    (unsafe { GetIfEntry2(&mut r) } == ERROR_SUCCESS).then(|| row(&r))
}

/// Non-loopback, non-filter interfaces that are hardware or currently connected.
fn candidates() -> Option<Vec<NET_LUID_LH>> {
    let mut t: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    if unsafe { GetIfTable2(&mut t) } != ERROR_SUCCESS || t.is_null() {
        return None;
    }
    let rows = unsafe { std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize) };
    let out = rows
        .iter()
        .filter(|r| r.Type != IF_TYPE_SOFTWARE_LOOPBACK && r.InterfaceAndOperStatusFlags._bitfield & FILTER == 0)
        .filter(|r| {
            let r = row(r);
            r.connected || r.flags & HARDWARE != 0
        })
        .map(|r| r.InterfaceLuid)
        .collect();
    unsafe { FreeMibTable(t.cast()) };
    Some(out)
}

fn ipv4_addrs() -> HashMap<u64, Vec<String>> {
    let mut out: HashMap<u64, Vec<String>> = HashMap::new();
    let mut t: *mut MIB_UNICASTIPADDRESS_TABLE = std::ptr::null_mut();
    if unsafe { GetUnicastIpAddressTable(AF_INET, &mut t) } != ERROR_SUCCESS || t.is_null() {
        return out;
    }
    let rows = unsafe { std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize) };
    for r in rows {
        let (luid, a) = unsafe { (r.InterfaceLuid.Value, r.Address.Ipv4.sin_addr.S_un.S_addr) };
        out.entry(luid).or_default().push(Ipv4Addr::from(a.to_ne_bytes()).to_string());
    }
    unsafe { FreeMibTable(t.cast()) };
    out
}
