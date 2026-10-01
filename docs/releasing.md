# Releasing

How a version of busy gets from `main` to a GitHub Release. The plan and the reasons are in `docs/plans/release.md`.

## Along the way (every PR)

- A change a user sees or gets (a feature, a fix, a changed default, a new opt-in) adds a line under `## [Unreleased]` in `CHANGELOG.md`, in the section that fits: Added, Changed, Fixed, Removed, Security. Internal work (refactors, tests, CI, docs for contributors) doesn't.
- A dependency change regenerates `THIRD-PARTY-LICENSES.txt` (`python tools/third-party-licenses.py`); CI's `tools/check-licenses.sh` fails until it names every crate linked into busy.exe.

## Cutting a release

1. On a branch (`release/vX.Y.Z`):
   - Set the workspace `version` in the root `Cargo.toml` (SemVer; `0.x` until the design migration and onboarding are done) and run `cargo check` so `Cargo.lock` follows.
   - In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` and start a new empty `## [Unreleased]` above it.
   - Open a PR (`chore: release vX.Y.Z`) and merge it.
2. Tag the merge commit on `main` and push the tag:
   ```
   git tag -a vX.Y.Z -m "busy X.Y.Z"
   git push origin vX.Y.Z
   ```
3. The release workflow (`.github/workflows/release.yml`) runs in two jobs:
   - `build`, with a read-only token: checks that the tag matches the version and is on `main`, runs the CI checks, builds `busy.exe` with the build machine's paths mapped out, packages the zip and `SHA256SUMS`, packs the same exe as the Microsoft Store package (`tools/pack-msix.ps1`, the run's `store-X.Y.Z` artifact, not a Release asset), and takes the version's changelog section as notes.
   - `publish`, for a tag only: attests the provenance of the exe and the zip, and makes a **draft** Release with them.
4. Check the draft:
   - The assets are there and the notes read right.
   - The exe from the zip starts, shows X.Y.Z in Settings and in its file properties, and `gh attestation verify busy.exe --repo aivanyuk/busy` passes.
5. Publish it.
6. The Store (docs/plans/store.md): download the run's `store-X.Y.Z` artifact, and in Partner Center → busy — system monitor → start an update submission → Packages, replace the package with its `busy-X.Y.Z-x64.msix`, then Submit. Partner Center signs and certifies it (often within a day, up to a few); the Store then updates installed copies itself. Each submission needs a higher version than the last, as tags have.

## Dry run

Running the workflow by hand (Actions → Release → Run workflow), or a pull request that changes it, runs `build` only; its assets are the run's `release-X.Y.Z` and `store-X.Y.Z` artifacts.

## If something is wrong

- Before publishing: delete the draft and the tag (`git push origin :refs/tags/vX.Y.Z`), fix on `main`, tag again.
- After publishing: don't move a published tag. Fix on `main` and release X.Y.Z+1.
