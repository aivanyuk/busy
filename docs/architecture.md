# Architecture

## Workspace

| Crate | Path | Role |
|---|---|---|
| `busy-core` | `crates/core` | Shared types (`Snapshot`, `*Info`), `Source` trait, `Module`, `Config` (+ JSON persistence). No Win32. |
| `busy-metrics` | `crates/metrics` | Collectors: CPU, memory, disk, network, battery, top processes. |
| `busy-sensors` | `crates/sensors` | GPU (DXGI/PDH/D3DKMT/NVML/ADL) and temperature/fan sensors (LHM WMI, HWiNFO shared memory). |
| `busy-settings` | `crates/settings` | Modeless native settings window, autostart registry. |
| `busy` | `app` | Binary: sampler thread, history, taskbar widget, flyout, theme. |

Dependency direction: `busy` → {`busy-metrics`, `busy-sensors`, `busy-settings`} → `busy-core`. Collector and UI crates never depend on each other; everything they share goes through `busy-core`.

## Data flow

```
sampler thread (COM MTA)                       UI thread (message loop)
  sources: Vec<Box<dyn Source>>                  Config (source of truth, persisted)
  every interval_ms:                             History (rolling series)
    snap = Snapshot::default()                   taskbar widget (child of Shell_TrayWnd)
    for s in active sources: s.sample(&mut snap) flyout (top-level popup)
    hand snap to UI + PostMessage(WM_APP) ──────▶ on WM_APP: push history, redraw
  ◀── config changes (interval / active modules)  settings window (busy-settings)
```

- One fresh `Snapshot` per tick; each source fills only its part. Sources are ordered: the GPU source runs before the Sensors source (they share state via `Rc<RefCell<_>>`, which is why `Source` has no `Send` bound).
- Sources are constructed **on** the sampler thread after `CoInitializeEx(COINIT_MULTITHREADED)` and never leave it.
- Sources skipped when `Config::is_active(module)` is false.
- Rates are computed inside sources from deltas; the first sample may have zero rates.

## Threads

| Thread | May block? | Owns |
|---|---|---|
| UI | **Never** — input queue is attached to explorer's taskbar thread via cross-process parenting | windows, D2D resources, `Config`, history |
| Sampler | Yes (PDH, WMI, NtQuerySystemInformation) | all `Source`s, COM MTA |

## Persistence

- Config: `%APPDATA%\busy\config.json` (atomic write via temp + rename). Unknown/missing fields fall back to defaults (`#[serde(default)]`), then `Config::normalize()`.
- Autostart: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\busy` (+ `StartupApproved\Run` flag), owned by `busy-settings`.
