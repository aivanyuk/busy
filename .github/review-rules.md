# Review rules — what an automated reviewer checks a PR against

This is the rubric for the automated PR reviewer (`.claude/commands/pr-review.md`, fired by a GitHub
`pull_request` webhook into a cloud routine, with a daily cron behind it as a net). It is an **index
of checkable assertions**, not a second copy of the doctrine. `CLAUDE.md` and `docs/` are
authoritative, and every rule below points at the passage that states it. **When this file
disagrees with them, they win and this file is the bug.** A rubric that drifts from the rules it
summarises fails reviews for rules nobody holds any more.

Each rule is an assertion a reviewer can hold a diff against, plus the diff shape that makes it
worth checking. Rules that a tool already decides are listed under "Already enforced" and are not
the reviewer's business, because a model's guess does not improve on an exact check.

The reviewer runs in a Linux cloud sandbox. busy is a Windows-only binary on the `windows` crate, so
the reviewer cannot build, lint, test or run it, and `cargo` there proves nothing. Whether a change
compiles, passes clippy and tests, or looks right on a taskbar is CI's job and the author's; the
reviewer reads the diff and the PR body, and judges what can be judged from them.

## How to use it

1. Read the diff, and collect the rule IDs whose *Trigger* the diff matches. Skip the rest.
2. For each collected rule, read the authority passage before judging. The reasons carry the
   exceptions, and several of these rules have one.
3. Report a deviation as `path:line`, the rule ID, what the code does, and the fix. A finding with
   no rule ID and no passage behind it is an opinion, and is labelled `nit`.

Severity: **`rule`** — deviates from a stated rule; **`bug`** — wrong regardless of any rule;
**`question`** — looks like a deviation but the diff alone cannot tell; **`nit`** — style or taste,
no rule behind it. Nothing here blocks a merge.

## P — Process

| ID | Assertion | Trigger |
| --- | --- | --- |
| P1 | Every change lands through a PR from a feature branch; nothing is committed or pushed to `main`. The branch reads `feat/<short-name>`, `fix/<short-name>`, `refactor/…`, `docs/…` or `ci/…`, and is rebased onto `main`, never merged with it: no merge commit in the branch. | Always (base, branch name, commit parents) |
| P2 | A commit subject reads `<type>(<scope>): <imperative summary>` — type one of `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`, `chore`; scope an area (`core`, `metrics`, `sensors`, `settings`, `app`, `docs`, `ci`) or omitted for a workspace-wide change; at most 72 characters, no trailing period. A breaking change carries `!` and a `BREAKING CHANGE:` footer. The PR title is in the same form and summarises the whole PR. | New commit or PR title |
| P3 | **One logical change per commit.** A subject that needs "and" is two commits. No `wip`, `fix build` or `address review` commits: review feedback is folded into the commit it fixes (fixup + autosquash, then `--force-with-lease`). Mechanical changes — renames, moves, formatting, dependency bumps, pure refactors — sit in their own commits ahead of the change that needs them, and a commit over roughly 300 changed lines (lock files excluded) is mechanical and says so in its body. | Any commit list |
| P4 | Every commit builds and passes fmt, clippy and tests on its own, and what belongs together is together: a core type change and all its users, behaviour and its tests, behaviour and its `docs/areas/*.md` update. CI checks only the PR head, so a commit that leaves the workspace broken (a field renamed in `busy-core` in one commit, its users fixed in the next) is the reviewer's to catch. | Multi-commit PR, especially one touching `crates/core` |
| P5 | A commit body says why — motivation, trade-offs, alternatives rejected — and a `perf` change carries measured numbers. | `perf` commit, or a body that only restates the diff |
| P6 | No AI or tool attribution trailer (`Co-Authored-By`, "Generated with …") in a commit or the PR body, and no generated files, screenshots or `target/` in a commit. Screenshots go in the PR description. | Commit message, PR body, a binary or `target/` path in the diff |
| P7 | The PR description covers what and why, how it was tested (commands run, hardware, screenshots for UI changes) and risks/follow-ups, and holds one topic. A verification claim names what was run: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. An unrelated fix found along the way is a separate PR. | PR body; a diff spanning unrelated areas |
| P8 | A change under `app/` or `crates/settings/` states the manual UI check of `docs/testing.md`: taskbar strip and flyout screenshotted, both anchors (`NearTray`, `Left`), both themes, and the settings demo with `BUSY_FORCE_DARK=1` and `0`. The reviewer cannot run the app, so a UI PR with no such evidence is a `question`, not a pass. | Any change under `app/` or `crates/settings/` |
| P9 | **Learned something non-obvious, wrote it down in the same change.** A Win32 quirk, a measured cost or a layout gotcha lands in that area's doc (`docs/areas/*.md`) in the commit that relies on it, and a behaviour change updates the doc passage that described the old behaviour. | New workaround comment, magic constant, retry/throttle interval, changed behaviour of a documented source or window |
| P10 | The PR is merged by **rebase-merge** so its atomic commits land on `main` unchanged: nothing in it relies on a squash to tidy it (fixup commits left in, a history only the squash subject explains). | Commit list shaped for squashing |

Authority: `CLAUDE.md` § Hard rules and § Commands (definition of done) and its closing paragraph
under § Docs; `docs/commits.md` § Conventional Commits, § Atomic commits, § Pull requests;
`docs/testing.md` § Manual UI check; `.github/pull_request_template.md`.

## H — Hard rules

| ID | Assertion | Trigger |
| --- | --- | --- |
| H1 | **No admin requirement, no kernel driver.** Nothing raises `requestedExecutionLevel`, asks for a privilege, installs or talks to a driver of ours, or makes a default-on feature need elevation. Data that needs admin rights (ETW, SRUM), an external service or a third-party tool is opt-in, off by default, listed under Settings → Advanced with its risk shown, and asks for confirmation on enable; when off, its rows are hidden, never faked. | `app.manifest`, `AdjustTokenPrivileges`/`Se*Privilege`, ETW/SRUM, a network call, a new third-party data source |
| H2 | **Vendor DLLs load only from System32**: `LoadLibraryExW(name, LOAD_LIBRARY_SEARCH_SYSTEM32)`, symbols through `GetProcAddress`, absence normal and silent. No `LoadLibraryW`/`LoadLibraryA`, no absolute or relative path, no other search flag. | Any `LoadLibrary*`, a new DLL name, `GetProcAddress` |
| H3 | **Explorer is not ours.** Nothing kills or restarts `explorer.exe`, and nothing sends synthetic input (`SendInput`, `keybd_event`, `mouse_event`, SendKeys) to a window we don't own; code, tests and scripts post messages to our own HWNDs instead. | `SendInput`, `keybd_event`, `mouse_event`, `TerminateProcess`, `explorer`, `Stop-Process`, `taskkill`, `PostMessage`/`SendMessage` to a found HWND |
| H4 | Nothing leaves autostart (`HKCU\...\Run\busy`) enabled after testing: a test, example or script that enables it disables it again. | `autostart::set(true)` or the `Run` key outside `crates/settings/src/autostart.rs` |

Authority: `CLAUDE.md` § Hard rules; `README.md` (no admin rights, no kernel driver);
`docs/areas/sensors.md` § Security rules; `docs/code-quality.md` § Unsafe / FFI;
`docs/plans/design-migration.md` § Decisions and § Opt-in sources; `docs/testing.md` § Manual UI
check, step 4; `docs/areas/settings.md` § Gotchas.

## A — Workspace and core types

| ID | Assertion | Trigger |
| --- | --- | --- |
| A1 | `busy-core` stays pure Rust with no Win32: no `windows` dependency, no FFI. | Any change to `crates/core` or its `Cargo.toml` |
| A2 | Dependency direction is `busy` → {`busy-metrics`, `busy-sensors`, `busy-settings`} → `busy-core`. Collector and UI crates never depend on each other; what they share goes through `busy-core`. | A crate `Cargo.toml` `[dependencies]` change, a `use busy_*` across crates |
| A3 | A core type change is additive where it can be (a new `Option` field). Removing or renaming a field is one commit that builds the whole workspace, and is marked breaking (P2). | Removed/renamed field or variant in `crates/core` |
| A4 | A new `Config` field has its default in `impl Default for Config`, loads from an old file through `#[serde(default)]`, is clamped/deduped in `normalize()` if it has a range, and comes with a test (default, roundtrip, old JSON without the field — Q2). `Config::load()` still never fails: a missing or invalid file gives defaults, then `normalize()`. | New or changed `Config` field, `load`, `normalize` |
| A5 | A new `Module` variant updates `Module::ALL`, `label()` and `Config::default()` as well as every exhaustive `match` (the compiler finds only the last). Processes stays flyout-only. | New `Module` variant |
| A6 | Config persists to `%APPDATA%\busy\config.json` by atomic write (temp file + rename), never an in-place overwrite. | `Config::save`, a new file write |

Authority: `docs/architecture.md` § Workspace and § Persistence; `docs/areas/core.md` § Config and
§ Changing core types.

## T — Threading

| ID | Assertion | Trigger |
| --- | --- | --- |
| T1 | **The UI thread never blocks.** Our taskbar window is a child of explorer's `Shell_TrayWnd`, so its input queue is attached to the taskbar's and a stall freezes the user's taskbar. No sampling (a `Source::sample`, PDH, WMI, NVML/ADL, `NtQuerySystemInformation`), no file I/O and no waits (sleep, `join`, blocking `recv`, `WaitFor*`) on it — in a window procedure, a message handler, a render path, or anything they call. | Code in `app/` or `crates/settings/` that runs on the message loop; a new call from a `WndProc` |
| T2 | Sources are constructed **on** the sampler thread after `CoInitializeEx(COINIT_MULTITHREADED)` and never leave it. `Source` has no `Send` bound on purpose (the GPU and Sensors sources share `Rc<RefCell<_>>`); a diff that adds one, or builds sources elsewhere and moves them, is wrong. | `Source` trait, `sources()`, `sampler.rs`, thread spawn, `CoInitializeEx` |
| T3 | The sampler hands a fresh `Snapshot` per tick to the UI and wakes it with `PostMessage(WM_APP)`; the UI pushes history and redraws on that message. Config changes (interval, active modules) flow back to the sampler; the UI never calls into a source. | Sampler/UI hand-off, `WM_APP`, interval or module toggling |

Authority: `CLAUDE.md` § Hard rules; `docs/architecture.md` § Data flow and § Threads;
`docs/areas/app.md` § Taskbar widget ("Critical"); `docs/areas/core.md` § Source contract;
`docs/code-quality.md` § Errors and robustness.

## U — Unsafe and FFI

| ID | Assertion | Trigger |
| --- | --- | --- |
| U1 | An `unsafe` block wraps only the FFI call, not the logic around it. | New or grown `unsafe` block |
| U2 | A hand-written `#[repr(C)]` struct cites the header or SDK version it copies and carries `const _: () = assert!(size_of::<T>() == N)`, plus offset asserts where the layout matters. | New or changed `#[repr(C)]` |
| U3 | **External memory is untrusted.** Shared memory, WMI results, driver IOCTL output and `NtQuerySystemInformation` buffers are bounds-checked on every offset, size and count before a read — including a `NextEntryOffset` walk and a count read from the buffer itself. | Pointer arithmetic, `from_raw_parts`, `read_unaligned`, a parser over a foreign buffer |
| U4 | Handles are RAII: a `Drop` closes or frees them, and no path leaks a handle per sample. | `CreateFile*`, `OpenProcess`, `OpenFileMapping`, `MapViewOfFile`, PDH/SetupDi/registry handles, `CloseHandle` |

Authority: `docs/code-quality.md` § Unsafe / FFI; `docs/areas/sensors.md` § Security rules.

## E — Errors and panics

| ID | Assertion | Trigger |
| --- | --- | --- |
| E1 | **`Source::sample()` never panics**, nor does any collector code on its path, constructors included. `unwrap`/`expect` are clippy's (see "Already enforced"); the reviewer owns the rest: indexing or slicing a buffer whose length the code did not check, `panic!`/`unreachable!`/`assert!` outside tests, integer division by a value that can be zero, and a `RefCell` borrowed twice across the shared GPU/Sensors state. The release profile is `panic = "abort"`, so one panic takes the whole app down. | Any change in `crates/metrics` or `crates/sensors` |
| E2 | A collector that fails leaves its fields untouched (`None`/empty) and tries again later, with backoff for an expensive retry. It never fills a value it did not read. | Error path in a source, a retry loop |
| E3 | UI failures degrade silently to "—". No message box, except in the settings window for a user-initiated action. | `MessageBoxW`, an error path in `app/` or `crates/settings/` |

Authority: `CLAUDE.md` § Hard rules; `docs/areas/core.md` § Source contract;
`docs/code-quality.md` § Errors and robustness; `Cargo.toml` `[profile.release]`.

## M — Metrics and sources

| ID | Assertion | Trigger |
| --- | --- | --- |
| M1 | A source costs a few ms per call at about 1 Hz; anything slower (WMI, re-enumeration) is throttled or cached inside the source, and the sampler total stays well under 50 ms a tick. A perf-sensitive change is measured with the `dump_*` example before and after, and the numbers are recorded in the area doc (P9). | New API call on a sample path, a changed refresh interval, a `perf` commit |
| M2 | `GetAdaptersAddresses` (6–9 ms) is not reintroduced on the network hot path; discovery stays on `GetIfTable2` (every 60 s or on IPv4 change) with `GetIfEntry2` per sample. | `network.rs` |
| M3 | Snapshot semantics hold: units are percent 0..=100, bytes, bytes/s, °C, RPM, W, MHz; `CpuInfo.total` is Task Manager's `% Processor Utility`, clamped, with the user/kernel split from the *uncapped* ratio; `ProcEntry.io_bps` is all process I/O; `GpuInfo.util_pct` is the max engine; rates come from deltas inside the source and may be zero on the first sample. | A new or changed `Snapshot` field, a unit conversion, `cpu.rs`, `processes.rs` |
| M4 | The network totals count only connected, non-loopback, non-filter interfaces with `HardwareInterface` set, falling back to all connected non-filter non-loopback ones when no hardware interface is up. | Interface filtering in `network.rs` |
| M5 | PDH uses English counter names, and instance churn (hot-plugged disks, cores) is tolerated by re-parsing instance names every sample. Processes deltas stay keyed by (pid, CreateTime). | `pdh.rs`, a counter path, `processes.rs` |

Authority: `docs/areas/core.md` § Source contract and § Snapshot semantics worth knowing;
`docs/areas/metrics.md` § Per source, § Network counting rule (totals), § Measured cost, § Gotchas;
`docs/code-quality.md` § Errors and robustness.

## G — GPU and sensors

| ID | Assertion | Trigger |
| --- | --- | --- |
| G1 | `busy_sensors::sources()` returns `[Gpu, Sensors]` in that order; the two share `Rc<RefCell<Shared>>`, and the Sensors source reads what the GPU source wrote this tick. | `sensors/src/lib.rs`, source order |
| G2 | DXGI enumeration skips software and Microsoft (0x1414) adapters; NVML loads only when an NVIDIA adapter exists; NVML/ADL devices are matched to DXGI adapters by PCI bus/device from D3DKMT. | `gpu.rs`, `nvml.rs`, `adl.rs`, adapter matching |
| G3 | `snap.sensors` precedence is LHM → HWiNFO → NVML/ADL/WDDM, the vendor readings only when neither tool is present; LHM/HWiNFO fill missing GPU temp/hotspot/fan, matched by GPU name. GPU fan % is a `SensorKind::Load` reading named "GPU Fan". | Sensors merge logic |
| G4 | LHM WMI connects lazily, retries every 30 s, and throttles a query over 15 ms to every 3 s; HWiNFO's view is copied whole, then parsed with bounds checks (U3). | `lhm.rs`, `hwinfo.rs` |
| G5 | A backend the area doc marks **untested** (ADL on AMD, LHM running, HWiNFO running, battery) is changed with the PR saying on what hardware it ran, or that it did not; the marker is not dropped without that evidence. | Change to `adl.rs`, `lhm.rs`, `hwinfo.rs`, `battery.rs`, or to an "Untested" line |

Authority: `docs/areas/sensors.md` § Backends, § Security rules, § Gotchas; `docs/areas/core.md`
§ Snapshot semantics worth knowing; `docs/areas/metrics.md` § Per source (battery);
`docs/commits.md` § Pull requests (hardware in the description).

## W — Taskbar widget, flyout, rendering

| ID | Assertion | Trigger |
| --- | --- | --- |
| W1 | The widget is a `WS_CHILD` of `Shell_TrayWnd` on the primary monitor, parented with `SetParent`. It re-finds and re-parents on `RegisterWindowMessageW("TaskbarCreated")`, and destroys its child window on exit so explorer is not left with a dead child. | `taskbar.rs`, window creation, exit path |
| W2 | Rendering is a layered child window: D2D `ID2D1DCRenderTarget` (premultiplied BGRA) into a 32-bit DIB, then `UpdateLayeredWindow`. The background alpha is 1, not 0, so clicks still hit us. | `render.rs`, `taskbar.rs` painting |
| W3 | Position is re-checked each render tick and the window moved only on change; `NearTray` sits left of `TrayNotifyWnd`, `Left` at the taskbar's left edge, both plus a DPI-scaled `offset_px`. | Positioning code, `Anchor` |
| W4 | The flyout is a top-level `WS_POPUP` with `WS_EX_TOOLWINDOW \| WS_EX_TOPMOST`, clamped to the work area, dismissed on deactivate, Esc or re-click (minding the deactivate → click toggle race); its sections follow `Config.modules` order where `flyout = true`. | `flyout.rs` |
| W5 | The taskbar theme follows `SystemUsesLightTheme`, not `AppsUseLightTheme`, unless `Config.theme` overrides it, and reacts to `WM_SETTINGCHANGE` "ImmersiveColorSet". | `theme.rs`, theme reads |
| W6 | `app/app.manifest` keeps PerMonitorV2, comctl32 v6 and the Win8/10 `supportedOS` entries, embedded by `app/build.rs`; layered child windows need them. | `app.manifest`, `app/build.rs` |
| W7 | UI migration work follows `design/` and the plan's decisions: the app name stays busy; the flyout is the Detailed (1a) layout at 360 px, Compact/Tiles out of scope; theme tokens and palette mirror the design 1:1; a missing data row is omitted, not faked; each phase lands as its own PR(s) of atomic commits. | Changes to `theme.rs`, `taskbar.rs`, `flyout.rs` or settings that cite the design |

Authority: `docs/areas/app.md` intro, § Taskbar widget, § Flyout, § Theme;
`docs/plans/design-migration.md` § Decisions, § Phases, § Out of scope.

## S — Settings and autostart

| ID | Assertion | Trigger |
| --- | --- | --- |
| S1 | The public API stays `open`, `is_open`, `is_dialog_message`, `autostart::{is_enabled, set}`; the app's message loop calls `is_dialog_message` before `TranslateMessage`/`DispatchMessage`. The planned custom-D2D rewrite keeps that API unchanged and adds no dependency. | `crates/settings/src/lib.rs`, the app's message loop |
| S2 | `on_apply` fires only when the config actually changed, and the **caller** persists it (`Config::save`); the settings crate does not write the config file. | `on_apply`, dirty tracking, `Config::save` |
| S3 | Autostart lives in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\busy` plus the `StartupApproved\Run` flag and is owned by `busy-settings`: `is_enabled` honours the Task Manager "disabled" flag, `set(true)` clears it, `set(false)` removes both values. On registry failure the window shows a message box and reflects the real registry state in `cfg.autostart`. | `autostart.rs`, the Start with Windows control |
| S4 | The window is an unowned top-level with its own taskbar button (the app's HWND is explorer's child and cannot own it). Every label has an Alt mnemonic, Enter is OK and Esc Cancel, and it is DPI-aware through `SystemParametersInfoForDpi` + `WM_DPICHANGED`. Theme resolution is `BUSY_FORCE_DARK` → `Config.theme` → `AppsUseLightTheme`. | `window.rs`, `dark.rs` |

Authority: `docs/areas/settings.md` § API, § Layout, § Dark mode; `docs/architecture.md`
§ Persistence; `docs/plans/design-migration.md` § Decisions and § 4. Settings window.

## Q — Tests

| ID | Assertion | Trigger |
| --- | --- | --- |
| Q1 | New parsing of external data (struct layouts, instance names, shared memory) gets a unit test fed synthetic bytes, including truncated and oversized input. | New parser, `#[repr(C)]` read, instance-name parsing |
| Q2 | A new `Config` field gets tests for its default, a roundtrip, and an old JSON file without the field. | New `Config` field |
| Q3 | A new formatter or unit conversion gets a table-driven unit test. | `fmt.rs`, a unit conversion |
| Q4 | A new source or backend extends the smoke test, is compared against a reference tool via its `dump_*` example, and has its cost recorded in the area doc. Smoke tests range-check hardware-dependent fields (GPU, battery, sensors) only when present, so they pass on a CI runner without them. | New `Source`, new backend, `tests/smoke.rs` |
| Q5 | A bug fix carries a regression test at the lowest layer that can reproduce it. | `fix` commit |
| Q6 | Unit tests live in a `#[cfg(test)] mod tests` next to the code they test. | New test file or module |

Authority: `docs/testing.md` § Layers and § What to test when; `docs/areas/core.md` § Config.

## D — Dependencies and build

| ID | Assertion | Trigger |
| --- | --- | --- |
| D1 | **No crate beyond `windows`, `serde`, `serde_json`** — in `[dependencies]`, `[dev-dependencies]` or `[build-dependencies]`, direct or via `[workspace.dependencies]` — without explicit maintainer approval and a reason in the PR. A new workspace crate of our own is not a new dependency. | Any `Cargo.toml` dependency change, a new `Cargo.lock` package |
| D2 | `windows` features are added to the crate that needs them, not to `[workspace.dependencies]`. | `features = [...]` change |
| D3 | A dependency bump or toolchain change is its own commit, typed `build`, ahead of the change that needs it. | `Cargo.toml` version change, `rust-toolchain.toml` |
| D4 | The release version lives in root `Cargo.toml` `[workspace.package]`; a release is a `vX.Y.Z` tag on `main`. | `version` change |

Authority: `CLAUDE.md` § Hard rules; `docs/code-quality.md` § Dependencies and § Enforced;
`docs/commits.md` § Conventional Commits, § Atomic commits, § Branches and releases.

## L — Lints and suppressions

| ID | Assertion | Trigger |
| --- | --- | --- |
| L1 | A local `#[allow(...)]` carries a one-line reason next to it. Examples opt out of `unwrap_used`/`expect_used` with a crate-level `#![allow]`; tests are allowed through `clippy.toml`, and nothing else widens it. | New `#[allow]`/`#![allow]`, `clippy.toml` change |
| L2 | Lints are configured once, in root `Cargo.toml` `[workspace.lints]`, and every crate, new ones included, opts in with `[lints] workspace = true`. A diff never lowers a workspace lint or drops a crate's opt-in. | New crate, `[workspace.lints]`, a crate's `[lints]` |

Authority: `docs/code-quality.md` § Enforced.

## C — Code style

| ID | Assertion | Trigger |
| --- | --- | --- |
| C1 | Comments explain *why* or document a Win32 quirk (struct layout source, undocumented behaviour, measured cost); they don't narrate what the code does. | New comment |
| C2 | One concern per file, and each `Source` in its own file. | New source, a file gaining a second subject |
| C3 | Win32 types keep their Win32 names; our own types use Rust naming. | New type or binding |
| C4 | Idiomatic, compact Rust matching the surrounding code: `let … else`, `?` on `Option`/`Result`, iterator chains over index loops where it reads better. Reported as `nit` at most. | New code |

Authority: `docs/code-quality.md` § Style.

## Already enforced — not the reviewer's business

A tool or CI (`.github/workflows/ci.yml`, on `windows-latest`, for every PR and every push to
`main`) decides these exactly. Report them only if the diff disables or weakens the check itself
(L1, L2).

- Formatting — `cargo fmt --all --check` against `rustfmt.toml` (`max_width = 120`).
- Every clippy lint and every compiler warning —
  `cargo clippy --workspace --all-targets --locked -- -D warnings`. That covers `unwrap_used`/`expect_used` outside tests, `dbg_macro`, `todo`,
  `unimplemented`, `semicolon_if_nothing_returned`, `redundant_closure_for_method_calls`,
  `unused_qualifications`, and `unsafe_op_in_unsafe_fn` (deny). Do not hunt for these in a diff.
  The reviewer still owns whether a *suppression* earns its place (L1) and every panic clippy does
  not see (E1).
- Whether it compiles and whether tests pass — `cargo test --workspace --locked`, and the release
  build `cargo build --workspace --release --locked`. The smoke tests run on a runner without a GPU
  or battery, so hardware-dependent paths are not exercised there (G5, Q4).
- `Cargo.lock` in step with the manifests — `--locked`. A *new package* in it is still D1.
- The toolchain version — `rust-toolchain.toml`, which CI installs.
- A non-exhaustive `match` after a new `Module` variant — the compiler. `Module::ALL`, `label()` and
  `Config::default()` are still A5.

CI checks the PR head only, not each commit, so "every commit builds" (P4) is not among these.
