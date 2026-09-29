# busy (app)

`app/` — the binary. Files: `main.rs` (DLL search restriction, single instance, `--open-flyout` debug flag), `app.rs` (state, config, message routing — the only module that changes state), `win.rs` (window-class registration, `win::Event` routing), `ctx.rs` (read-only render context), `sampler.rs`, `worker.rs` (latest-value worker threads: config writer, theme reader), `select.rs` (what to show: the taskbar sensor), `sync.rs` (poison-tolerant lock), `history.rs`, `fake.rs` (synthetic Source for UI dev, debug builds only), `taskbar/` (`mod.rs` window and placement, `explorer.rs` taskbar probing and slot, `cells.rs` per-module cells, `surface.rs` DIB), `flyout/` (`mod.rs` window, `painter.rs` layout primitives, `sections.rs` per-module sections), `render.rs` (D2D/DWrite helpers), `theme.rs`, `fmt.rs`. Manifest: `app/app.manifest` embedded by `app/build.rs` via `/MANIFEST:EMBED /MANIFESTINPUT` (PerMonitorV2, comctl32 v6, supportedOS Win8/10 — required for layered child windows).

Verified on Windows 11 build 26200.9457 (25H2), 3840×2160 @ 200 %, centered taskbar icons. Release exe ~500 KB.

## Threads and state

- UI state lives in a `thread_local RefCell` accessed via `app::with()` using `try_borrow_mut`: re-entrant messages are skipped, never panic. Window procedures don't touch it: they `win::raise` an `Event`, and `app::on_event` (registered with `win::set_handler` before any window exists) routes it synchronously. Messages that can arrive during our own calls (flyout deactivate, re-render, new config) are posted back to ourselves.
- A hidden top-level `busy.main` window receives `TaskbarCreated`, `WM_SETTINGCHANGE("ImmersiveColorSet")`, colorization changes, snapshots (`WM_APP+1`) and new configs (`WM_APP_CONFIG`).
- `app::submit_config(Config)` is callable from any thread; the UI thread applies it live (sampler interval/modules, history size, theme, re-layout) and queues the save to the config writer thread (a `worker::Worker`), which writes one at a time and skips superseded configs. Config save never runs on the UI thread; load runs once at startup, before the widget is embedded.
- Sampler: own thread, COM MTA, sources built there; Mutex+Condvar carries `sampler::Params` (interval, active modules, `SourceOptions` — `Copy`, so nothing allocates under the lock; the sampler calls `Source::configure` when the options differ from the last ones applied) and the stop flag (interruptible waits, joined on drop, poison-tolerant).
- Debug hooks, compiled only under `cfg(debug_assertions)` so a release build ignores them: `BUSY_FAKE=1` swaps in `fake.rs` (fills every field) — for UI work on machines without GPU/battery/sensors; `BUSY_DUMP=<dir>` writes `taskbar.bmp`; `BUSY_PIN_FLYOUT=1` keeps the flyout open on focus loss; `--open-flyout` opens it after the first sample.

## Taskbar widget

- Hosting: `WS_CHILD` created directly with `Shell_TrayWnd` as parent (primary monitor only) — same end state as TrafficMonitor's `SetParent`. Win11 removed deskbands; this is undocumented and may need fixes after Windows updates.
- Rendering: `WS_EX_LAYERED | WS_EX_NOPARENTNOTIFY` child, D2D `ID2D1DCRenderTarget` (premultiplied BGRA) into a 32-bpp DIB → `UpdateLayeredWindow`; DWM honours per-pixel alpha on the layered child. Background alpha 1/255 so the whole widget is clickable. Returns `MA_NOACTIVATE`.
- Positioning (re-checked by a 1 s timer, moved only when the rect changes; the timer also re-embeds if the widget or taskbar died):
  - `NearTray`: left of `TrayNotifyWnd` — still present on 26200 and matching the XAML notification area.
  - `Left`: taskbar left edge + offset with centered icons; after the task list with left-aligned icons.
  - Task-button extent: `ReBarWindow32` is **not** kept in sync on 26200; the hidden `Start` window's rect is, and with centered icons it is mirrored around the taskbar centre to find the icon group's right end.
  - Cells that don't fit are dropped from the end of the config order — the widget never covers task buttons.
- Cells: one per module with `taskbar = true`, drawn in its `style`; `Io` draws the two-row read/write or up/down text. Processes is flyout-only (no cell, not in the context menu's "Show on taskbar").
- Z-order: `SetWindowPos(HWND_TOP)` above `Windows.UI.Composition.DesktopWindowContentBridge` (the XAML island); each tick re-raised if `GetWindow(GW_HWNDPREV)` finds a sibling above.
- Explorer restart: `RegisterWindowMessageW("TaskbarCreated")` → re-find and re-parent. Exit: `App`'s `Drop` destroys the child and the flyout first, then joins the sampler (joining while the child exists would freeze the taskbar for as long as a WMI call runs).

**Critical:** cross-process parenting attaches our input queue to explorer's. Any blocking on the UI thread freezes the user's taskbar. No sampling, file I/O, or waits on the UI thread.

## Flyout

`WS_POPUP` tool window, topmost, rounded corners, acrylic (`DWMSBT_TRANSIENTWINDOW`), dark-mode attribute, frame extended over the client area; drawn with `ID2D1HwndRenderTarget` (premultiplied) plus a translucent tint; solid fallback if the backdrop attribute fails (Win10). Height fits content, clamped to the work area, wheel scrolling with indicator; redraws per snapshot. Closes on Esc, re-click and focus loss; a 250 ms guard after deactivation handles the click-toggle race.

## Theme

Taskbar follows `SystemUsesLightTheme` (not `AppsUseLightTheme`) unless `Config.theme` overrides. Reacts to `WM_SETTINGCHANGE` "ImmersiveColorSet" and `WM_DWMCOLORIZATIONCOLORCHANGED`. Accent from DWM colorization / registry. The registry reads run on the theme reader thread (`WM_APP_THEME` brings the result back), except the first resolution at startup, before the widget exists.

## Integration points

- Sources: `sampler.rs` `build_sources()` — `busy_metrics::sources()` then `busy_sensors::sources()` (GPU before Sensors).
- Settings: context menu "Settings…" → `busy_settings::open(main, &cfg, Box::new(submit_config))`; the message loop calls `busy_settings::is_dialog_message` first.

## Gotchas

- `windows` uses `windows-numerics::Vector2` for D2D points without re-exporting it; instead of adding the crate, `render::point` builds them with a size-checked `transmute_copy`.
- `#![allow(linker_messages)]` in `main.rs`: link.exe's manifest validator doesn't know `<dpiAwareness>` (warning 81010002); it is embedded anyway.
- Screenshots: PowerShell isn't DPI-aware, so coordinates are virtualized and `CopyFromScreen` is wrong unless the thread calls `SetThreadDpiAwarenessContext(-4)` first.
- Test interaction by posting messages to our own windows only — never synthetic input.
- DLL search is System32-only for the whole process. `/DEPENDENTLOADFLAG:0x800` (`app/build.rs`) covers the exe's static imports: with copies of `pdh`, `powrprof`, `dwmapi`, `dwrite` and `vcruntime140` planted next to the exe, a build without it loads all five from the exe's directory, with it all from System32. `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)` at the top of `main` covers later loads. Consequences: `VCRUNTIME140.dll` must come from the installed VC++ runtime (app-local deployment no longer works); comctl32 v6 still resolves through the manifest to WinSxS; NVML's System32 `nvml.dll` still loads the DriverStore copy (by full path); WMI's in-proc servers load from `System32\wbem`.

## Known issues

- Right-click menu stays light in dark mode (dark menus need undocumented uxtheme calls).
- Not exercised live: theme-change reaction, explorer restart path, graph hover readout.
- Left anchor overlaps the Win11 Widgets button if enabled (`offset_px` works around it).
- Left-aligned taskbars fall back to `ReBarWindow32` for the icon-group extent, which is stale on 26200.
