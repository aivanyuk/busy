# busy

Lightweight Windows 11 system monitor (macOS "Stats"-like): a widget embedded in the taskbar, flyout with details on click. Rust, Win32 via the `windows` crate only.

## Commands

```
cargo build -p busy                      # app (debug)
cargo build -p busy --release            # shipping exe: target/release/busy.exe
cargo fmt --all                          # format (rustfmt.toml: max_width 120)
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                   # unit + real-hardware smoke tests
cargo run -p busy-metrics --example dump_metrics   # print a live Snapshot
cargo run -p busy-sensors --example dump_sensors
cargo run -p busy-settings --example demo          # settings window standalone
```

Definition of done for any change: fmt clean, clippy clean with `-D warnings`, tests pass. Changes to UI must be run and visually checked (see docs/testing.md).

## Hard rules

- No new crates beyond `windows`, `serde`, `serde_json` without explicit user approval. No kernel drivers, no admin requirement.
- Vendor DLLs load only from System32 (`LoadLibraryExW` + `LOAD_LIBRARY_SEARCH_SYSTEM32`).
- The UI thread never blocks: our taskbar window is a child of explorer's `Shell_TrayWnd`, so a stalled UI thread freezes the user's taskbar.
- `Source::sample()` never panics.
- Never kill or restart `explorer.exe`; never send synthetic input (SendKeys/SendInput) to windows you don't own — post messages to our own HWNDs instead.
- Don't leave autostart (`HKCU\...\Run\busy`) enabled after testing.
- Never commit or push to `main`. All changes land via PR from a feature branch, as atomic commits that each build, pass checks and do one reviewable thing (docs/commits.md). Rebase-merge only.

## Docs — read the one for the area you touch

| Area | Doc |
|---|---|
| Workspace layout, data flow, threading | [docs/architecture.md](docs/architecture.md) |
| Shared types, `Source` contract, `Config` | [docs/areas/core.md](docs/areas/core.md) |
| CPU/memory/disk/network/battery/processes collectors | [docs/areas/metrics.md](docs/areas/metrics.md) |
| GPU, NVML/ADL/D3DKMT, LHM/HWiNFO sensors | [docs/areas/sensors.md](docs/areas/sensors.md) |
| Taskbar widget, flyout, rendering, theme | [docs/areas/app.md](docs/areas/app.md) |
| Shared UI code: formatting, selection, history | [docs/areas/ui.md](docs/areas/ui.md) |
| Settings window, autostart | [docs/areas/settings.md](docs/areas/settings.md) |
| Code style, unsafe, errors, lints | [docs/code-quality.md](docs/code-quality.md) |
| Test strategy and manual UI checks | [docs/testing.md](docs/testing.md) |
| Commit/branch conventions, automated PR review | [docs/commits.md](docs/commits.md), `.github/review-rules.md` |
| UI design source + migration plan | [docs/plans/design-migration.md](docs/plans/design-migration.md), `design/` |

When you learn something non-obvious about an area (Win32 quirk, measured cost, layout gotcha), add it to that area's doc in the same change.
