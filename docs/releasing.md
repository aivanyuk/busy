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
3. The release workflow (`.github/workflows/release.yml`) checks that the tag matches the version and runs the full CI on it. It then builds `busy.exe` and attests its provenance, and makes a **draft** Release with the zip, the bare exe, `SHA256SUMS` and the version's changelog section as notes.
4. Check the draft:
   - The assets are there and the notes read right.
   - The exe from the zip starts, shows X.Y.Z in Settings and in its file properties, and `gh attestation verify busy.exe --repo aivanyuk/busy` passes.
5. Publish it.

## If something is wrong

- Before publishing: delete the draft and the tag (`git push origin :refs/tags/vX.Y.Z`), fix on `main`, tag again.
- After publishing: don't move a published tag. Fix on `main` and release X.Y.Z+1.
