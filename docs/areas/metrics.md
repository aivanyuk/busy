# busy-metrics

`crates/metrics` — `sources()` returns CPU, Memory, Disk, Network, Battery, Processes. Files: `cpu.rs`, `memory.rs`, `disk.rs`, `network.rs`, `battery.rs`, `processes.rs`, shared `pdh.rs` (PDH query wrapper), `util.rs`.

Live check: `cargo run -p busy-metrics --example dump_metrics [samples]`.

## Per source

| Source | API | Notes |
|---|---|---|
| CPU | PDH `\Processor Information(*)\% Processor Utility`, `% Privileged Utility`, `% Processor Performance` (English counters) | Utility exceeds 100 under turbo → total clamped; user/kernel split uses the *uncapped* privileged/total ratio (capping made user read 0). Freq = `% Processor Performance` × registry `~MHz` (fallback `CallNtPowerInformation`). Fallback path: `NtQuerySystemInformation(SystemProcessorPerformanceInformation)` (only the calling thread's processor group). |
| Memory | `GlobalMemoryStatusEx`, `GetPerformanceInfo` | `cached` = `SystemCache` × page size — lower than Task Manager's standby+modified. |
| Disk | PDH `\PhysicalDisk(*)` read/write bytes/s, `% Idle Time` | active = 100 − idle. Query rebuilt ≤ every 10 s on failure. Volumes (fixed drives) refreshed every 10 s. |
| Network | `GetIfTable2` for discovery (every 60 s or on IPv4 change), `GetIfEntry2` per sample, `GetUnicastIpAddressTable` every 10 s | `GetAdaptersAddresses` was 6–9 ms — don't reintroduce it on the hot path. |
| Battery | `GetSystemPowerStatus`; SetupDi + `IOCTL_BATTERY_*` for rate/capacity/cycles | Capacity refreshed every 60 s. Devices opened `GENERIC_READ` only: the three query IOCTLs encode `FILE_READ_ACCESS` (only `IOCTL_BATTERY_SET_INFORMATION` needs write). **Untested on real battery hardware** (dev machine is a desktop). |
| Processes | one `NtQuerySystemInformation(SystemProcessInformation)` into a reused buffer | Hand-defined `SYSTEM_PROCESS_INFORMATION` with compile-time size/offset asserts (x64). Deltas keyed by (pid, CreateTime). CPU% normalized to all logical CPUs. "Memory Compression" excluded from `by_mem`. |

## Network counting rule (totals)

Count an interface if: connected, not loopback, not a filter/LWF row, `HardwareInterface` set. Excludes vEthernet/vSwitch/VPN/tunnels whose traffic also crosses a physical NIC. If no hardware interface is connected, sum all connected non-filter non-loopback ones.

## Measured cost (release, i9-10900KF, ~310 processes)

CPU 0.6–5 ms · Memory <0.3 ms · Disk ~0.3 ms · Network ~0.5 ms · Battery µs · Processes 4–13 ms (scales with thread count). `sources()` construction 0.4–0.8 s (first PDH query).

## Gotchas

- First sample: Processes/Network rates are 0; CPU/Disk collect once in the constructor so they have data.
- PDH instance churn (hot-plugged disks, cores) must be tolerated — re-parse instance names each sample.
- A formatted PDH value is usable only when its `CStatus` is `PDH_CSTATUS_VALID_DATA` (0) or `PDH_CSTATUS_NEW_DATA` (1); any other status (e.g. `PDH_CALC_NEGATIVE_VALUE` on a rate counter whose instance just restarted) carries garbage and is skipped. `busy_win::pdh` applies this to every read.
- Decode PDH instance names with `String::from_utf16`, falling back to `from_utf16_lossy` only on error: the lossy decoder is instantiated in our crate, so in debug builds it made the GPU source (~600 `GPU Engine` instances here) 1.5 ms a tick slower.
