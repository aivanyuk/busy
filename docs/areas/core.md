# busy-core

`crates/core` — the contract between collectors and UI. Pure Rust, no Win32, fully unit-testable.

## Source contract

```rust
pub trait Source {
    fn module(&self) -> Module;
    fn sample(&mut self, snap: &mut Snapshot);
}
```

- Constructed and called only on the sampler thread (COM MTA initialized). Not `Send` on purpose.
- Must not panic; on failure leave fields untouched (`None` / empty).
- Budget: a few ms per call at ~1 Hz. Anything slower (WMI, re-enumeration) is throttled/cached inside the source.
- Units: percentages 0..=100, bytes, bytes/s, °C, RPM, W, MHz.

## Snapshot semantics worth knowing

- `CpuInfo.total` matches Task Manager (`% Processor Utility`, clamped).
- `NetInfo.rx_bps/tx_bps` sum physical interfaces only (see metrics doc for the rule); `interfaces` lists more.
- `ProcEntry.io_bps` is **all** process I/O (read + write + other), not disk-only.
- `GpuInfo.util_pct` is the max engine utilization (Task Manager semantics).
- GPU fan % is reported as a `SensorKind::Load` reading named "GPU Fan" (no unit field yet).

## Config

- `Config::load()` never fails: missing/invalid file → defaults, then `normalize()` (dedup modules, append missing ones, clamp interval 250..10000 ms and history 10..3600 s).
- `modules` order is display order; each has `taskbar`, `flyout`, `style` (Text/Graph/Bar). Processes is flyout-only.
- Adding a field: give it a default in `impl Default for Config` — `#[serde(default)]` keeps old files loading. Add a test.
- Adding a `Module` variant: update `Module::ALL`, `label()`, `Config::default()`, and every exhaustive `match` in app/settings.

## Changing core types

Core is shared by four crates. Prefer additive changes (new `Option` fields). Removing/renaming a field is a cross-crate change — do it in one commit that builds the whole workspace.

## Pending suggestions (not yet done)

- `SensorReading.unit` or `SensorKind::FanPercent`.
- `ProcEntry.disk_bps` distinct from all-I/O.
- `CpuInfo.base_mhz`; `MemInfo.standby/modified` (Task Manager "Cached"); `GpuInfo.power_limit_w`.
