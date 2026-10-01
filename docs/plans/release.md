# Plan: versioning and releases

Goal: anyone can download a working `busy.exe` from GitHub Releases, check what they downloaded, and see which version they run. The release pipeline lands now; **v0.1.0 is published after Phase 5 (onboarding)** of the design migration, so first-time users get a first run.

## Decisions

- **SemVer**, `0.x` until the design migration and onboarding are done. The workspace `version` in the root `Cargo.toml` is the only place the number is written; everything else (manifest, `VERSIONINFO`, the About row, the release) is derived from it. Tags are `vX.Y.Z` on `main`.
- **x64 only** (`x86_64-pc-windows-msvc`). Windows on ARM runs it emulated. CI also builds and tests `aarch64-pc-windows-msvc` natively (`windows-11-arm`) and uploads that exe as a `busy-arm64-<sha>` artifact; the code needs no changes for it (no arch-specific code, the hand-written NT structs are LLP64 on both, NVML/ADL are absent on ARM and load as `None`). Releases add an ARM64 zip once its UI has been checked on ARM hardware (`tools/vm/guest.ps1` as a probe, docs/testing.md).
- **Unsigned** for now. The README says what SmartScreen shows for a new unsigned download ("Windows protected your PC" → More info → Run anyway). Downloads can be verified with `SHA256SUMS` and GitHub's build provenance (`gh attestation verify busy.exe --repo aivanyuk/busy`). Signing (Azure Trusted Signing or SignPath) can be added to the release job later without changing anything else.
- **Portable, no installer**, as the README promises: a zip and the bare exe. Suggested place: `%LOCALAPPDATA%\Programs\busy\busy.exe`. winget (portable manifest) comes once a release is stable.
- **No auto-update.** Settings shows the version and a link to the releases page. An **opt-in update check** (Advanced, off by default, with its risk stated) may ask GitHub for the latest release; it never downloads or installs anything.
- No new crates: resources are written by `build.rs` itself, HTTP goes through WinHTTP (`windows` crate). CI tools (like `cargo-audit` today) aren't crate dependencies.

## Release artifacts

For tag `vX.Y.Z`, a **draft** GitHub Release named `busy X.Y.Z`, published by hand after a look:

| Asset | Contents |
|---|---|
| `busy-X.Y.Z-x64.zip` | `busy.exe`, `LICENSE`, `README.md`, `THIRD-PARTY-LICENSES.txt` |
| `busy.exe` | the same exe, for a direct download |
| `SHA256SUMS` | SHA-256 of the two above |

Plus a build provenance attestation for both (`actions/attest-build-provenance`). Release notes are the version's section of `CHANGELOG.md`.

## Work, as PRs

### R1. Version and resources in the exe (`build`)
- `build.rs` writes a `.res` file itself (RES is a simple binary format; no `rc.exe`, no `embed-resource`/`winres`) and passes it to the linker:
  - `VERSIONINFO`: FileVersion/ProductVersion from `CARGO_PKG_VERSION`, ProductName "busy", FileDescription, LegalCopyright from `LICENSE`, OriginalFilename.
  - Icon resource ID 1 (the settings window already loads it): the app glyph (three bars), in 16, 20, 24, 32, 40, 48, 64 and 256 px. It is drawn by `build.rs` from the glyph's geometry, so there is no binary asset to keep in sync.
- `app.manifest`'s `assemblyIdentity version` is filled in from `CARGO_PKG_VERSION` (the manifest becomes a template).
- Check: Explorer → Properties → Details shows the version; the icon shows in Explorer, the taskbar button of Settings and Task Manager.

### R2. Safe to upgrade and downgrade (`core`, `settings`, `app`)
- A config written by a newer busy (`version` > `CONFIG_VERSION`) is read as well as it can be but never saved over, so going back a version doesn't drop settings. The Settings window says so ("Settings were saved by a newer busy; changes aren't saved").
- Autostart: at startup, if the `Run` value exists but names another path (the exe was moved or replaced elsewhere), rewrite it to the running exe, on the settings worker thread (T5).
- README: download, verify, where to put the exe, SmartScreen, how to uninstall (turn off Start with Windows, exit, delete the exe and `%APPDATA%\busy`).

### R3. Version in Settings, changelog, licenses (`settings`, `docs`, `ci`)
- Settings → General → About: "busy X.Y.Z" and a "Releases" link that opens `https://github.com/aivanyuk/busy/releases` in the browser (`ShellExecuteW`, only on click).
- `CHANGELOG.md` (Keep a Changelog): every user-visible PR adds a line under "Unreleased"; the release commit moves them under the new version. Backfilled from the design-migration PRs.
- `THIRD-PARTY-LICENSES.txt`: the license texts of every crate in the release build (the `windows` family, `serde`, `serde_json` and theirs, about 20, all MIT or Apache-2.0), kept by hand. CI fails when `Cargo.lock` gains a package the file doesn't name.
- `docs/releasing.md`: the release checklist below.

### R4. Release workflow (`ci`)
- `.github/workflows/release.yml`, on a pushed `v*` tag and by hand (`workflow_dispatch`, a dry run that builds and uploads the assets as a workflow artifact, no Release):
  1. Fail unless the tag equals `v` + the workspace version and the tagged commit is on `main`.
  2. The CI checks: `cargo audit`, fmt, clippy `-D warnings`, tests, all `--locked` on the pinned toolchain.
  3. `cargo build -p busy --release --locked` with `--remap-path-prefix`, so no build paths end up in the exe.
  4. Zip, `SHA256SUMS`, provenance attestation.
  5. Draft Release with the assets and the `CHANGELOG.md` section as notes (`gh release create --draft`).
- Permissions: `contents: read` by default; the release job alone gets `contents: write`, `id-token: write`, `attestations: write`. Actions pinned by commit SHA, as in `ci.yml`.
- Dry run on this PR's branch through `workflow_dispatch`; a real draft from a throwaway `v0.0.0-test` tag, deleted afterwards along with its draft.

### R5. Opt-in update check (`core`, `app`, `settings`)
- `OptIn::update_check` (off by default), listed under Settings → Advanced with the risk shown verbatim and the confirmation on enable:
  > Contacts api.github.com once a day while busy runs, to see whether a newer release exists; GitHub sees your IP address. Nothing is downloaded or installed.
- While on: one HTTPS GET of `https://api.github.com/repos/aivanyuk/busy/releases/latest` (WinHTTP, a `User-Agent` of `busy/X.Y.Z`, 10 s timeouts), at startup and then every 24 h, on its own thread, never the UI thread. Only `tag_name` and `html_url` are read (`serde_json`); drafts and pre-releases are never offered. Errors are silent and retried next time.
- A newer version shows in the About row ("busy 0.1.0 — 0.2.0 is available", the link going to that release). Nothing else changes; no notification, no badge.
- Turning it off stops the thread's timer; no request is ever made while it is off. `docs/plans/design-migration.md` § Opt-in sources gains its row.

## Releasing (to go into `docs/releasing.md` with R3)

1. On a branch: bump the workspace `version`, move "Unreleased" in `CHANGELOG.md` under `## [X.Y.Z] - YYYY-MM-DD`, open a PR, merge it.
2. Tag the merge commit: `git tag -a vX.Y.Z -m "busy X.Y.Z"` and push the tag.
3. When the workflow is done, check the draft: assets present, notes right, the zip's exe starts and shows X.Y.Z in Settings → About.
4. Publish.

## Order

R1, R2, R3 and R4 can go in any order and before Phase 5; R5 needs R3's About row. Then Phase 5, then v0.1.0.
