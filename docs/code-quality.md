# Code quality

## Enforced

- `cargo fmt` — `rustfmt.toml` (`max_width = 120`, `use_small_heuristics = "Max"`). CI runs `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets -- -D warnings` — lints configured once in root `Cargo.toml` `[workspace.lints]`; every crate opts in with `[lints] workspace = true`.
  - Beyond `clippy::all`: `unwrap_used`, `expect_used` (allowed in tests via `clippy.toml`; examples opt out with a crate-level `#![allow]`), `dbg_macro`, `todo`, `unimplemented`, `semicolon_if_nothing_returned`, `redundant_closure_for_method_calls`; rust `unsafe_op_in_unsafe_fn = deny`, `unused_qualifications`.
  - Local `#[allow(...)]` only with a one-line reason next to it.
- Toolchain pinned in `rust-toolchain.toml`.

## Dependencies

The whole point of this app is that users can trust it. Allowed: `windows`, `serde`, `serde_json`, and `windows-core` — `windows`' own core, already in every build — as a direct dependency only where COM's `#[implement]` needs it (its expansion names `::windows_core`). Anything else needs explicit maintainer approval and a reason in the PR. Add `windows` features to the crate that needs them, not to the workspace.

## Style

- Idiomatic, compact Rust. Match the surrounding code.
- Comments explain *why* or document a Win32 quirk (struct layout source, undocumented behaviour, measured cost). Don't narrate what the code does.
- One concern per file; each `Source` in its own file.
- Prefer `let … else`, `?` on `Option`/`Result`, iterator chains over index loops where it reads better.
- Names: Win32 types keep their Win32 names; our own types use Rust naming.

## Unsafe / FFI

- Keep `unsafe` blocks tight — wrap only the FFI call, not surrounding logic.
- Hand-written `#[repr(C)]` structs: cite the header/SDK version and add `const _: () = assert!(size_of::<T>() == N)` (and offsets where it matters).
- Treat all external memory (shared memory, WMI, driver IOCTL output, NtQuerySystemInformation buffers) as untrusted: bounds-check every offset/size/count.
- RAII for handles (`Drop` closes/frees); never leak a handle per sample.
- DLLs load only via `LOAD_LIBRARY_SEARCH_SYSTEM32`.

## Errors and robustness

- Collectors: no panics, no `unwrap`. Missing data → `None`/empty and try again later (with backoff for expensive retries).
- UI: never block the UI thread (see docs/architecture.md). Failures degrade silently to "—", never a message box, except in the settings window for user-initiated actions.
- Performance budget: sampler total well under 50 ms/tick; UI render a few ms. Measure with the dump examples before/after perf-sensitive changes and record numbers in the area doc.
