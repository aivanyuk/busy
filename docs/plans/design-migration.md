# Plan: migrate UI to the Meterbar design

Source of truth: `design/` (exported from Claude Design project `363af25b-1580-4dfb-ad86-243ea6d22a22`).

| File | What it specifies |
|---|---|
| `design/MeterWidget.dc.html` | One taskbar cell: Text / Graph / Bar / IO styles, sizes, typography |
| `design/Meterbar.dc.html` | Taskbar strip, per-module flyouts (3 layouts), Settings window, onboarding; theme tokens; all per-module options and data shown (`widget()`, `fly()`, `rows()`) |
| `design/Meterbar Overview.dc.html` | Canvas of the above in dark/light |
| `design/support.js` | Design runtime only — not used by the app |

## Decisions

- App name stays **busy** (design calls it "Meterbar"; only the name differs).
- Flyout layout: **Detailed (1a)**, 360 px. Compact/Tiles are not implemented.
- Settings window: **custom-drawn Direct2D**, reusing `app/src/render.rs`; own controls (toggle, dropdown, segmented, swatch, card, nav item) with keyboard navigation and basic UI Automation. No new dependencies.
- Data needing external services, admin rights, heavy/unsupported APIs or third-party tools is **opt-in**, off by default, each with a stated risk (see Opt-in sources).
- Baseline (current code) lands via PRs first; migration phases follow as separate PRs.

## Phases

Each phase is one or more PRs of atomic commits (docs/commits.md) and follows `docs/architecture.md`: window modules report input as `win::Event`s and never `use crate::app` (only `app.rs` changes state), no `pub` fields on window-owning structs, choices of what to show in a pure tested module (`select.rs`, `tone.rs` or `busy-core`), Win32 helpers only from `busy-win`, a `// SAFETY:` comment on every new or changed `unsafe` block, debug switches under `cfg(debug_assertions)`, and no file past 500 lines (split it in the PR that grows it). Every UI PR carries the manual check of `docs/testing.md`.

### 0. Land the baseline
PRs in order: `busy-core` + workspace → `busy-metrics` → `busy-sensors` → `busy-settings` → `busy` app (integration). Gives a reviewed, working base to diff against.

### 1. Foundation

**Status: done** (branch `feat/config-v2`). As built:
- Config v2 in `busy-core`: `CellStyle::Io` and `Module::allowed_styles()` (normalize coerces, Processes flyout-only); per-module `show_label`, `color` (palette index), `color_by_load`, `interval_s`; module options in `Config.options` (`options.rs`, one struct per module; network interface `Auto | WiFi | Ethernet | Named(alias)`; `sensors.sensor` replaces `pinned_sensor`); `opt_in: OptIn` (`opt_in.rs`); a schema `version` with v1 migration (`migrate.rs`: `onboarded = true`, `pinned_sensor` → `options.sensors.sensor`, Disk Text → Io). The general interval stays `interval_ms` (clamped 250..10000 ms; the design's 1/2/5 s are values of it).
- Sources get settings only as `SourceOptions` (`Config::source_options()`, carried in `sampler::Params`) through `Source::configure`; LHM/HWiNFO are read only with `opt_in.third_party_sensors`.
- `theme.rs`: the design's tokens, `PAL` and `LOAD` 1:1 as two const tables, fixed `--accent`; the theme reader resolves only light/dark. `tone.rs` (pure) decides module, load and battery colors; `Theme::color` maps them.
- Widget active background while the flyout is open (`Taskbar::set_active`, redraw on change); flyout border `--fly-line`; tabular figures through one cached `IDWriteTypography` (`Gfx::layout`).
- Not yet read by the UI: `show_label`, `interval_s`, the module options except the sensor pick, and every opt-in except `third_party_sensors`.

### 2. Taskbar cells (`app/src/taskbar/`)
- `cells.rs` becomes the cell model and measuring (one 40 px-high cell per enabled module, padding 0 8 px, radius 4, 2 px gap; 1 px × 20 px `--tb-line` divider before the tray for NearTray); drawing per style moves to its own file (e.g. `taskbar/styles.rs`) so neither passes 500 lines. `mod.rs` keeps the window, placement and the hit rectangles of the last layout.
- Styles exactly per `MeterWidget`: Text (9 px label, letter-spacing .06em, `--fg3`; 13 px semibold value, min 32 px right-aligned; label hidden when `show_label` is off), Graph (label+value row over 40×20 sparkline on `--track`, fill .3, stroke 1.2), Bar (26 px bars; CPU `options.cpu.bar` Cores = 8 bars × 3 px from core pairs, else one 6 px bar), IO (2 rows, 11 px, bold colored key ↑/↓ or R/W, value min 60 px right-aligned); Disk Text/Bar show `options.disk.drive`, Network honours `units` and `interface`, Battery `show_remaining`. Colors from `tone.rs` (already per module and by load).
- Per-cell hit-testing in the window procedure, which raises `win::Event`s carrying the cell's `Module` (hover, click, menu) instead of `WidgetHover`/`WidgetClick`; the router keeps which cell is hovered/active and tells the taskbar through methods (`set_hover(Option<Module>)`, `set_active(Option<Module>)` returning whether a redraw is due). **A pointer move repaints only when the hovered cell changes** (F3); `TrackMouseEvent` for leave.
- **Redraw only on change** (closes the "taskbar redraws every snapshot and every watch tick" gap): `render` compares the drawn state (cell texts, colors, sparkline samples, hover/active, size) with the last one and skips drawing, `UpdateLayeredWindow` and `SetWindowPos` when equal; the 1 s watch timer re-checks position only. Brushes, formats and constant-string widths are cached per DPI and theme (F4).
- Tooltip `"<Module>: <value>"` (tooltip control owned by the taskbar window; text set on hover change only).
- Sampler: per-source interval `interval_s.map(×1000).unwrap_or(interval_ms)` (`Config::module_interval_ms`), carried in `sampler::Params` as a `Copy` array indexed like `Module::ALL`; the loop waits for the earliest due source. Params stay `Copy` so the lock holds only copies (T6). Network interface choice reaches the Network source as a new `SourceOptions` field if it filters there.

### 3. Per-module flyout (`app/src/flyout/`)
- Click a cell → flyout for **that** module (toggle on re-click of the same cell, switch on another), anchored above the cell's side (left/right per anchor), 360 px, acrylic, radius 8. The router owns which module is open (`win::Event` from the taskbar → `app.rs` → `Flyout::show(module, …)`); `mod.rs` keeps the window, `painter.rs` the primitives, and each module's content goes in its own file under `flyout/sections/` once `sections.rs` would pass 500 lines.
- **Sample only what is visible** (F1, closes the "flyout-only modules are sampled with the flyout closed" gap): `sampler::Params::active` becomes taskbar cells ∪ the open flyout's module, recomputed by the router on show/hide; a flyout-only module (Processes) is sampled only while its flyout is open, and the first sample after opening is requested immediately (as a config change does today). Pausing while the session is locked or the display is off lands here too (`WTSRegisterSessionNotification`, `GUID_CONSOLE_DISPLAY_STATE`), posted to the main window.
- Layout (design `isDetailed`): header (title, hardware sub, big value + label) → 86 px chart (dashed `--grid` at 25/50/75 %, area .22 + line 1.5, second series) with span/peak labels → optional segment bar → legend → core grid (16 cols) → stats grid 2 cols → bars section → top processes → footer (`--footer`, "Open Task Manager" `--link`, "<Module> settings" button).
- Per-module content per design `fly()`; missing data rows are omitted, not faked; rows of an opt-in that is off are hidden.
- **Repaint on change only**: pointer moves repaint only when the hover target changes (hit rectangle or graph sample index), not on every `WM_MOUSEMOVE` (closes the flyout part of the redraw gap); snapshots repaint only while visible.
- "Open Task Manager": `ShellExecuteW("taskmgr.exe")`. "<Module> settings": opens Settings at that module's page (a new `busy_settings::open` argument or page field, keeping the rest of the API).
- New collector data (no opt-in needed — standard APIs, cheap; each source in its own file, buffers reused per tick, smoke test + `dump_*` cost in the area doc):
  - Memory: in use / modified / standby / free (`NtQuerySystemInformation(SystemMemoryListInformation)` or PDH `\Memory\Modified Page List Bytes`, `Standby Cache *`), paged / non-paged pool (`GetPerformanceInfo`), hardware reserved (`GetPhysicallyInstalledSystemMemory` − total).
  - Disk: avg response time (PDH `Avg. Disk sec/Transfer`), read/written since start, volume → physical disk mapping for the per-drive setting.
  - GPU: driver version (DXGI `CheckInterfaceSupport` UMD version), DirectX feature level (`D3D12CreateDevice` probe or registry), engine filtering (hide idle engines, a choice for `select.rs`).
  - Network: Wi-Fi SSID + signal (`WlanQueryInterface`), interface selection.

### 4. Settings window (custom D2D, `crates/settings/src/window/`)
- New implementation of the `window/` modules (replaces native controls; public API unchanged: `open`, `is_open`, `is_dialog_message`, `autostart`), one concern per file as today: window + message routing (`wndproc.rs`), layout, paint, one file per control kind (toggle, dropdown, segmented, swatch, card, nav item), one per page, `config.rs` for Config ↔ controls. No window-owning struct with `pub` fields; controls report input to the page as typed events rather than calling into it.
- **Registry I/O stays on the window's worker** (`window/worker.rs`, T5): autostart read/write and the app theme are requested and posted back; nothing on a message path touches the registry or files. The window stays hidden until the first read, as today.
- Needs the renderer: move shared D2D/DWrite helpers (`render.rs`), `theme.rs` and `tone.rs` from `app/` into a new `crates/ui` (or into `busy-settings` depending on size) so both use one source — a mechanical move commit ahead of the rewrite, with the layering table in `docs/architecture.md` updated. Formats, brushes and constant widths cached per DPI and theme (F4); repaint on hover-target change only (F3).
- Window: 1000×700 logical, custom title bar (min/max/close, `#C42B1C` close hover), Mica.
- Left nav: app header (icon + "N readings on the taskbar"), search box (visual; filters rows), General + one item per module (color dot, On/Off status, accent pill on selection).
- Pages: General (Behavior: Start with Windows, Widget position, Default update interval; Appearance: Theme, Run setup; Taskbar order with ↑/↓) and per-module (live preview cell on a taskbar strip; Taskbar: Show on taskbar, Style segmented from `allowed_styles()`, Show label, module options; Color: swatches, Color by load; Updates: interval with "Default (…)" option).
- Advanced page (new, not in design): opt-in sources, each a toggle + risk text + confirmation on enable.
- Controls: keyboard focus ring, Tab order, Space/Enter activation, arrow keys in segmented/dropdown, Esc closes popups; UIA via `UiaReturnRawElementProvider` basic roles (button, checkbox, combobox, list item) — at minimum names for screen readers.
- Changes apply live (design has no OK/Apply) through `on_apply` → `app::submit_config`; the app persists them through the config writer as today.

### 5. Onboarding
- Shown when `onboarded == false`; re-runnable from Settings → General → Run setup. Lives with the Phase 4 window code (same controls and worker), a page set of its own; the router opens it at startup after the widget is embedded.
- Reading cards (checkbox, name, live sample in module color), position radio cards, Start with Windows (written on the settings worker), Skip / Start monitoring. Toggling updates the real taskbar live through `submit_config`; the sampler follows through `Params` like any config change.

## Opt-in sources

All default **off**, listed under Settings → Advanced with the risk shown verbatim. Enabling asks for confirmation.

| Source | What it enables | Risk shown to user |
|---|---|---|
| Public IP lookup | "Public IP" in Network flyout | Contacts an external service (endpoint shown, configurable) which sees your IP; cached 10 min, only while flyout is open. |
| Per-process network (ETW) | "Top processes" in Network flyout | Requires running busy as administrator; kernel event tracing session. |
| Per-app battery usage | "Power usage" in Battery flyout | Requires administrator; reads the Windows SRUM database. Experimental. |
| Memory speed (WMI) | "Speed" in Memory flyout | One slow WMI query at startup (~100 ms+ on the sampler thread). |
| Third-party sensor tools | Reading LibreHardwareMonitor (WMI) / HWiNFO (shared memory) | Reads data published by another program you installed. Needed for CPU temps, fans, SSD temp/health, CPU power, throttling. |

When a source is off, its rows/sections are hidden (never shown as fake data). NVML/ADL/D3DKMT GPU sensors stay on (first-party driver APIs, no extra risk).

## Out of scope

Compact/Tiles flyout layouts; rename; multi-monitor taskbars.
