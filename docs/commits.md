# Commits and branches

## Conventional Commits

```
<type>(<scope>): <imperative summary, ≤ 72 chars, no trailing period>

<body: why, not what; wrap at 72>

<footer: BREAKING CHANGE: …, Refs #123>
```

**Types:** `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build` (Cargo, manifest, toolchain), `ci`, `chore`.

**Scopes** = area: `core`, `metrics`, `sensors`, `settings`, `app`, `docs`, `ci`. Omit the scope for workspace-wide changes.

Examples:

```
feat(sensors): read GPU hotspot temperature via ADL Overdrive8
fix(app): re-parent widget after explorer restart
perf(metrics): poll GetIfEntry2 instead of GetIfTable2 each sample
refactor(core)!: split ProcEntry.io_bps into disk_bps and other_bps

BREAKING CHANGE: ProcEntry.io_bps removed.
```

## Atomic commits

A PR is reviewed commit by commit. Each commit must be a self-contained step a reviewer can read, understand and verify on its own.

- **One logical change per commit.** A commit does one thing that its subject line fully describes. If the subject needs "and", split it.
- **Every commit builds and passes** `fmt`, `clippy -D warnings` and `test`. No "fix build" / "wip" / "address review" commits — fold them into the commit they fix (`git commit --fixup` + `git rebase -i --autosquash`) before requesting review.
- **Separate mechanical from behavioural changes.** Renames, moves, formatting, dependency bumps and pure refactors go in their own commits, before the change that needs them. A reviewer should never hunt for a behaviour change inside a 500-line rename.
- **Order tells the story:** preparatory refactor → new type/API → implementation → wiring → tests/docs if not already in the same commit.
- **Keep what belongs together together:** a core type change and all its users; behaviour and its tests; behaviour and its `docs/areas/*.md` update.
- **Body explains why**: motivation, trade-offs, alternatives rejected, measured numbers for perf changes. The diff already shows what.
- **Size:** aim for commits reviewable in a few minutes (roughly < 300 changed lines, excluding generated/lock files). Larger is fine only for mechanical changes, stated as such in the body.
- No AI/tool attribution trailers (`Co-Authored-By`, "Generated with …").
- No generated files, screenshots, or `target/` in commits.

## Pull requests

**Every change lands through a PR. Never commit or push directly to `main`.**

1. Branch from up-to-date `main`: `feat/<short-name>`, `fix/<short-name>`, `refactor/…`, `docs/…`, `ci/…`.
2. Commit atomically (above); rebase onto `main` instead of merging `main` into the branch.
3. Open the PR:
   - **Title** in Conventional Commits form (it summarizes the whole PR).
   - **Description**: what and why, how it was tested (commands run, hardware, screenshots for UI changes), risks/follow-ups, and a short commit-by-commit guide if the order matters.
   - One topic per PR. Unrelated fixes found along the way go in a separate PR.
4. CI must be green; review feedback is addressed by amending the relevant commit (fixup + autosquash), then force-push with `--force-with-lease`.
5. Merge with **rebase-merge** so the atomic commits land on `main` unchanged. No squash (it destroys the commit structure reviewers read), no merge commits.

## Branches and releases

- `main` is always releasable and protected: PR required, CI green, linear history, no force pushes.
- Delete the branch after merge.
- Releases: tag `vX.Y.Z` on `main`; version in root `Cargo.toml` `[workspace.package]`.
