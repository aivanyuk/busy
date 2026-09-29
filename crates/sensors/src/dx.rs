//! Per-adapter DirectX facts for the GPU flyout: user-mode driver version and highest feature level.
//!
//! The feature level is read from the adapter cache DXGI keeps under `HKLM\SOFTWARE\Microsoft\DirectX\{GUID}`
//! (what dxdiag shows). Probing it with `D3D12CreateDevice` instead cost 200–350 ms and left the vendor
//! user-mode driver mapped (+17–23 MB private bytes) even after the device and d3d12.dll were released.

use busy_win::{reg_dword, reg_qword, reg_subkeys};
use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;

const CACHE: &str = r"SOFTWARE\Microsoft\DirectX";

/// Bounds the walk over the cache. It holds one entry per adapter seen (plus `ShaderCache`); 3 here.
const MAX_ENTRIES: usize = 64;

/// UMD version from `IDXGIAdapter::CheckInterfaceSupport(IDXGIDevice)`, four 16-bit parts high to low,
/// formatted like Task Manager's "Driver version" (e.g. 32.0.16.1692).
pub(crate) fn driver_version(umd: i64) -> String {
    let p = |shift: u32| (umd >> shift) & 0xFFFF;
    format!("{}.{}.{}.{}", p(48), p(32), p(16), p(0))
}

/// D3D_FEATURE_LEVEL (e.g. 0xC200 = 12_2) as (major, minor); None for 0 (not supported / not recorded).
fn feature_level(v: u32) -> Option<(u8, u8)> {
    (v != 0).then_some(((v >> 12) as u8 & 0xF, (v >> 8) as u8 & 0xF))
}

/// One adapter entry of the DirectX registry cache.
#[derive(Debug, PartialEq)]
pub(crate) struct Cached {
    luid: u64,
    ids: (u32, u32),
    level: Option<(u8, u8)>,
}

/// All adapter entries of the DirectX cache; run on each GPU re-enumeration (every 60 s).
pub(crate) fn cached_adapters() -> Vec<Cached> {
    let hklm = HKEY_LOCAL_MACHINE;
    reg_subkeys(hklm, CACHE, MAX_ENTRIES)
        .into_iter()
        .filter_map(|sub| {
            let key = format!(r"{CACHE}\{sub}");
            // Entries without a LUID (e.g. `ShaderCache`) are not adapters.
            let luid = reg_qword(hklm, &key, "AdapterLuid")?;
            let level = |name| reg_dword(hklm, &key, name).and_then(feature_level);
            let dword = |name| reg_dword(hklm, &key, name).unwrap_or(0);
            Some(Cached {
                luid,
                ids: (dword("VendorId"), dword("DeviceId")),
                level: level("MaxD3D12FeatureLevel").or_else(|| level("MaxD3D11FeatureLevel")),
            })
        })
        .collect()
}

/// Feature level for an adapter: by LUID (rewritten by DXGI each boot), else by vendor/device id when exactly
/// one cached entry has them (cache not yet refreshed after a driver update).
pub(crate) fn level_for(cache: &[Cached], luid: u64, ids: (u32, u32)) -> Option<(u8, u8)> {
    if let Some(c) = cache.iter().find(|c| c.luid == luid) {
        return c.level;
    }
    let mut same = cache.iter().filter(|c| c.ids == ids);
    match (same.next(), same.next()) {
        (Some(c), None) => c.level,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_driver_version() {
        assert_eq!(driver_version(0x0020_0000_0010_069C), "32.0.16.1692");
        assert_eq!(driver_version(0x000A_0000_65F4_243E), "10.0.26100.9278");
        assert_eq!(driver_version(-1), "65535.65535.65535.65535");
        assert_eq!(driver_version(0), "0.0.0.0");
    }

    #[test]
    fn decodes_feature_level() {
        let t = [(0xC200, Some((12, 2))), (0xC100, Some((12, 1))), (0xB000, Some((11, 0))), (0x9300, Some((9, 3)))];
        for (v, want) in t {
            assert_eq!(feature_level(v), want, "{v:#x}");
        }
        assert_eq!(feature_level(0), None);
    }

    #[test]
    fn matches_cache_entries() {
        let c = |luid, ids, l| Cached { luid, ids, level: Some((12, l)) };
        let cache = [c(1, (0x10DE, 1), 2), c(2, (0x8086, 2), 1), c(3, (0x8086, 2), 0)];
        assert_eq!(level_for(&cache, 2, (0, 0)), Some((12, 1)));
        assert_eq!(level_for(&cache, 9, (0x10DE, 1)), Some((12, 2)));
        // Ambiguous or unknown ids.
        assert_eq!(level_for(&cache, 9, (0x8086, 2)), None);
        assert_eq!(level_for(&cache, 9, (0x1002, 3)), None);
    }

    #[test]
    fn reads_the_cache_without_failing() {
        // Contents depend on the machine (a CI runner may have none); entries must decode sanely.
        for c in cached_adapters() {
            assert!(c.level.is_none_or(|(major, minor)| (9..=15).contains(&major) && minor <= 15), "{c:?}");
        }
    }
}
