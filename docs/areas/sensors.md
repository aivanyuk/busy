# busy-sensors

`crates/sensors` — `sources()` returns `[Gpu, Sensors]` (in that order; they share `Rc<RefCell<Shared>>`). Files: `gpu.rs` (DXGI + PDH through `busy_win::pdh` + D3DKMT), `dx.rs` (driver version, feature level), `nvml.rs`, `adl.rs`, `lhm.rs` (WMI), `hwinfo.rs` (shared memory), `lib.rs` (System32 DLL loader, sensors merge logic).

Live check: `cargo run -p busy-sensors --example dump_sensors -- [samples] [--third-party]` (`--third-party` also reads LHM/HWiNFO).

## Opt-in: third-party sensor tools

LibreHardwareMonitor (WMI) and HWiNFO (shared memory) are read only when `SourceOptions::third_party_sensors` (from `Config.opt_in.third_party_sensors`) is on — default **off**: they publish data from another program the user installed. `SensorsSource::configure` creates their readers when it turns on and drops them (releasing the WMI connection) when it turns off; while off, neither the WMI namespace nor the shared-memory section is opened. NVML, ADL and D3DKMT are first-party driver APIs and stay on. Off by default means no CPU temperatures, fans or CPU power on most machines — the Sensors module shows only the GPU vendor readings.

## Backends

| Backend | Provides | Notes |
|---|---|---|
| DXGI `EnumAdapters1` | adapters, name, LUID, VRAM total | Skips software + Microsoft (0x1414) adapters. Re-enumerated every 60 s. |
| DXGI `CheckInterfaceSupport(IDXGIDevice)` | driver version (UMD, 4 × u16, e.g. 32.0.16.1692 = NVIDIA 616.92) | ~0.5 ms per adapter at enumeration; does not load the user-mode driver (checked: `nvwgf2umx.dll` stays unloaded). |
| DirectX adapter cache `HKLM\SOFTWARE\Microsoft\DirectX\{GUID}` | feature level (`MaxD3D12FeatureLevel`, else `MaxD3D11FeatureLevel`) | Written by DXGI, read by dxdiag; read through `busy_win::reg_subkeys`/`reg_qword`/`reg_dword` (at most 64 entries). Matched by `AdapterLuid` (rewritten each boot), else by a unique vendor/device id. ~0.25 ms at enumeration (3 entries). Rejected: a `D3D12CreateDevice` probe took 200–350 ms and kept the UMD mapped (+17–23 MB private) even after release and `FreeLibrary`; D3D11 took 180–230 ms and caps at FL 12_1. |
| PDH `\GPU Engine(*)`, `\GPU Adapter Memory(*)` | util, per-engine, VRAM used, per-process GPU | Opening the counters costs 540–680 ms once. First sample has no util (needs two collections). |
| D3DKMT (`D3DKMTQueryAdapterInfo`) | PCI location, temp, fan RPM, mem clock | Task Manager's source; no DLL, any WDDM 2.5+ driver. Labelled "WDDM". Used to match NVML/ADL devices to DXGI adapters by PCI bus/device. |
| NVML (`nvml.dll`) | temp, fan %, power, clocks | Loaded only if an NVIDIA adapter exists. No hotspot (not public). |
| ADL (`atiadlxx.dll`) | edge/hotspot temp, fan, power, clocks (Overdrive8, fallback OverdriveN) | **Untested on real AMD hardware.** |
| LibreHardwareMonitor / OpenHardwareMonitor WMI (opt-in) | all sensors incl. CPU temps | `root\LibreHardwareMonitor` then `root\OpenHardwareMonitor`. Lazy connect, retry every 30 s; queries >15 ms throttled to every 3 s. **Untested with LHM running.** |
| HWiNFO `Global\HWiNFO_SENS_SM2` (opt-in) | all sensors | Requires "Shared Memory Support" enabled in HWiNFO. Whole view copied then parsed with bounds checks. **Untested with HWiNFO running.** |

Precedence for `snap.sensors`: LHM → HWiNFO → NVML/ADL/WDDM readings (the latter only when neither tool is opted in and present, to avoid duplicates). LHM/HWiNFO also fill missing GPU temp/hotspot/fan, matched by GPU name — only when opted in.

## Security rules

- DLLs: through `busy_win::Dll` only (`LoadLibraryExW(name, LOAD_LIBRARY_SEARCH_SYSTEM32)`, symbols via `GetProcAddress`); absence is normal and silent.
- Shared memory / WMI data is untrusted input: validate every offset, size, count before reading.

## Measured cost (debug, RTX 3080)

GPU steady 3–5 ms (PDH 2.5–4 ms, NVML 0.5 ms). Sensors µs when nothing is present. Startup ~0.4–0.8 s.

Release, `dump_sensors 60`, median per sample: GPU ~2 ms and 39 heap allocations (about 680 while the per-sample maps and PDH instance names were allocated every tick; each per-process engine instance cost one `String`).

## Gotchas

- Unelevated `OpenProcess` fails for some processes (e.g. dwm.exe) → name fallback via `NtQuerySystemInformation(SystemProcessIdInformation)`.
- Optimus laptops: polling NVML may keep the dGPU awake (not handled).
- Engine list includes idle engines (Security, VR, OFA…) like Task Manager; the UI may filter them.
