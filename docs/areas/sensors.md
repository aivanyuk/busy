# busy-sensors

`crates/sensors` — `sources()` returns `[Gpu, Sensors]` (in that order; they share `Rc<RefCell<Shared>>`). Files: `gpu.rs` (DXGI + PDH + D3DKMT), `nvml.rs`, `adl.rs`, `lhm.rs` (WMI), `hwinfo.rs` (shared memory), `lib.rs` (System32 DLL loader, sensors merge logic).

Live check: `cargo run -p busy-sensors --example dump_sensors [samples]`.

## Backends

| Backend | Provides | Notes |
|---|---|---|
| DXGI `EnumAdapters1` | adapters, name, LUID, VRAM total | Skips software + Microsoft (0x1414) adapters. Re-enumerated every 60 s. |
| PDH `\GPU Engine(*)`, `\GPU Adapter Memory(*)` | util, per-engine, VRAM used, per-process GPU | Opening the counters costs 540–680 ms once. First sample has no util (needs two collections). |
| D3DKMT (`D3DKMTQueryAdapterInfo`) | PCI location, temp, fan RPM, mem clock | Task Manager's source; no DLL, any WDDM 2.5+ driver. Labelled "WDDM". Used to match NVML/ADL devices to DXGI adapters by PCI bus/device. |
| NVML (`nvml.dll`) | temp, fan %, power, clocks | Loaded only if an NVIDIA adapter exists. No hotspot (not public). |
| ADL (`atiadlxx.dll`) | edge/hotspot temp, fan, power, clocks (Overdrive8, fallback OverdriveN) | **Untested on real AMD hardware.** |
| LibreHardwareMonitor / OpenHardwareMonitor WMI | all sensors incl. CPU temps | `root\LibreHardwareMonitor` then `root\OpenHardwareMonitor`. Lazy connect, retry every 30 s; queries >15 ms throttled to every 3 s. **Untested with LHM running.** |
| HWiNFO `Global\HWiNFO_SENS_SM2` | all sensors | Requires "Shared Memory Support" enabled in HWiNFO. The view stays mapped; each sample copies only the header-declared extent into a reused buffer and parses it with bounds checks. The mapping is dropped (and reopened after 5 s, picking up a restarted HWiNFO's new section) when the signature is no longer `HWiS` (`DEAD` once HWiNFO stops) or the poll time has not moved for 30 s. **Untested with HWiNFO running.** |

Precedence for `snap.sensors`: LHM → HWiNFO → NVML/ADL/WDDM readings (the latter only when neither tool is present, to avoid duplicates). LHM/HWiNFO also fill missing GPU temp/hotspot/fan, matched by GPU name.

## Security rules

- DLLs: through `busy_win::Dll` only (`LoadLibraryExW(name, LOAD_LIBRARY_SEARCH_SYSTEM32)`, symbols via `GetProcAddress`); absence is normal and silent.
- Shared memory / WMI data is untrusted input: validate every offset, size, count before reading.

## Measured cost (debug, RTX 3080)

GPU steady 3–5 ms (PDH 2.5–4 ms, NVML 0.5 ms). Sensors µs when nothing is present. Startup ~0.4–0.8 s.

Release, `dump_sensors 60`, median per sample: GPU ~1.7 ms and 36 heap allocations (692 while the per-sample maps and PDH instance names were allocated every tick; each per-process engine instance cost one `String`).

## Gotchas

- Unelevated `OpenProcess` fails for some processes (e.g. dwm.exe) → name fallback via `NtQuerySystemInformation(SystemProcessIdInformation)`.
- Optimus laptops: polling NVML may keep the dGPU awake (not handled).
- Engine list includes idle engines (Security, VR, OFA…) like Task Manager; the UI may filter them.
