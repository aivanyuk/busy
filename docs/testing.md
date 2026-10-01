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
| Windows versions | `tools/vm/` | The widget and flyout against each supported build's taskbar, in Hyper-V VMs (below). |

## What to test when

- New parsing of external data (struct layouts, instance names, shared memory): unit test with synthetic input, including truncated/oversized inputs.
- New `Config` field: default + roundtrip + old-JSON-without-field test.
- New formatter/unit conversion: table-driven unit test.
- New Source or backend: extend the smoke test; run the dump example and compare against a reference tool; record cost in the area doc.
- Bug fix: regression test at the lowest layer that can reproduce it.

## Manual UI check (any change under `app/` or `crates/settings/`)

1. `cargo run -p busy` (or `-- --open-flyout` to open the flyout after the first sample).
2. Screenshot the taskbar strip and the flyout; check both anchors (`NearTray`, `Left`) and both themes (set `theme` in `%APPDATA%\busy\config.json`).
3. Settings: `cargo run -p busy-settings --example demo`, with `BUSY_FORCE_DARK=1` and `0`, `BUSY_SETUP=1` for setup (onboarding), `BUSY_NEWER=0.2.0` for About naming a newer release, and `BUSY_PACKAGED=1` for the window as installed from the Microsoft Store (About without Releases, no update check). The app's first run: start it with `APPDATA` pointing at an empty directory; setup opens once the widget is embedded. For UI Automation changes, read the window with a UIA client (Accessibility Insights, Narrator, or `System.Windows.Automation` from Windows PowerShell; see docs/areas/settings.md § Gotchas): names, roles, patterns, and the focus events as Tab moves.
4. Stop your instance (`Stop-Process -Name busy`). Never restart `explorer.exe` (outside a test VM, below); never use SendKeys/SendInput against windows you don't own.

## Store package

`pwsh tools/pack-msix.ps1` builds the release exe and packs `target\msix\busy-X.Y.Z-x64.msix` (unsigned: the Store signs it). It needs `makepri.exe` and `makeappx.exe`: an installed Windows SDK, or, with nothing installed, the `Microsoft.Windows.SDK.BuildTools` package from nuget.org unpacked into `target\sdk-buildtools\<version>` (the `.nupkg` is a zip; about 21 MB). `makeappx` validates the manifest against its schema as it packs. `-Layout` stops at the unpacked layout in `target\msix\layout`.

Checking it installed (any change to the package, or to what a packaged busy does differently: config path, autostart, update check). Needs Developer Mode, which only the user turns on:

1. `pwsh tools/pack-msix.ps1 -Layout`, then `Add-AppxPackage -Register target\msix\layout\AppxManifest.xml` (no signature needed for a registered layout). Exit any running busy first: the single-instance mutex is shared, so whichever starts second exits.
2. Start it as Windows does: `Start-Process "shell:AppsFolder\<family>!busy"` (`(Get-AppxPackage tmik.busysystemmonitor).PackageFamilyName`); `(Get-Process busy).Path` is in the layout. A fresh package has no config, so setup opens.
3. Start with Windows, from setup or Settings (toggled through UI Automation from Windows PowerShell, as for any UIA check): the startup task's state is the `State` DWORD under `HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\SystemAppData\<family>\busy` (0 off, 1 turned off by the user, 2 on; the key appears on the first change). Turning it on from busy sets 2 without a prompt; off sets 0. Setting 1 by hand stands in for Windows Settings → Apps → Startup: turning it on then shows the "turned off for busy in Windows Settings" message and the toggle stays off. The `Run` value must not change.
4. Finish setup: `config.json` appears in `%LOCALAPPDATA%\Packages\<family>\LocalState` and `%APPDATA%\busy\config.json` keeps its time stamp. Exit (`WM_CLOSE` to `busy.main`) and start it again: no setup, the widget is back.
5. The widget is a visible child of `Shell_TrayWnd`; a click posted to it opens the flyout, and its "<Module> settings" button opens Settings, whose taskbar button shows the package's logo. Settings → General → About has no Releases button, Advanced no update check. `Get-StartApps` lists "busy — system monitor".
6. `target\release\busy.exe` started while the packaged one runs exits at once.
7. Clean up: exit busy, `Get-AppxPackage tmik.busysystemmonitor | Remove-AppxPackage` (it takes the startup task, its registry key and `LocalState` with it).

Not covered on a workstation: the next sign-in starting busy (it means signing out), and re-embedding after explorer restarts (a VM only, above).

Verified so far: 26300.9457, 3840×2160 @ 200 %, taskbar on the right edge: every step above passes.

## Windows versions

Supported: Windows 11 build 22631 (23H2) and later. What changes between builds is explorer's taskbar: the legacy windows `taskbar/explorer.rs` reads (`TrayNotifyWnd`, `Start`, `ReBarWindow32`) and how they track the XAML taskbar, alignment, and the DWM attributes the flyout sets. Monthly updates and staged feature rollouts change it too, so a result is for a build *and* its UBR.

`tools/vm/guest.ps1` runs a debug `busy.exe` with `BUSY_FAKE=1` (every cell has data in a VM) and `BUSY_SELFTEST` (docs/areas/app.md), once per variant (`anchor-theme-alignment-cells`, e.g. `tray-dark-center-all`), and checks:

- the widget is embedded in `Shell_TrayWnd`, visible, top of its siblings, inside the taskbar, anchored where `explorer::slot` puts it, and covers none of the buttons explorer exposes through UI Automation — the independent check on what the legacy windows told us; `start-landmark` warns when the legacy `Start` window no longer matches the XAML Start button;
- how many of the configured cells fit (a warning when some were dropped for room);
- each cell's flyout opens on a click posted to our widget, lies inside the work area 12 DIPs from the anchor's edge, got its backdrop, and closes on a second click;
- in a VM: the widget re-embeds after explorer restarts (`TaskbarCreated` and the watch timer), and after every variant `WM_CLOSE` exits busy cleanly without taking explorer down.

It writes `summary.json`, each variant's reports, the explorer buttons it compared against, and screenshots of the taskbar and every flyout (look at them: the checks can't judge clipping or colors).

**Anywhere (a probe):** `powershell -File tools\vm\guest.ps1 -Exe target\debug\busy.exe -Out target\vm\local` on your own machine, or on an ARM64 device, changes nothing outside `-Out` (a config of its own in `APPDATA`); variants whose alignment isn't the current one are skipped, and explorer is left alone. Exit busy first.

**In VMs:** with `-Vm`, which refuses to run outside a Hyper-V guest, each variant also sets the taskbar alignment and the system theme (light/dark) and restarts explorer. `tools\vm\run.ps1` drives a set of VMs from an elevated shell on the host:

```
cargo build -p busy
tools\vm\run.ps1 -VMName busy-22631, busy-26100, busy-26200, busy-insider -Credential (Get-Credential tester)
```

It reverts each VM to its `busy-ready` checkpoint, copies the exe in over PowerShell Direct, starts `guest.ps1` in the signed-in user's session through a scheduled task (PowerShell Direct has no desktop), and collects the results into `target\vm\<time>\<vm>`, with a table of failures and warnings per VM and variant at the end.

Setting up a VM, once per build:

1. Hyper-V (Windows Pro/Enterprise host). A Generation 2 VM with TPM, 4 GB, `Set-VM -CheckpointType Standard` (the checkpoint keeps the signed-in desktop).
2. Install from an ISO: the current release from Microsoft's download page, older builds from Visual Studio subscription downloads, Insider builds from the Insider ISO page. One VM per build that matters: 22631, 26100 (24H2, also LTSC 2024), 26200 (25H2), and an Insider channel to see explorer changes coming.
3. A local administrator account with a password, signed in automatically (Sysinternals Autologon). Install the VC++ x64 runtime (`vcruntime140.dll`; busy loads it from System32 only). Pause Windows Update so the build stays put; record build and UBR in the VM's notes.
4. Connect with a Basic session (not Enhanced, which is RDP: a different session, DPI and lock behaviour), set the resolution and scale to test (one per VM, or one VM per scale), sign in, and checkpoint as `busy-ready`.

Hyper-V on an x64 host runs x64 Windows only. For ARM64, CI's `ui-probe` job runs `guest.ps1 -Vm` on the hosted `windows-11-arm` runner (and on `windows-latest` as the control) and uploads the results as `ui-probe-<os>-<sha>`, with failed checks listed in the job summary; it is experimental and never fails the build until the ARM runner's taskbar works (below). Otherwise: the probe on an ARM64 device, or `run.ps1` on an ARM64 host with ARM64 VMs.

Verified so far (record each run here: build.UBR, scale, result):

- 26300.9457, the probe on a workstation: taskbar vertical on the right, left-aligned icons, 200 %. All checks pass (3 of 7 cells dropped for room with all on).
- 26100.33438 Server Datacenter x64, CI `windows-latest`, 1024×768 @ 100 %, `-Vm`: every check passes in all five variants (185), including re-embedding after each explorer restart; 1 of 4 cells fits with centered icons (86 px of room).
- 26200.9457 Enterprise ARM64, CI `windows-11-arm`, `-Vm`: explorer's taskbar was empty (no XAML island, zero-size legacy windows, the sign-in screen's accessibility button in the capture), so busy found no room and stayed hidden: a machine problem, not busy's. Not a session problem: the runner's session is the active console session and LogonUI isn't running. The job's Machine step and the probe's `screen-*.png` (taken when the taskbar is empty) are for finding out why.
