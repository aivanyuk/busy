# busy-core

`crates/core` — the contract between collectors and UI. Pure Rust, no Win32, fully unit-testable.

## Source contract

```rust
pub trait Source {
    fn module(&self) -> Module;
    fn configure(&mut self, _opts: SourceOptions) {} // default: ignore
    fn sample(&mut self, snap: &mut Snapshot);
}
```

- `configure` is called on the sampler thread before the first `sample` and again whenever the `SourceOptions` change (also for inactive modules). `SourceOptions` (`opt_in.rs`, built by `Config::source_options()`) carries only the settings sources act on — today `third_party_sensors` — never the whole `Config` (architecture: workers get only what they use). A setting a source needs is added there, with a test. One field is not a setting: `network_processes` (the Network flyout is open and lists processes) is set by `sampler::Params::new` from the open flyout, because the Processes source that fills `top.by_net` also serves the CPU, Memory and Disk flyouts, which don't need it. `configure` must be cheap (no I/O — defer work to the next `sample`) and must not panic.

- Constructed and called only on the sampler thread (COM MTA initialized). Not `Send` on purpose.
- Must not panic; on failure leave fields untouched (`None` / empty).
- Panic policy: release builds keep `panic = "abort"`, and sources are **not** wrapped in `catch_unwind`. A panicking source ends the process; the no-panic rule is the guard, not isolation. Unwinding was rejected: it would need an unwind-safe boundary around shared `Rc<RefCell<_>>` GPU/Sensors state, which a panic mid-borrow leaves inconsistent, and it would grow the binary.
- Budget: a few ms per call at ~1 Hz. Anything slower (WMI, re-enumeration) is throttled/cached inside the source.
- Units: percentages 0..=100, bytes, bytes/s, °C, RPM, W, MHz.

## Snapshot semantics worth knowing

- Each module's source owns a fixed set of fields, and `Snapshot::clear(module)` drops exactly those: Disk owns `disks` and `volumes`, Gpu owns `gpus` and `top.by_gpu`, Processes the other `top` lists, the rest their one field. The sampler keeps one snapshot across ticks and clears a module before its source refills it, so modules can run at different intervals. A source must not write another module's fields; the one exception is the Sensors source filling third-party temperatures into `gpus`, which is why the sampler always runs GPU and Sensors together.
- `CpuInfo.total` matches Task Manager (`% Processor Utility`, clamped).
- `NetInfo.rx_bps/tx_bps` sum physical interfaces only (see metrics doc for the rule); `interfaces` lists more.
- `ProcEntry.io_bps` is **all** process I/O (read + write + other), not disk-only.
- `GpuInfo.util_pct` is the max engine utilization (Task Manager semantics).
- GPU fan % is reported as a `SensorKind::Load` reading named "GPU Fan" (no unit field yet).
- `MemInfo.used` = total − available and so includes the modified list; Task Manager's "In use" is `MemInfo::in_use()` (total − modified − standby − free), so the composition bar's four segments sum to `total`.

## Config

- The file is `%APPDATA%\busy\config.json`; a busy installed from the Microsoft Store keeps its own in `%LOCALAPPDATA%\Packages\<family>\LocalState\config.json`, once the app has called `Config::set_package` at startup (core has no Win32; `busy_win::package_family` tells). It is named outright because MSIX file-system virtualization redirects only new files under `%APPDATA%` and changes existing ones in place, which would share a portable busy's file. Uninstalling the package removes it.
- `Config::load()` never fails: missing/invalid file → defaults, then `normalize()` (dedup modules, append missing ones, clamp interval 250..10000 ms and history 10..3600 s).
- A file with a later `version` than `CONFIG_VERSION` (written by a newer busy) loads as far as this build understands it and is marked `newer` (not stored); `save` then refuses, so the file, with the fields this build doesn't know, survives going back a version. Changes still apply for the session.
- Schema version: `version` (`CONFIG_VERSION` = 2, `migrate.rs`). A file without it is v1 (the schema before the design migration); `normalize()` runs `Config::migrate()` first and stamps the current version. v1 → v2: `onboarded = true` (an existing file means an existing user, so first-run setup is skipped), Disk `Text` → `Io` (v1's Disk text cell showed read/write rates). A fresh install (no file) or an unreadable file gets `Config::default()`: v2, `onboarded = false`.
- `modules` order is display order; each has `taskbar`, `flyout`, `style` (Text/Graph/Bar/Io). Since flyouts are per module (a cell always opens its own), `flyout` only matters for Processes, where it shows or hides the top-process lists in the CPU, Memory and Disk flyouts.
- Per-module appearance (all optional in JSON): `show_label` (default true), `color` (palette index `0..PALETTE_LEN`, `None` = `Module::default_color()`, design `MODS.color`; out-of-range → `None`), `color_by_load` (load colors instead of the module color), `interval_s` (1..=10 s, `None` = general `interval_ms`; read through `Config::module_interval_ms`).
- Module-specific options live in `Config.options` (`ModuleOptions`, `options.rs`), one struct per module rather than on `ModuleCfg`, so a value can't sit on the wrong module and survives module reordering/dedup: `cpu.bar` (`Cores`|`Total`, for the Bar style), `disk.drive` (a `VolumeInfo::mount` such as `"D:"`, `None` = system drive), `network.units` (`Bytes`|`Bits`) and `network.interface` (`"Auto"`|`"WiFi"`|`"Ethernet"`|`{"Named": "<NetIf::name>"}` — a specific adapter by alias), `battery.show_remaining` (default true), `sensors.sensor` (`"Cpu"`|`"Gpu"`|`"Storage"`|`{"Named": "hardware/name"}`, resolved by the app's `select::taskbar_sensor`). v1's `pinned_sensor` is read only for migration (`legacy_pinned_sensor`, never serialized): non-empty → `Named`, empty → `Cpu`.
- `opt_in: OptIn` (`opt_in.rs`) — `public_ip`, `process_network`, `app_battery`, `memory_speed`, `third_party_sensors`, `update_check`, all default `false`. Each gates a data source that contacts an external service, needs admin, runs a slow API or reads another program's data (risks: docs/plans/design-migration.md § Opt-in sources). A source that is not opted in must not be touched at all, not merely hidden. Sources learn the switch through `SourceOptions`.
- `Module::allowed_styles()` lists the styles a module can use on the taskbar, preferred first (design `MODS.styles`): CPU/GPU Graph·Text·Bar, Memory Bar·Text·Graph, Disk Io·Text·Bar, Network Io·Graph, Battery Text·Bar, Sensors Text·Graph. `normalize()` coerces a disallowed style to the first allowed one. Processes allows none: it is flyout-only and `normalize()` clears its `taskbar`.
- `language: Option<Lang>` (`lang.rs`) — the UI language, stored by its BCP 47 tag (`"de"`, `"pt-BR"`, `"zh-Hans"`); `None` (the default) follows Windows' display languages through `Lang::pick`, the first one busy has, else English. Matching is by primary subtag (`pt-PT` gets Brazilian Portuguese); a traditional-script Chinese tag has no match rather than getting Simplified. A tag this build doesn't know (a newer busy's) reads as `None` instead of failing the file. The strings are `busy_ui::i18n`.
- Adding a field: give it a default in `impl Default for Config` — `#[serde(default)]` keeps old files loading. Add a test.
- A new `ModuleCfg` field needs its own `#[serde(default…)]` (the struct has no `Default`: `module` is required). A `modules` entry that still fails to parse — an unknown `Module` or `CellStyle` from a newer build, a hand-edit — is dropped on load and `normalize()` re-adds that module with its default; before, one bad entry reset the whole file.
- Adding a `Module` variant: update `Module::ALL`, `Config::default()`, its name in `busy_ui::i18n::Common` (and every language), and every exhaustive `match` in app/settings.

## Releases

`release.rs` is the update check's pure half: `newer(current, body)` reads GitHub's `releases/latest` JSON (`tag_name`, `html_url`, `draft`, `prerelease` only) and returns a `Release { version, url }` only for a published, non-pre-release `vX.Y.Z` tag above `current`, whose page starts with `RELEASES_URL/`, so the link About opens can only be one of busy's release pages. Versions compare as three numbers; any suffix or missing part means "not a version". The request is the app's (`app/src/update.rs`).

## Changing core types

Core is shared by four crates. Prefer additive changes (new `Option` fields). Removing/renaming a field is a cross-crate change — do it in one commit that builds the whole workspace.

## Pending suggestions (not yet done)

- `SensorReading.unit` or `SensorKind::FanPercent`.
- `ProcEntry.disk_bps` distinct from all-I/O.
- `CpuInfo.base_mhz`; `GpuInfo.power_limit_w`.
