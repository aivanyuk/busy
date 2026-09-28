# busy-settings

`crates/settings` — modeless native settings window + autostart. Files: `lib.rs` (API), `window.rs` (layout, events, dark painting), `autostart.rs`, `dark.rs`.

Standalone check: `cargo run -p busy-settings --example demo` (set `BUSY_FORCE_DARK=1` / `0` to force a theme).

## API (called from the app's UI thread)

```rust
pub fn open(_owner: HWND, cfg: &Config, on_apply: Box<dyn Fn(Config)>); // opens or focuses
pub fn is_open() -> bool;
pub fn is_dialog_message(msg: &MSG) -> bool; // call before TranslateMessage/DispatchMessage
pub mod autostart { pub fn is_enabled() -> bool; pub fn set(enabled: bool) -> windows::core::Result<()>; }
```

- Window is unowned top-level with its own taskbar button (the app's HWND is a child of explorer's taskbar — can't own).
- `on_apply` fires only when the config actually changed; the **caller** persists it (`Config::save`).
- The window applies autostart itself; on registry failure shows a message box and reflects the real registry state in `cfg.autostart`.
- `autostart::is_enabled` also checks the Task Manager "disabled" flag (`StartupApproved\Run`); `set(true)` clears it, `set(false)` removes both values.

## Layout

Groups: Modules (ListView: Module | Taskbar | Flyout | Style, Move up/down, row editors below), Taskbar (Position, Offset with up-down), General (interval, history, theme, temp unit, pinned sensor, Start with Windows). OK / Cancel / Apply (Apply enabled only when dirty). Every label has an Alt mnemonic; Enter = OK, Esc = Cancel. DPI-aware via `SystemParametersInfoForDpi` + `WM_DPICHANGED`.

## Dark mode

Theme resolution: `BUSY_FORCE_DARK` env → `Config.theme` → `AppsUseLightTheme`. Title bar via DWM; controls via `SetWindowTheme` (`DarkMode_Explorer`, `DarkMode_CFD`, `DarkMode_ItemsView`) + `WM_CTLCOLOR*` brushes. Checkboxes, group-box frames and ListView header text are owner-drawn in dark mode (themed versions ignore text color).

## Gotchas

- Linker warning 81010002 on `<dpiAwareness>` is benign; silenced with `#![allow(linker_messages)]`.
- Window icon loads from resource ID 1 of the exe (the app must embed its icon as ID 1).
- Testing: post messages to the demo's own HWNDs; never SendKeys (focus may be elsewhere). Remove autostart after testing.
- Untested: dragging across monitors with different DPI.
