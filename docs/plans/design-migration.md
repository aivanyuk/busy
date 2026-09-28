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

Each phase is one or more PRs of atomic commits (docs/commits.md).

### 0. Land the baseline
PRs in order: `busy-core` + workspace → `busy-metrics` → `busy-sensors` → `busy-settings` → `busy` app (integration). Gives a reviewed, working base to diff against.

### 1. Foundation
- `busy-core` Config v2 (additive, old files still load via `#[serde(default)]` + `normalize()`):
  - `CellStyle::Io`; per-module: `show_label`, `color: Option<u8>` (palette index), `color_by_load`, `interval: Option<u32>`; module options: CPU bar `Cores|Total`, disk drive, net `units: Bytes|Bits` + `interface: Auto|WiFi|Ethernet|<name>`, battery `show_remaining`, sensors `sensor` selection; general: `onboarded`, `default_interval`.
  - Allowed styles per module (design `MODS.styles`): CPU/GPU graph·text·bar, Memory bar·text·graph, Disk io·text·bar, Network io·graph, Battery text·bar, Sensors text·graph.
  - `opt_in: OptIn { public_ip, process_network, app_battery, memory_speed, third_party_sensors }`.
- `app/src/theme.rs`: design tokens 1:1 (`--tb`, `--fly`, `--fg/2/3`, `--hover`, `--active`, `--track`, `--well`, `--line`, `--grid`, `--accent`, `--link`, …) for dark and light; module palette `PAL` (6 colors per theme) and load colors `LOAD` (<60 / <85 / ≥85).
- Typography: Segoe UI Variable Text; tabular numerals (DirectWrite `DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES`).

### 2. Taskbar cells (`taskbar.rs`)
- One 40 px-high cell per enabled module, padding 0 8 px, radius 4, 2 px gap; 1 px × 20 px divider before the tray (NearTray anchor).
- Per-cell hit-testing, hover (`--hover`) via `TrackMouseEvent`, active (`--active`) while that module's flyout is open.
- Styles exactly per `MeterWidget`: Text (9 px label, letter-spacing .06em, `--fg3`; 13 px semibold value, min 32 px right-aligned), Graph (label+value row over 40×20 sparkline on `--track`, fill .3, stroke 1.2), Bar (26 px bars; CPU "cores" = 8 bars × 3 px from core pairs, else one 6 px bar), IO (2 rows, 11 px, bold colored key ↑/↓ or R/W, value min 60 px right-aligned).
- Color by load; per-module colors; value color follows load when enabled.
- Tooltip `"<Module>: <value>"`.
- Sampler: per-source interval (`module.interval.unwrap_or(default_interval)`).

### 3. Per-module flyout (`flyout.rs`)
- Click a cell → flyout for **that** module (toggle on re-click), anchored above the cell's side (left/right per anchor), 360 px, acrylic, radius 8.
- Layout (design `isDetailed`): header (title, hardware sub, big value + label) → 86 px chart (dashed grid at 25/50/75 %, area .22 + line 1.5, second series) with span/peak labels → optional segment bar → legend → core grid (16 cols) → stats grid 2 cols → bars section → top processes → footer ("Open Task Manager" link, "<Module> settings" button).
- Per-module content per design `fly()`; missing data rows are omitted, not faked.
- "Open Task Manager": `ShellExecuteW("taskmgr.exe")`. "<Module> settings": opens Settings at that module's page.
- New collector data (no opt-in needed — standard APIs, cheap):
  - Memory: in use / modified / standby / free (`NtQuerySystemInformation(SystemMemoryListInformation)` or PDH `\Memory\Modified Page List Bytes`, `Standby Cache *`), paged / non-paged pool (`GetPerformanceInfo`), hardware reserved (`GetPhysicallyInstalledSystemMemory` − total).
  - Disk: avg response time (PDH `Avg. Disk sec/Transfer`), read/written since start, volume → physical disk mapping for the per-drive setting.
  - GPU: driver version (DXGI `CheckInterfaceSupport` UMD version), DirectX feature level (`D3D12CreateDevice` probe or registry), engine filtering (hide idle engines).
  - Network: Wi-Fi SSID + signal (`WlanQueryInterface`), interface selection.

### 4. Settings window (custom D2D)
- New `crates/settings` implementation (replaces native controls; public API unchanged: `open`, `is_open`, `is_dialog_message`, `autostart`).
- Needs the renderer: move shared D2D/DWrite helpers + theme tokens from `app/` into a new `crates/ui` (or into `busy-settings` depending on size) so both use one source.
- Window: 1000×700 logical, custom title bar (min/max/close, `#C42B1C` close hover), Mica.
- Left nav: app header (icon + "N readings on the taskbar"), search box (visual; filters rows), General + one item per module (color dot, On/Off status, accent pill on selection).
- Pages: General (Behavior: Start with Windows, Widget position, Default update interval; Appearance: Theme, Run setup; Taskbar order with ↑/↓) and per-module (live preview cell on a taskbar strip; Taskbar: Show on taskbar, Style segmented, Show label, module options; Color: swatches, Color by load; Updates: interval with "Default (…)" option).
- Advanced page (new, not in design): opt-in sources, each a toggle + risk text + confirmation on enable.
- Controls: keyboard focus ring, Tab order, Space/Enter activation, arrow keys in segmented/dropdown, Esc closes popups; UIA via `UiaReturnRawElementProvider` basic roles (button, checkbox, combobox, list item) — at minimum names for screen readers.
- Changes apply live (design has no OK/Apply); persisted by the app as today.

### 5. Onboarding
- Shown when `onboarded == false`; re-runnable from Settings → General → Run setup.
- Reading cards (checkbox, name, live sample in module color), position radio cards, Start with Windows, Skip / Start monitoring. Toggling updates the real taskbar live.

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
