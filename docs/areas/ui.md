# busy-ui

`crates/ui` — what busy's windows show and how, shared by the app (taskbar widget, flyout) and the settings window, so both draw from one source.

- Pure logic, unit-tested: `fmt.rs` (human-readable values), `select.rs` (what to show: the taskbar sensor, the busiest GPU, the disk volume and network interface a module reads, CPU core bars), `history.rs` (rolling series per module, sized by `history_secs` at the module's interval), `tone.rs` (which design color: module palette, load colors).
- Drawing: `theme.rs` (the design's tokens and palettes, light/dark resolution), `render.rs` (`Gfx`: D2D factory, DWrite formats with tabular figures and tracking; `Canvas`: fills, outlines, text, bars, sparklines), `ctx.rs` (`Ctx`, the read-only view a window draws from: config, snapshot, history, theme, `Gfx`).

Everything here runs on the UI thread and never blocks, except `Theme::resolve`, which reads the registry for `ThemeMode::System` (call it on a worker, or at startup before a window exists).

The app's area doc (`docs/areas/app.md`) describes how these choices appear on the taskbar and in the flyout.
