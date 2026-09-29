# busy (app)

`app/` — the binary. Files: `main.rs` (DLL search restriction, single instance, `--open-flyout` debug flag), `app.rs` (state, config, message routing — the only module that changes state), `menu.rs` (the widget's context menu, returning a `menu::Command` the router applies), `win.rs` (window-class registration, `win::Event` routing), `ctx.rs` (read-only render context), `sampler.rs`, `worker.rs` (latest-value worker threads: config writer, theme reader), `select.rs` (what to show: the taskbar sensor), `tone.rs` (which design color: module palette, load colors), `sync.rs` (poison-tolerant lock), `history.rs`, `fake.rs` (synthetic Source for UI dev, debug builds only), `taskbar/` (`mod.rs` window and placement, `explorer.rs` taskbar probing and slot, `cells.rs` per-module cells, `surface.rs` DIB), `flyout/` (`mod.rs` window, `painter.rs` layout primitives, `sections.rs` per-module sections), `render.rs` (D2D/DWrite helpers), `theme.rs`, `fmt.rs`. Manifest: `app/app.manifest` embedded by `app/build.rs` via `/MANIFEST:EMBED /MANIFESTINPUT` (PerMonitorV2, comctl32 v6, supportedOS Win8/10 — required for layered child windows).

Verified on Windows 11 build 26200.9457 (25H2), 3840×2160 @ 200 %, centered taskbar icons. Release exe ~500 KB.

## Threads and state

- UI state lives in a `thread_local RefCell` accessed via `app::with()` using `try_borrow_mut`: re-entrant messages are skipped, never panic. Window procedures don't touch it: they `win::raise` an `Event`, and `app::on_event` (registered with `win::set_handler` before any window exists) routes it synchronously. Messages that can arrive during our own calls (flyout deactivate, re-render, new config) are posted back to ourselves.
- A hidden top-level `busy.main` window receives `TaskbarCreated`, `WM_SETTINGCHANGE("ImmersiveColorSet")`, snapshots (`WM_APP+1`) and new configs (`WM_APP_CONFIG`).
- `app::submit_config(Config)` is callable from any thread; the UI thread applies it live (sampler interval/modules, history size, theme, re-layout) and queues the save to the config writer thread (a `worker::Worker`), which writes one at a time and skips superseded configs. Config save never runs on the UI thread; load runs once at startup, before the widget is embedded.
- Sampler: own thread, COM MTA, sources built there; Mutex+Condvar carries `sampler::Params` (interval, active modules, `SourceOptions` — `Copy`, so nothing allocates under the lock; the sampler calls `Source::configure` when the options differ from the last ones applied) and the stop flag (interruptible waits, joined on drop, poison-tolerant).
- Debug hooks, compiled only under `cfg(debug_assertions)` so a release build ignores them: `BUSY_FAKE=1` swaps in `fake.rs` (fills every field) — for UI work on machines without GPU/battery/sensors; `BUSY_DUMP=<dir>` writes `taskbar.bmp`; `BUSY_PIN_FLYOUT=1` keeps the flyout open on focus loss; `--open-flyout` opens it after the first sample.

## Taskbar widget

- Hosting: `WS_CHILD` created directly with `Shell_TrayWnd` as parent (primary monitor only) — same end state as TrafficMonitor's `SetParent`. Win11 removed deskbands; this is undocumented and may need fixes after Windows updates.
- Rendering: `WS_EX_LAYERED | WS_EX_NOPARENTNOTIFY` child, D2D `ID2D1DCRenderTarget` (premultiplied BGRA) into a 32-bpp DIB → `UpdateLayeredWindow`; DWM honours per-pixel alpha on the layered child. Background alpha 1/255 so the whole widget is clickable. Returns `MA_NOACTIVATE`.
- Redraw only on change: each render builds a `Frame` (DPI, pixel size, hover, active, theme, and per cell its width plus a `cells::Key` of what its style draws: text and its color, bar fill in 1/256 steps, and for graphs each series' push count) and skips drawing and `UpdateLayeredWindow` when it equals the frame on screen. DWM keeps the layered window's last bitmap, so nothing has to be redrawn for it. Text and Bar cells change only when a formatted value changes; a Graph cell redraws on every new sample because the sparkline scrolls.
- Positioning (re-checked by a 1 s timer, moved only when the rect changes; the timer also re-embeds if the widget or taskbar died). The timer re-lays out only when the taskbar geometry (DPI, client rect, free slot) changed; otherwise it only restores z-order and visibility:
  - `NearTray`: left of `TrayNotifyWnd` — still present on 26200 and matching the XAML notification area.
  - `Left`: taskbar left edge + offset with centered icons; after the task list with left-aligned icons.
  - Task-button extent: `ReBarWindow32` is **not** kept in sync on 26200; the hidden `Start` window's rect is, and with centered icons it is mirrored around the taskbar centre to find the icon group's right end.
  - Cells that don't fit are dropped from the end of the config order — the widget never covers task buttons.
- Cells: one per module with `taskbar = true`, drawn in its `style`; `Io` draws the two-row read/write or up/down text. Processes is flyout-only (no cell, not in the context menu's "Show on taskbar").
- Z-order: `SetWindowPos(HWND_TOP)` above `Windows.UI.Composition.DesktopWindowContentBridge` (the XAML island); each tick re-raised if `GetWindow(GW_HWNDPREV)` finds a sibling above.
- Explorer restart: `RegisterWindowMessageW("TaskbarCreated")` → re-find and re-parent. Exit: `App`'s `Drop` destroys the child and the flyout first, then joins the sampler (joining while the child exists would freeze the taskbar for as long as a WMI call runs).

**Critical:** cross-process parenting attaches our input queue to explorer's. Any blocking on the UI thread freezes the user's taskbar. No sampling, file I/O, or waits on the UI thread.

## Flyout

`WS_POPUP` tool window, topmost, rounded corners, acrylic (`DWMSBT_TRANSIENTWINDOW`), dark-mode attribute, frame extended over the client area; drawn with `ID2D1HwndRenderTarget` (premultiplied) plus a translucent tint; solid fallback if the backdrop attribute fails (Win10). Height fits content, clamped to the work area, wheel scrolling with indicator; redraws per snapshot. A render lays out once, while painting: only when that layout's height differs from the window's does it resize and paint again (the first frame after a height change is painted twice, which is rare). `SetWindowPos` runs only when the window rect changed; opening measures first so the first frame is painted at its final size. A pointer move repaints only when the hover target changes: the hit rect under it (tabs) or the graph and sample index under it, computed from the rects the last paint recorded. The graph readout is placed from the sample, not the pointer, so its horizontal position depends on the sample alone; its vertical position is the pointer's as of that repaint. Closes on Esc, re-click and focus loss; a 250 ms guard after deactivation handles the click-toggle race.

## Theme

Taskbar follows `SystemUsesLightTheme` (not `AppsUseLightTheme`) unless `Config.theme` overrides. Reacts to `WM_SETTINGCHANGE` "ImmersiveColorSet". The registry read runs on the theme reader thread (`WM_APP_THEME` brings the resolved `Theme` back), except the first resolution at startup, before the widget exists.

`theme.rs` holds the design's colors 1:1 (`design/Meterbar.dc.html`): one `Theme` field per token (`--tb` → `tb`, `--fg2` → `fg2`, `--on-accent` → `on_accent`, …) plus the module palette `pal` (`PAL`, 6 per theme) and `load` (`LOAD`), in two const tables, dark and light; resolving a theme only picks one. The accent is the design's fixed `--accent`, not the system accent color, so nothing reads `AccentPalette` or DWM colorization any more. Settings-window tokens (`--win`, `--card`, `--ctl*`, `--pop`, `--knob`) come with Phase 4.

- Module colors are decided in `tone.rs` (pure, tested) as a `Tone` — `Fg`, `Pal(i)` or `Load(level)` — and `Theme::color` maps it to this theme's value. Module color: `pal[ModuleCfg::color_index()]` (`color`, else `Module::default_color()`). Graphs and bars use `tone::fill` (module color, or the load color when `color_by_load`); values use `tone::value` (`fg`, or the load color). Load steps: < 60 → normal, < 85 → elevated, else high (`LOAD`).
- Rates (Network, Disk) pair the module color (download, read) with `tone::SECOND` = `pal[4]` (upload, write), and ignore `color_by_load` (no percentage).
- Sensors: color by load reads a temperature in °C as the percentage (design); temperatures are not tinted otherwise. Battery (`tone::battery`): its load is depletion (100 − charge); below 20 % on battery it is always the high load color.
- Widget background: `active` while the flyout is open (`App::sync_active` after every show/hide, including hide-on-deactivate and Esc; `Taskbar::set_active` reports a change, so it redraws only then), else `hover` under the mouse. Per-cell hover/active is Phase 2.
- Flyout border: `DWMWA_BORDER_COLOR` = `fly_line` blended over `fly` (COLORREF has no alpha; ignored before Win11).
- Labels `fg3`; bar and sparkline tracks `track`; flyout chart background `well` with `grid` lines; selected Processes tab `accent` / `on_accent`; the flyout paints the translucent `fly` over its acrylic backdrop (opaque `fly` without one).

## Typography

Segoe UI Variable Text (fallback Segoe UI). Numbers use tabular figures (design `tabular-nums`): `Gfx` creates one `IDWriteTypography` with `DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES` at startup, and every measurement plus every drawn string containing a digit goes through `Gfx::layout`, an `IDWriteTextLayout` carrying it (`DrawText` can't carry typography, hence `DrawTextLayout`). Without it Segoe UI Variable's digits are proportional ("1111" is 20 DIPs, "8888" 27 at 12 px), so values changing every second jitter. Strings without digits (constant labels) still go through `DrawText`, which needs no layout of ours. Formats stay cached per window. Measured (release, i9-10900KF, 13.5 px): create + measure a layout 10.6 µs with the typography against 6.5 µs without; a taskbar render measures and draws a few dozen strings, so tens of µs per render.

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
