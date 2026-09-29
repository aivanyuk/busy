# busy-ui

`crates/ui` — what busy's windows show and how, shared by the app (taskbar widget, flyout) and the settings window, so both draw from one source. Pure logic, unit-tested, no Win32 so far: `fmt.rs` (human-readable values), `select.rs` (what to show: the taskbar sensor, the busiest GPU, the disk volume and network interface a module reads, CPU core bars), `history.rs` (rolling series per module, sized by `history_secs` at the module's interval).

The app's area doc (`docs/areas/app.md`) describes how these choices appear on the taskbar and in the flyout.
