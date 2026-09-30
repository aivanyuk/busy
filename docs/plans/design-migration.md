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
- Settings window: **custom-drawn Direct2D**, reusing `app/src/render.rs`; own controls (toggle, dropdown, segmented, swatch, card, nav item) with keyboard navigation and basic UI Automation. No new dependencies (`windows-core`, which `windows` is built on, became a direct one for UI Automation's `#[implement]`, with the user's approval).
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

**Status: done** (branch `feat/taskbar-cells`, on top of the #14 re-land, which brought redraw-on-change, flyout hover repaints, sampling flyout-only modules only while the flyout is open, and pausing on lock/display off). As built:
- `cells.rs` decides a cell's content (label, value, colors, series) as a `Body` per style; `styles.rs` measures and draws it to `MeterWidget`: 40-DIP cells, padding 8, 2 DIPs apart, radius-4 hover/active backgrounds; 9 px labels with .06em tracking (`Gfx::text_width_tracked`/`Canvas::text_tracked`), hidden with `show_label` off; Text, Graph (40×20, last 20 samples), Bar and Io exactly per the design. NearTray draws a 1×20 `--line` divider before the tray (the design uses `--line`, not `--tb-line`). Label and key widths are cached.
- Module options (`select.rs`, tested): CPU Cores bars (up to 8, each the mean of an equal run of logical processors), Disk Text/Bar = used share of `disk.drive` (else the system drive) labelled with its letter (Io stays all-disk rates, as in the design), Network `interface` and `units`, Battery `show_remaining` (`h:mm` label while discharging).
- Per-cell hit-testing (`HITS`, pointer x → `Module`), `WidgetHover(Option<Module>)` and `WidgetClick(Module)`; the router's `open_cell` is drawn `--active`; redraws only when the hovered or open cell changes. Tooltip "<Module>: <value>" (`tip.rs`).
- Per-module intervals: the sampler keeps one snapshot across ticks (`Snapshot::clear` per module), samples only due modules (`sampler::Schedule`, pure and tested), GPU and Sensors together; history is sized and pushed per module (`history::Fresh`).
- Deviations: the Battery value has no charging bolt and the Network graph no value, as in the design; the interface choice is applied in the app (`select::net_rates`), so sources need no new `SourceOptions`.
- Until Phase 3, clicking any cell opens the full flyout (a click on another cell while it is open closes it, like a re-click).

### 3. Per-module flyout (`app/src/flyout/`)

**Status: done** (branch `feat/module-flyout`). As built:
- A cell opens its module's flyout, a re-click closes it, a click on another cell switches in place: the widget answers `MA_NOACTIVATE`, so a click on it never takes activation from the open flyout and no race with the deactivation guard arises. `Flyout::open_module` is the one source of the active cell. Placement is the design's: 12 DIPs in from the work area's edge on the widget's side (not aligned to the widget).
- Sampling: taskbar cells ∪ the open module ∪ what its flyout borrows (`Module::flyout_needs`: CPU and Disk borrow Processes and Sensors, Memory Processes). `ModuleCfg::flyout` now only matters for Processes (its top-process lists); the other modules' flags are ignored.
- Layout: `flyout/detail.rs` (what a module's flyout shows) + `flyout/modules/<module>.rs` (design `fly()` from real data, rows without data left out) + `flyout/draw.rs` (the design's detailed measures, chart hover readout, footer). The old sections, painter and Processes tabs are gone; top processes are part of the CPU, Memory, Disk and GPU flyouts, with the design's `tile` placeholder as icon.
- Footer: "Open Task Manager" (`ShellExecuteW`, System32 path, on a new launcher worker) and "<Module> settings" (`busy_settings::open(…, Some(module))` selects the row in today's native window).
- Choices added to `select.rs`: busiest GPU, busy engines (idle ones dropped), the interface the Network flyout describes, a part's temperature without fallback.
- Not built (opt-ins, Phase 4 UI): Public IP, per-process network, per-app battery usage, memory speed. Real process icons need file I/O off the UI thread and are left out.

### 4. Settings window (custom D2D, `crates/settings/src/window/`)

**Status: done.** Landed in three PRs (#25, #26, #27):
1. `refactor/ui-crate`: the shared code moved into `busy-ui` (`crates/ui`), which holds formatting, selection, history, tones, theme, the D2D helpers, the draw context and the taskbar cell.
2. `feat/settings-window`: the window itself, as built:
   - Design measures and tokens, including the new `--win`/`--card`/`--ctl-strong`/`--pop`/`--knob` tokens.
   - A custom title bar with working Minimize and Maximize, and a solid `--win` background, as the design has no Mica.
   - Nav, pages of one card per setting, and the taskbar order list.
   - Controls, one file per kind: toggle, dropdown with popup, segmented, swatches, order, nav, preview.
   - The Keyboard section below, and search, which filters rows across all pages.
   - The page model is pure and tested: `model`, `choices`, `edit`.
   - **API change** (the user chose live data over a static preview):
     - `open` takes a `Host` (`apply`, `page`, `with_data`) instead of `on_apply`.
     - The app calls `refresh(snap, hist)` after each sample and `sync(cfg)` after each applied config.
     - The shown page's module is sampled (`Params::with_preview`, since generalized to `with_shown`).
   - Kept from the native window as extra rows: Offset, History, and Processes' "Top processes" page.
   - Named adapters and sensors are offered from live data.
   - Not built: page subtitles with hardware names (they are static descriptions). The "Run setup" row came with Phase 5.
3. `feat/settings-advanced`: the Advanced page, which lists the one opt-in that is built (third-party sensors) with its risk text and a confirmation, and UI Automation.

The original plan follows.

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

**Status: done.** As built:
- Setup is the settings window's second mode (`window/setup/`), so it shares the window's render target, worker, theme, frame, keyboard focus and UI Automation. It is a fixed 680-DIP page with Close only, titled "busy". The app opens it at startup while `onboarded` is false (`busy_settings::setup`), and Settings → General → Appearance → Setup → "Run setup" turns the window into it.
- Reading cards (one per module with a cell, in taskbar order), with the live reading from `busy_ui::cell::sample` in the module's palette color. The host samples every card's module while setup is up: `Host::page` became `Host::shown(&[Module])`.
- Choices apply live through `Host::apply`, so the taskbar follows them. Skip, Start monitoring, Close, Esc and Enter all keep them, set `onboarded` and close.
- Decided with the maintainer:
  - Start with Windows starts as the registry has it (unchecked on a fresh install), not checked as in the design.
  - The checked readings are `Config::default`'s (CPU, Memory, GPU, Network), not the design's (CPU, Memory, Disk, Network).
  - Setup has UI Automation too (checkboxes, radio buttons, buttons).

The original plan follows.

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
| Check for updates | A newer release named in Settings → General → About (docs/plans/release.md R5) | Contacts api.github.com once a day while busy runs, to see whether a newer release exists; GitHub sees your IP address. Nothing is downloaded or installed. |

When a source is off, its rows/sections are hidden (never shown as fake data). NVML/ADL/D3DKMT GPU sensors stay on (first-party driver APIs, no extra risk).

## Out of scope

Compact/Tiles flyout layouts; rename; multi-monitor taskbars.
