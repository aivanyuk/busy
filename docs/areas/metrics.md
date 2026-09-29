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
| Battery | `GetSystemPowerStatus`; SetupDi + `IOCTL_BATTERY_*` for rate/capacity/cycles | Capacity refreshed every 60 s. **Untested on real battery hardware** (dev machine is a desktop). |
| Processes | one `NtQuerySystemInformation(SystemProcessInformation)` into a reused buffer | Hand-defined `SYSTEM_PROCESS_INFORMATION` with compile-time size/offset asserts (x64). Deltas keyed by (pid, CreateTime). CPU% normalized to all logical CPUs. "Memory Compression" excluded from `by_mem`. |

## Network counting rule (totals)

Count an interface if: connected, not loopback, not a filter/LWF row, `HardwareInterface` set. Excludes vEthernet/vSwitch/VPN/tunnels whose traffic also crosses a physical NIC. If no hardware interface is connected, sum all connected non-filter non-loopback ones.

## Measured cost (release, i9-10900KF, ~310 processes)

CPU 0.6–5 ms · Memory <0.3 ms · Disk ~0.3 ms · Network ~0.5 ms · Battery µs · Processes 4–13 ms (scales with thread count). `sources()` construction 0.4–0.8 s (first PDH query).

Heap allocations per sample (release, `dump_metrics 60`, median): CPU 32 · Memory 0 · Disk 31 · Network 20 · Battery 0 · Processes 30. What remains is mostly the snapshot's own output (names, lists) and PDH instance strings.

## Gotchas

- First sample: Processes/Network rates are 0; CPU/Disk collect once in the constructor so they have data.
- PDH instance churn (hot-plugged disks, cores) must be tolerated — re-parse instance names each sample.
