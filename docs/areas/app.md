# busy (app)

`app/` — the binary. Files: `main.rs` (single instance, `--open-flyout` debug flag), `app.rs` (state, config, message routing), `sampler.rs`, `history.rs`, `fake.rs` (synthetic Source for UI dev), `taskbar.rs`, `flyout.rs`, `render.rs` (D2D/DWrite helpers), `theme.rs`, `fmt.rs`. Manifest: `app/app.manifest` embedded by `app/build.rs` via `/MANIFEST:EMBED /MANIFESTINPUT` (PerMonitorV2, comctl32 v6, supportedOS Win8/10 — required for layered child windows).

## Taskbar widget

- Hosting: our window is `WS_CHILD` of `Shell_TrayWnd` (primary monitor only), `SetParent` cross-process (TrafficMonitor approach). Win11 removed deskbands; this is undocumented and may need fixes after Windows updates.
- Rendering: layered child window, D2D `ID2D1DCRenderTarget` (premultiplied BGRA) into a 32-bit DIB → `UpdateLayeredWindow`. Background alpha = 1 (not 0) so clicks hit us.
- Positioning: `Anchor::NearTray` = left of `TrayNotifyWnd`; `Anchor::Left` = taskbar left edge; plus DPI-scaled `offset_px`. Re-checked each render tick, repositioned only on change.
- Explorer restart: handle `RegisterWindowMessageW("TaskbarCreated")` → re-find and re-parent.
- On exit, destroy the child window so explorer isn't left with a dead child.

**Critical:** cross-process parenting attaches our input queue to explorer's. Any blocking on the UI thread freezes the user's taskbar. No sampling, file I/O, or waits on the UI thread.

## Flyout

Top-level `WS_POPUP` with `WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, opened above the widget, clamped to the work area. Dismissed on deactivate, Esc, or re-click (watch the deactivate→click toggle race). DWM rounded corners + transient-window backdrop + immersive dark mode per theme. Sections follow `Config.modules` order where `flyout = true`.

## Theme

Taskbar follows `SystemUsesLightTheme` (not `AppsUseLightTheme`) unless `Config.theme` overrides. Reacts to `WM_SETTINGCHANGE` "ImmersiveColorSet". Accent from DWM colorization / registry.

## Integration points

- Sources: in `sampler.rs`, sources are built on the sampler thread (`busy_metrics::sources()` + `busy_sensors::sources()`; `fake.rs` during UI-only work).
- Settings: context menu "Settings…" → `busy_settings::open(hwnd, &cfg, on_apply)`; message loop calls `busy_settings::is_dialog_message` first.

> This doc is written from the design spec; update it with measured facts (z-order under Win11 XAML, verified Windows build, known issues) as they are confirmed.
