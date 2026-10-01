# Testing

```
cargo test --workspace
```

## Layers

| Layer | Where | What |
|---|---|---|
| Unit | `#[cfg(test)] mod tests` next to the code | Pure logic: config normalize/serde, formatting (`crates/ui/src/fmt.rs`), history ring, PDH instance-name parsing, shared-memory parsers fed with synthetic bytes, network counting rule. |
| Smoke (real hardware) | `crates/metrics/tests/smoke.rs`, `crates/sensors/tests/smoke.rs` | Build all sources, sample twice, assert no panic and sane ranges. Hardware-dependent fields (GPU, battery, sensors) are range-checked only when present, so they pass on CI runners without a GPU. |
| Live dump | `cargo run -p busy-metrics --example dump_metrics`, `-p busy-sensors --example dump_sensors` | Human sanity check against Task Manager / `Get-Counter` / `nvidia-smi`. Prints per-source sample cost and heap allocations (a counting global allocator in the example). |
| UI manual | see below | Taskbar/flyout/settings can't be meaningfully unit-tested. |
| Windows versions | `tools/vm/guest.ps1` | The widget and flyout against the taskbar of the build it runs on (below). |

## What to test when

- New parsing of external data (struct layouts, instance names, shared memory): unit test with synthetic input, including truncated/oversized inputs.
- New `Config` field: default + roundtrip + old-JSON-without-field test.
- New formatter/unit conversion: table-driven unit test.
- New Source or backend: extend the smoke test; run the dump example and compare against a reference tool; record cost in the area doc.
- Bug fix: regression test at the lowest layer that can reproduce it.

## Manual UI check (any change under `app/` or `crates/settings/`)

1. `cargo run -p busy` (or `-- --open-flyout` to open the flyout after the first sample).
2. Screenshot the taskbar strip and the flyout; check both anchors (`NearTray`, `Left`) and both themes (set `theme` in `%APPDATA%\busy\config.json`).
3. Settings: `cargo run -p busy-settings --example demo`, with `BUSY_FORCE_DARK=1` and `0`, `BUSY_SETUP=1` for setup (onboarding), and `BUSY_NEWER=0.2.0` for About naming a newer release. The app's first run: start it with `APPDATA` pointing at an empty directory; setup opens once the widget is embedded. For UI Automation changes, read the window with a UIA client (Accessibility Insights, Narrator, or `System.Windows.Automation` from Windows PowerShell; see docs/areas/settings.md § Gotchas): names, roles, patterns, and the focus events as Tab moves.
4. Stop your instance (`Stop-Process -Name busy`). Never restart `explorer.exe` (outside a test VM, below); never use SendKeys/SendInput against windows you don't own.

## Windows versions

Supported: Windows 11 build 22631 (23H2) and later. What changes between builds is explorer's taskbar: the legacy windows `taskbar/explorer.rs` reads (`TrayNotifyWnd`, `Start`, `ReBarWindow32`) and how they track the XAML taskbar, alignment, and the DWM attributes the flyout sets. Monthly updates and staged feature rollouts change it too, so a result is for a build *and* its UBR.

`tools/vm/guest.ps1` runs a debug `busy.exe` with `BUSY_FAKE=1` (every cell has data in a VM) and `BUSY_SELFTEST` (docs/areas/app.md), once per variant (`anchor-theme-alignment-cells`, e.g. `tray-dark-center-all`), and checks:

- the widget is embedded in `Shell_TrayWnd`, visible, top of its siblings, inside the taskbar, anchored where `explorer::slot` puts it, and covers none of the buttons explorer exposes through UI Automation — the independent check on what the legacy windows told us; `start-landmark` warns when the legacy `Start` window no longer matches the XAML Start button;
- how many of the configured cells fit (a warning when some were dropped for room);
- each cell's flyout opens on a click posted to our widget, lies inside the work area 12 DIPs from the anchor's edge, got its backdrop, and closes on a second click;
- in a VM: the widget re-embeds after explorer restarts (`TaskbarCreated` and the watch timer), and after every variant `WM_CLOSE` exits busy cleanly without taking explorer down.

It writes `summary.json`, each variant's reports, the explorer buttons it compared against, and screenshots of the taskbar and every flyout (look at them: the checks can't judge clipping or colors).

**Anywhere (a probe):** `powershell -File tools\vm\guest.ps1 -Exe target\debug\busy.exe -Out target\vm\local` on your own machine, or on an ARM64 device, changes nothing outside `-Out` (a config of its own in `APPDATA`); variants whose alignment isn't the current one are skipped, and explorer is left alone. Exit busy first.

**In VMs:** with `-Vm`, which refuses to run outside a Hyper-V guest, each variant also sets the taskbar alignment and the system theme (light/dark) and restarts explorer. Start it in the signed-in user's session.

Verified so far (record each run here: build.UBR, scale, result):

- 26300.9457, the probe on a workstation: taskbar vertical on the right, left-aligned icons, 200 %. All checks pass (3 of 7 cells dropped for room with all on).
