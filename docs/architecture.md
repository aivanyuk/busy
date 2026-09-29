# Architecture

## Workspace

| Crate | Path | Role |
|---|---|---|
| `busy-core` | `crates/core` | Shared types (`Snapshot`, `*Info`), `Source` trait, `Module`, `Config` (+ JSON persistence). No Win32. |
| `busy-win` | `crates/win` | Win32 plumbing used by more than one crate: wide strings, registry reads, the System32-only `Dll` loader, the PDH wrapper (`pdh`). |
| `busy-metrics` | `crates/metrics` | Collectors: CPU, memory, disk, network, battery, top processes. |
| `busy-sensors` | `crates/sensors` | GPU (DXGI/PDH/D3DKMT/NVML/ADL) and temperature/fan sensors (LHM WMI, HWiNFO shared memory). |
| `busy-settings` | `crates/settings` | Modeless native settings window, autostart registry. |
| `busy-ui` | `crates/ui` | What the windows show, shared by the app and the settings window: formatting, choices of what to show, rolling history. |
| `busy` | `app` | Binary: sampler thread, history, taskbar widget, flyout, theme. |

Dependency direction: `busy` → {`busy-metrics`, `busy-sensors`, `busy-settings`, `busy-ui`} → {`busy-core`, `busy-win`}. Collector and UI crates never depend on each other; data they share goes through `busy-core`, Win32 helpers through `busy-win`.

## Layering

Each crate may depend only on the crates in its row, and exposes only what its row lists. A new `use busy_*` or `[dependencies]` entry outside this table is a design change and is argued in the PR.

| Crate | May depend on | Exposes |
|---|---|---|
| `busy-core` | `serde`, `serde_json` | Data types, `Source`, `Module`, `Config`. No Win32, no threads; its only I/O is `Config::load`/`save`. |
| `busy-win` | `windows` | Win32 plumbing shared by more than one crate: wide strings, registry reads, the System32 DLL loader, the PDH wrapper. No `busy-*` dependency and no policy (it never decides *what* to read). |
| `busy-metrics`, `busy-sensors` | `busy-core`, `busy-win` | `sources()` only (plus examples and tests). |
| `busy-settings` | `busy-core`, `busy-win` | `open`, `is_open`, `is_dialog_message`, `autostart`. |
| `busy-ui` | `busy-core` | `fmt`, `select`, `history`. |
| `busy` | all of the above | The binary. |

Inside the `busy` app crate, modules form layers too. A module may use the ones below it, never above:

| Layer | Modules | Role |
|---|---|---|
| Router | `app.rs` | Owns the `App` state; the **only** module that turns events into state changes. |
| Windows | `taskbar/`, `flyout/`, `menu.rs` | Own an HWND (or a popup menu) and its GPU resources; draw from a read-only `Ctx`; report input as `win::Event`s, or return the menu's choice as a `menu::Command`. |
| Workers | `sampler.rs`, `worker.rs` | Threads with a small, typed hand-off to the router. |
| Support | `win.rs`, `ctx.rs`, `render.rs`, `theme.rs`, `launch.rs` | Window-class and message plumbing, render context, D2D/DWrite helpers, palette, starting other programs. |
| Pure logic | `tone.rs`, and `busy_ui::{select, fmt, history}` | No Win32; unit-tested. |

Rules:

- **Lower layers never name higher ones.** Window modules do not `use crate::app`: input goes up as a `win::Event` to the handler the router registered, or as a message posted to the main window. Only the router calls into more than one window.
- **No `pub` fields on a window-owning struct.** Its HWND, visibility and hover state change only through its methods, so the invariants (z-order, hide timestamps, repaint on change) live in one place.
- **Domain decisions live in pure modules.** Choosing what to show (the pinned sensor, the busiest GPU) is in `busy_ui::select` or `busy-core`, with tests, not in the router or a window procedure.
- **One implementation of each Win32 helper.** Wide-string conversion, registry reads, DLL loading and the PDH wrapper exist once, in `busy-win`; kernel handles are held in `windows::core::Owned`. A new `encode_utf16().chain(..)`, `RegGetValueW`, `PdhOpenQueryW`, `LoadLibraryExW` or bare `CloseHandle` elsewhere is a duplicate.
- **One concern per file, 500 lines at most.** A file that passes 500 lines, or that combines a window procedure with layout and painting, is split in the PR that grows it.
- **Workers get only what they use.** The sampler receives `sampler::Params` (per-module intervals, active modules and the `SourceOptions` its sources act on), not the whole `Config`.
- **Items in private modules are `pub(crate)` or private**, never plain `pub`, so the crate's real surface is its `lib.rs`.

## Data flow

```
sampler thread (COM MTA)                       UI thread (message loop)
  sources: Vec<Box<dyn Source>>                  Config (source of truth, persisted)
  snap: Snapshot (kept across ticks)             History (rolling series)
  when a module is due (its own interval):       taskbar widget (child of Shell_TrayWnd)
    snap.clear(m); m's sources sample(&mut snap) flyout (top-level popup)
    hand (snap, fresh) + PostMessage(WM_APP) ───▶ on WM_APP: push fresh history, redraw
  ◀── Params (intervals / active / SourceOptions) settings window (busy-settings)
                                                  │ on_apply → submit_config → WM_APP_CONFIG
config writer thread ◀── latest Config ───────────┘ (persisted off the UI thread)
```

- One `Snapshot` kept across ticks; each source fills only its part (`Snapshot::clear` names it), and a module is cleared and refilled when it is due, at `Config::module_interval_ms`. The UI gets a copy plus the set of modules refreshed since it last took one, and pushes history only for those. Sources are ordered: the GPU source runs before the Sensors source (they share state via `Rc<RefCell<_>>`, which is why `Source` has no `Send` bound).
- Sources are constructed **on** the sampler thread after `CoInitializeEx(COINIT_MULTITHREADED)` and never leave it.
- Sources skipped when `Config::is_active(module, open)` is false: a module is sampled for its taskbar cell, and otherwise only while the open flyout shows it, as its own module or as data it borrows (`Module::flyout_needs`: processes and temperatures).
- Settings reach sources only as `SourceOptions` (`Config::source_options()`), carried in `sampler::Params`: the sampler calls `Source::configure` on every source before the first tick and whenever the options change, on the sampler thread. Sources never see the `Config`.
- Rates are computed inside sources from deltas; the first sample may have zero rates.
- A config from another thread (the settings window's `on_apply`) is parked in a latest-wins slot and applied on `WM_APP_CONFIG`.

## Threads

Every thread the app starts is listed here.

| Thread | May block? | Owns |
|---|---|---|
| UI | **Never** — input queue is attached to explorer's taskbar thread via cross-process parenting | windows, D2D resources, `Config`, history |
| Sampler | Yes (PDH, WMI, NtQuerySystemInformation) | all `Source`s, COM MTA |
| Config writer | Yes (file I/O) | the one `config.json` writer; writes the latest submitted `Config`, older pending ones are dropped |
| Theme reader | Yes (registry) | resolves `Theme` (light or dark, from `SystemUsesLightTheme`) on theme broadcasts and config changes, posts it back |
| Launcher | Yes (shell) | starts Task Manager for the flyout's "Open Task Manager" (`launch::task_manager`: `ShellExecuteW` with the System32 path, in its own STA) |
| Settings registry (one per open settings window) | Yes (registry) | reads autostart and `AppsUseLightTheme`, writes autostart, posts results to the window; ends with the window, never joined |
| Debug dump (debug builds, per render) | Yes (file I/O) | one `taskbar.bmp` write |

Rules:

- **No wait on the UI thread while a child of `Shell_TrayWnd` exists.** That covers `JoinHandle::join`, blocking `recv`, `Condvar::wait` and `WaitFor*`. Shutdown destroys the taskbar widget first and only then stops and joins workers.
- **No registry or file API is reachable from a window procedure or message handler.** A result that needs one (theme, autostart state) is computed on a worker and posted back. Startup code that runs before the widget is embedded (`Config::load`, the first theme resolution) is exempt.
- **Critical sections on a mutex shared with a worker contain only moves, swaps and `Copy` reads**: no allocation, I/O or FFI under the lock.
- **Config persistence goes through the config writer**; never a `thread::spawn` per save.

## Performance

Budget: sampler total well under 50 ms per tick; a UI render a few ms (`docs/code-quality.md`). Rules that keep it there:

- **Sample only what is visible.** A module is sampled only while one of its surfaces (taskbar cell, open flyout section) needs it; the sampler idles while the session is locked or the display is off.
- **Sources reuse buffers.** Per-tick containers are fields that are cleared and refilled; no mapping or handle is opened per tick, and no per-tick copy is larger than the data actually parsed.
- **Redraw only on change.** A render path compares against the last drawn state and skips drawing, `UpdateLayeredWindow` and `SetWindowPos` when nothing differs. Timers re-check position; they do not force a repaint.
- **Pointer moves repaint only when the hover target changes** (hit rectangle or graph sample index).
- **No D2D/DWrite object creation in a paint path for constant inputs.** Brushes, formats and the widths of constant strings are cached per DPI and theme.
- **Release profile settings change only with measured size/CPU numbers** in the commit body.

## Security baseline

- **Every new or changed `unsafe` block carries a `// SAFETY:` comment** naming the invariant it relies on (pointer validity, buffer length, handle ownership).
- **DLL search is System32-only for the whole process**: static imports through `/DEPENDENTLOADFLAG:0x800`, dynamic loads through `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)` at the top of `main`, vendor DLLs as in `CLAUDE.md`.
- **Debug hooks are debug-only.** Environment switches and command-line flags that change behaviour (`BUSY_FAKE`, `BUSY_DUMP`, `BUSY_PIN_FLYOUT`, `--open-flyout`) are compiled in only under `cfg(debug_assertions)`.
- **Least privilege.** Handles are opened with the narrowest access mask the call needs (`PROCESS_QUERY_LIMITED_INFORMATION`, `GENERIC_READ` for query-only IOCTLs).
- **Foreign data is bounded.** Counts and lengths from WMI, shared memory and the config file are capped before they size an allocation or a loop.
- **CI** pins third-party actions by commit SHA, declares least-privilege `permissions:`, and audits `Cargo.lock` against the RustSec database.

## Known gaps

Existing code that does not yet meet the rules above. A PR that fixes one removes its row; a PR must not add one.

| Gap | Rule |
|---|---|
| About 2 `SAFETY` comments for about 200 `unsafe` blocks. | Security: `SAFETY` comments |
| LHM row count and string lengths, and the config file size, are unbounded. | Security: foreign data |

## Persistence

- Config: `%APPDATA%\busy\config.json` (atomic write via temp + rename). Unknown/missing fields fall back to defaults (`#[serde(default)]`), then `Config::normalize()`.
- Autostart: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\busy` (+ `StartupApproved\Run` flag), owned by `busy-settings`.
