# Testing

```
cargo test --workspace
```

## Layers

| Layer | Where | What |
|---|---|---|
| Unit | `#[cfg(test)] mod tests` next to the code | Pure logic: config normalize/serde, formatting (`app/src/fmt.rs`), history ring, PDH instance-name parsing, shared-memory parsers fed with synthetic bytes, network counting rule. |
| Smoke (real hardware) | `crates/metrics/tests/smoke.rs`, `crates/sensors/tests/smoke.rs` | Build all sources, sample twice, assert no panic and sane ranges. Hardware-dependent fields (GPU, battery, sensors) are range-checked only when present, so they pass on CI runners without a GPU. |
| Live dump | `cargo run -p busy-metrics --example dump_metrics`, `-p busy-sensors --example dump_sensors` | Human sanity check against Task Manager / `Get-Counter` / `nvidia-smi`. Prints per-source sample cost and heap allocations (a counting global allocator in the example). |
| UI manual | see below | Taskbar/flyout/settings can't be meaningfully unit-tested. |

## What to test when

- New parsing of external data (struct layouts, instance names, shared memory): unit test with synthetic input, including truncated/oversized inputs.
- New `Config` field: default + roundtrip + old-JSON-without-field test.
- New formatter/unit conversion: table-driven unit test.
- New Source or backend: extend the smoke test; run the dump example and compare against a reference tool; record cost in the area doc.
- Bug fix: regression test at the lowest layer that can reproduce it.

## Manual UI check (any change under `app/` or `crates/settings/`)

1. `cargo run -p busy` (or `-- --open-flyout` to open the flyout after the first sample).
2. Screenshot the taskbar strip and the flyout; check both anchors (`NearTray`, `Left`) and both themes (set `theme` in `%APPDATA%\busy\config.json`).
3. Settings: `cargo run -p busy-settings --example demo`, with `BUSY_FORCE_DARK=1` and `0`.
4. Stop your instance (`Stop-Process -Name busy`). Never restart `explorer.exe`; never use SendKeys/SendInput against windows you don't own.
