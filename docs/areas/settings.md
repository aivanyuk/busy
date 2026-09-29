# busy-settings

`crates/settings` — modeless native settings window + autostart. Files: `lib.rs` (API), `autostart.rs`, `dark.rs` (theme resolution, dark palette), and `window/`, one concern per file: `mod.rs` (`Ui` state, create/open), `controls.rs` (IDs, creation, control helpers), `layout.rs` (font, positions, DPI), `paint.rs` (theme application, dark owner-draw), `commands.rs` (`WM_COMMAND`/`WM_NOTIFY`, module list editors), `config.rs` (fill from / collect to `Config`, Apply), `wndproc.rs` (window procedure), `worker.rs` (registry thread: autostart read/write, app theme).

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
- The window applies autostart itself, on its registry worker (`window/worker.rs`): Apply/OK with a changed Start with Windows disables OK/Apply until the write is read back, then finishes (OK closes). On registry failure it shows a message box and reflects the real registry state in `cfg.autostart`. Cancel/Esc during the write closes once it lands.
- `autostart::{is_enabled, set}` block on the registry: call them off a UI thread (the window calls them only from its worker).
- `autostart::is_enabled` also checks the Task Manager "disabled" flag (`StartupApproved\Run`); `set(true)` clears it, `set(false)` removes both values.

## Layout

Groups: Modules (ListView: Module | Taskbar | Flyout | Style, Move up/down, row editors below; the Style combo offers only the selected module's `allowed_styles()`), Taskbar (Position, Offset with up-down), General (interval, history, theme, temp unit, pinned sensor, Start with Windows). OK / Cancel / Apply (Apply enabled only when dirty). Every label has an Alt mnemonic; Enter = OK, Esc = Cancel. DPI-aware via `SystemParametersInfoForDpi` + `WM_DPICHANGED`.

## Dark mode

Theme resolution: `BUSY_FORCE_DARK` env → `Config.theme` → `AppsUseLightTheme`, the last read on the worker at open and on each `ImmersiveColorSet` broadcast. The window stays hidden until that first read, so it never flashes the wrong theme; the Start with Windows box stays disabled until the registry state is known. Title bar via DWM; controls via `SetWindowTheme` (`DarkMode_Explorer`, `DarkMode_CFD`, `DarkMode_ItemsView`) + `WM_CTLCOLOR*` brushes. Checkboxes, group-box frames and ListView header text are owner-drawn in dark mode (themed versions ignore text color).

## Gotchas

- Linker warning 81010002 on `<dpiAwareness>` is benign; silenced with `#![allow(linker_messages)]`.
- Window icon loads from resource ID 1 of the exe (the app must embed its icon as ID 1).
- Testing: post messages to the demo's own HWNDs; never SendKeys (focus may be elsewhere). Remove autostart after testing.
- Untested: dragging across monitors with different DPI; the autostart-failure message box since the move to the worker (no safe way to make the `Run` key write fail).
