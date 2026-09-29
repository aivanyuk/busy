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
use crate::wifi::Wlan;
use busy_core::{Module, NetIf, NetInfo, NetKind, Snapshot, Source, WifiInfo};
use busy_win::from_wide;
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::NetworkManagement::IpHelper::*;
use windows::Win32::NetworkManagement::Ndis::{IfOperStatusUp, MediaConnectStateConnected, NET_LUID_LH};
use windows::Win32::Networking::WinSock::AF_INET;
use windows::core::GUID;

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
    wlan: Wlan,
    wifi_full: Every,
    wifi_rssi: Every,
    /// GUIDs of the connected Wi-Fi interfaces this sample; refilled each sample.
    wifi_up: Vec<u128>,
    /// (interface GUID, association; None: queried, not associated) for the connected Wi-Fi interfaces.
    wifi: Vec<(u128, Option<WifiInfo>)>,
}

impl Network {
    /// The full association query costs ~30 ms, so it runs when a Wi-Fi interface connects and then every
    /// 60 s (SSID, quality and channel can lag a roam by that much); RSSI (~0.2 ms) every 5 s.
    fn refresh_wifi(&mut self, rows: &[Row]) {
        let up = rows.iter().filter(|r| r.connected && r.kind == NetKind::Wifi).map(|r| r.guid.to_u128());
        self.wifi_up.clear();
        self.wifi_up.extend(up);
        self.wifi.retain(|(g, _)| self.wifi_up.contains(g));
        if self.wifi_up.is_empty() {
            return;
        }
        let new = self.wifi_up.iter().any(|g| !self.wifi.iter().any(|(k, _)| k == g));
        if new || self.wifi_full.due(60) {
            self.wifi_full.arm();
            self.wifi_rssi.arm();
            for &g in &self.wifi_up {
                let info = self.wlan.info(&GUID::from_u128(g));
                match self.wifi.iter_mut().find(|(k, _)| *k == g) {
                    Some(e) => e.1 = info,
                    None => self.wifi.push((g, info)),
                }
            }
        } else if self.wifi_rssi.due(5) {
            for (g, w) in &mut self.wifi {
                if let Some(w) = w
                    && let Some(rssi) = self.wlan.rssi(&GUID::from_u128(*g))
                {
                    w.rssi_dbm = Some(rssi);
                }
            }
        }
    }
}

struct Row {
    luid: u64,
    guid: GUID,
    name: String,
    rx: u64,
    tx: u64,
    speed: u64,
    flags: u8,
    connected: bool,
    kind: NetKind,
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
        self.refresh_wifi(&rows);
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
                    kind: r.kind,
                    wifi: self.wifi.iter().find(|(g, _)| *g == r.guid.to_u128()).and_then(|(_, w)| w.clone()),
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
    let flags = r.InterfaceAndOperStatusFlags._bitfield;
    Row {
        luid: unsafe { r.InterfaceLuid.Value },
        guid: r.InterfaceGuid,
        name: from_wide(&r.Alias),
        rx: r.InOctets,
        tx: r.OutOctets,
        speed: if speed == u64::MAX { 0 } else { speed },
        flags,
        connected: r.OperStatus == IfOperStatusUp && r.MediaConnectState == MediaConnectStateConnected,
        kind: kind(r.Type, flags),
    }
}

/// Hyper-V vEthernet and TAP adapters are IF_TYPE_ETHERNET_CSMACD too; only hardware NICs count as Ethernet.
fn kind(if_type: u32, flags: u8) -> NetKind {
    match if_type {
        IF_TYPE_IEEE80211 => NetKind::Wifi,
        IF_TYPE_ETHERNET_CSMACD if flags & HARDWARE != 0 => NetKind::Ethernet,
        _ => NetKind::Other,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_kinds() {
        assert_eq!(kind(IF_TYPE_IEEE80211, HARDWARE), NetKind::Wifi);
        assert_eq!(kind(IF_TYPE_ETHERNET_CSMACD, HARDWARE | CONNECTOR), NetKind::Ethernet);
        assert_eq!(kind(IF_TYPE_ETHERNET_CSMACD, 0), NetKind::Other);
        assert_eq!(kind(IF_TYPE_SOFTWARE_LOOPBACK, HARDWARE), NetKind::Other);
        assert_eq!(kind(IF_TYPE_PROP_VIRTUAL, 0), NetKind::Other);
    }
}
