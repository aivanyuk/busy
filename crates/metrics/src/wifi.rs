//! Wi-Fi association (SSID, signal, channel) through the Native Wifi API.
//!
//! wlanapi.dll is loaded at runtime from System32 instead of imported: it is absent on some Windows Server
//! installs (Wireless LAN Service feature off), and a missing import would stop the whole app from starting.
//! On desktops without Wi-Fi the DLL exists but `WlanOpenHandle` fails while the WLAN AutoConfig service is
//! stopped; both cases leave Wi-Fi info `None` and are retried every 60 s.

use crate::util::Every;
use busy_core::WifiInfo;
use busy_win::Dll;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows::Win32::Foundation::{ERROR_INVALID_STATE, ERROR_NOT_FOUND, HANDLE};
use windows::Win32::NetworkManagement::WiFi::*;
use windows::core::{BOOL, GUID, w};

// Signatures from wlanapi.h.
type OpenHandle = unsafe extern "system" fn(u32, *const c_void, *mut u32, *mut HANDLE) -> u32;
type CloseHandle = unsafe extern "system" fn(HANDLE, *const c_void) -> u32;
type QueryInterface = unsafe extern "system" fn(
    HANDLE,
    *const GUID,
    WLAN_INTF_OPCODE,
    *const c_void,
    *mut u32,
    *mut *mut c_void,
    *mut WLAN_OPCODE_VALUE_TYPE,
) -> u32;
type GetNetworkBssList = unsafe extern "system" fn(
    HANDLE,
    *const GUID,
    *const DOT11_SSID,
    DOT11_BSS_TYPE,
    BOOL,
    *const c_void,
    *mut *mut WLAN_BSS_LIST,
) -> u32;
type FreeMemory = unsafe extern "system" fn(*const c_void);

/// Upper bound on BSS entries looked at. A cached scan list holds tens of entries even in dense areas.
const MAX_BSS_ENTRIES: usize = 1024;
const BSS_ENTRIES_OFFSET: usize = std::mem::offset_of!(WLAN_BSS_LIST, wlanBssEntries);

/// An open WLAN client handle, closed on drop (before `_dll` is freed: `Drop::drop` runs ahead of the fields).
struct Api {
    handle: HANDLE,
    close: CloseHandle,
    query: QueryInterface,
    bss_list: GetNetworkBssList,
    free: FreeMemory,
    _dll: Dll,
}

impl Api {
    fn load() -> Option<Self> {
        let dll = Dll::load(w!("wlanapi.dll"))?;
        // SAFETY: each type above is the wlanapi.h signature of the export it is looked up by.
        let (open, close, query, bss_list, free) = unsafe {
            (
                dll.sym::<OpenHandle>(c"WlanOpenHandle")?,
                dll.sym::<CloseHandle>(c"WlanCloseHandle")?,
                dll.sym::<QueryInterface>(c"WlanQueryInterface")?,
                dll.sym::<GetNetworkBssList>(c"WlanGetNetworkBssList")?,
                dll.sym::<FreeMemory>(c"WlanFreeMemory")?,
            )
        };
        let (mut version, mut handle) = (0u32, HANDLE::default());
        // SAFETY: both out-pointers are valid; the reserved parameter must be null. Client version 2 = Vista+.
        if unsafe { open(2, null(), &mut version, &mut handle) } != 0 {
            return None;
        }
        Some(Self { handle, close, query, bss_list, free, _dll: dll })
    }

    /// Calls `f` with the bytes WlanQueryInterface returned for `op`, then frees them. Err is the Win32 error.
    fn query_with<T>(&self, guid: &GUID, op: WLAN_INTF_OPCODE, f: impl FnOnce(&[u8]) -> T) -> Result<T, u32> {
        let (mut size, mut data) = (0u32, null_mut());
        // SAFETY: `self.handle` is open; `guid` outlives the call; the out-pointers are valid; reserved is null.
        let r = unsafe { (self.query)(self.handle, guid, op, null(), &mut size, &mut data, null_mut()) };
        if r != 0 || data.is_null() {
            return Err(r);
        }
        // SAFETY: on success `data` is a WlanFreeMemory allocation of `size` bytes, freed only below.
        let out = f(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) });
        // SAFETY: `data` came from WlanQueryInterface and is not used after this.
        unsafe { (self.free)(data) };
        Ok(out)
    }

    /// Current association. Measured ~30 ms (Intel AX201): ~16 ms for the connection query, ~13 ms for the BSS
    /// list — the caller runs it rarely. Err when the handle looks broken (WLAN service restarted).
    fn info(&self, guid: &GUID) -> Result<Option<WifiInfo>, ()> {
        let conn = self.query_with(guid, wlan_intf_opcode_current_connection, |b| {
            // SAFETY: `b` holds at least one WLAN_CONNECTION_ATTRIBUTES (checked); the read tolerates any alignment.
            (b.len() >= size_of::<WLAN_CONNECTION_ATTRIBUTES>())
                .then(|| unsafe { b.as_ptr().cast::<WLAN_CONNECTION_ATTRIBUTES>().read_unaligned() })
        });
        let Some(conn) =
            not_associated_is_none(conn)?.flatten().filter(|c| c.isState == wlan_interface_state_connected)
        else {
            return Ok(None);
        };
        let assoc = &conn.wlanAssociationAttributes;
        let rssi = self.rssi(guid).ok().flatten();
        let bss = self.bss(guid, assoc, conn.wlanSecurityAttributes.bSecurityEnabled);
        Ok(Some(WifiInfo {
            ssid: ssid(&assoc.dot11Ssid),
            signal_pct: assoc.wlanSignalQuality.min(100) as u8,
            rssi_dbm: rssi.or(bss.map(|b| b.0)),
            channel_mhz: bss.map(|b| b.1).filter(|&f| f > 0),
        }))
    }

    /// RSSI of the current association, dBm; ~0.2 ms.
    fn rssi(&self, guid: &GUID) -> Result<Option<i32>, ()> {
        let r =
            self.query_with(guid, wlan_intf_opcode_rssi, |b| Some(i32::from_ne_bytes(b.get(..4)?.try_into().ok()?)));
        Ok(not_associated_is_none(r)?.flatten())
    }

    /// (RSSI dBm, channel center MHz) of the associated BSS, from the driver's cached scan list (no new scan).
    fn bss(&self, guid: &GUID, assoc: &WLAN_ASSOCIATION_ATTRIBUTES, secure: BOOL) -> Option<(i32, u32)> {
        let mut list: *mut WLAN_BSS_LIST = null_mut();
        let (ssid, ty) = (&assoc.dot11Ssid, assoc.dot11BssType);
        // SAFETY: `self.handle` is open; `guid` and `ssid` outlive the call; `list` is a valid out-pointer.
        let r = unsafe { (self.bss_list)(self.handle, guid, ssid, ty, secure, null(), &mut list) };
        if r != 0 || list.is_null() {
            return None;
        }
        // SAFETY: on success `list` points at a WLAN_BSS_LIST header, which the API always writes whole.
        let (total, n) = unsafe { ((*list).dwTotalSize as usize, (*list).dwNumberOfItems as usize) };
        let fits = entries_fit(total, n);
        // SAFETY: `fits` checked that `n` entries lie inside the `total` bytes the API allocated at `list`.
        let entries = fits.then(|| unsafe {
            std::slice::from_raw_parts((&raw const (*list).wlanBssEntries).cast::<WLAN_BSS_ENTRY>(), n)
        });
        let out = entries
            .and_then(|es| es.iter().find(|e| e.dot11Bssid == assoc.dot11Bssid))
            .map(|e| (e.lRssi, e.ulChCenterFrequency / 1000));
        // SAFETY: `list` came from WlanGetNetworkBssList; `entries` borrowed from it is not used after this.
        unsafe { (self.free)(list.cast()) };
        out
    }
}

impl Drop for Api {
    fn drop(&mut self) {
        // SAFETY: we own the client handle opened in `load`, and the DLL exporting `close` is still loaded.
        unsafe { (self.close)(self.handle, null()) };
    }
}

/// Whether `n` BSS entries fit in a list of `total` bytes; `n` is also capped, since both come from the driver.
fn entries_fit(total: usize, n: usize) -> bool {
    n <= MAX_BSS_ENTRIES
        && n.checked_mul(size_of::<WLAN_BSS_ENTRY>())
            .and_then(|b| b.checked_add(BSS_ENTRIES_OFFSET))
            .is_some_and(|e| e <= total)
}

/// Maps "interface not associated / gone" to Ok(None); any other error means the handle is unusable.
fn not_associated_is_none<T>(r: Result<T, u32>) -> Result<Option<T>, ()> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if e == ERROR_INVALID_STATE.0 || e == ERROR_NOT_FOUND.0 => Ok(None),
        Err(_) => Err(()),
    }
}

/// The SSID is bytes, usually UTF-8; its length comes from the driver and is clamped to the 32-byte field.
fn ssid(s: &DOT11_SSID) -> String {
    let n = (s.uSSIDLength as usize).min(s.ucSSID.len());
    String::from_utf8_lossy(s.ucSSID.get(..n).unwrap_or_default()).into_owned()
}

/// The WLAN client, opened on first use and kept; reopened with a 60 s backoff after a failure.
#[derive(Default)]
pub(crate) struct Wlan {
    api: Option<Api>,
    retry: Every,
}

impl Wlan {
    fn with<T>(&mut self, f: impl FnOnce(&Api) -> Result<Option<T>, ()>) -> Option<T> {
        if self.api.is_none() && self.retry.due(60) {
            self.api = Api::load();
        }
        f(self.api.as_ref()?).unwrap_or_else(|()| {
            self.api = None;
            self.retry.arm();
            None
        })
    }

    /// Association of the Wi-Fi interface `guid`; None when disconnected or the WLAN API is unavailable.
    pub(crate) fn info(&mut self, guid: &GUID) -> Option<WifiInfo> {
        self.with(|api| api.info(guid))
    }

    pub(crate) fn rssi(&mut self, guid: &GUID) -> Option<i32> {
        self.with(|api| api.rssi(guid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssid_bytes() {
        let mk = |len: u32, bytes: &[u8]| {
            let mut s = DOT11_SSID { uSSIDLength: len, ucSSID: [0; 32] };
            s.ucSSID[..bytes.len()].copy_from_slice(bytes);
            ssid(&s)
        };
        assert_eq!(mk(4, b"home"), "home");
        assert_eq!(mk(0, b""), "");
        assert_eq!(mk(5, b"caf\xc3\xa9"), "café");
        // Truncated multi-byte sequence decodes lossily instead of failing.
        assert_eq!(mk(4, b"caf\xc3\xa9"), "caf\u{FFFD}");
        // A length beyond the 32-byte buffer is clamped, not trusted.
        assert_eq!(mk(200, &[b'a'; 32]).len(), 32);
    }

    #[test]
    fn bss_entry_counts_are_bounded() {
        let one = BSS_ENTRIES_OFFSET + size_of::<WLAN_BSS_ENTRY>();
        assert!(entries_fit(one, 1) && entries_fit(one, 0));
        assert!(!entries_fit(one - 1, 1) && !entries_fit(one, 2));
        assert!(!entries_fit(usize::MAX, MAX_BSS_ENTRIES + 1));
        assert!(!entries_fit(usize::MAX, usize::MAX));
    }

    #[test]
    fn not_associated_maps_to_none() {
        assert_eq!(not_associated_is_none(Ok(1)), Ok(Some(1)));
        assert_eq!(not_associated_is_none::<u8>(Err(ERROR_NOT_FOUND.0)), Ok(None));
        assert_eq!(not_associated_is_none::<u8>(Err(ERROR_INVALID_STATE.0)), Ok(None));
        assert_eq!(not_associated_is_none::<u8>(Err(6)), Err(()));
    }
}
