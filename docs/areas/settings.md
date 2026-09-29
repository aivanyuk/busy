# busy-settings

`crates/settings` — the settings window, custom-drawn with Direct2D to the design (`design/Meterbar.dc.html`, Settings), and autostart. Files: `lib.rs` (API), `autostart.rs`, `dark.rs` (theme resolution, DWM title-bar mode), and `window/`, one concern per file:

- `mod.rs` — `Ui` state, create/open, applying edits, autostart through the worker, rendering to an `ID2D1HwndRenderTarget`.
- `model.rs` — what each page shows (design `rows()`): headers, one card per setting with its control, the taskbar order; `choices.rs` — each dropdown's options and the value each sets; `edit.rs` — applying an `Edit` to the config. All pure and tested.
- `layout.rs` — the design's measures, the `View` (page, items, their places, scroll, hover, open popup) and hit-testing; `paint.rs` — drawing the whole window from the `View`.
- `controls/` — one file per control kind (`toggle`, `dropdown` with its popup, `segmented`, `swatch`, `order` for the taskbar order list, `nav` for the nav column); each sizes and draws itself and names the part under the pointer.
- `input.rs` — pointer input: hover, clicks, wheel; a click on a control becomes a `model::Edit`.
- `frame.rs` — the custom title bar and the non-client handling behind it; `wndproc.rs` — the window procedure; `worker.rs` — registry thread (autostart read/write, app theme); `dump.rs` — debug-only frame dump.

Standalone check: `cargo run -p busy-settings --example demo` (set `BUSY_FORCE_DARK=1` / `0` to force a theme).

## API (called from the app's UI thread)

```rust
pub fn open(_owner: HWND, cfg: &Config, on_apply: Box<dyn Fn(Config)>, page: Option<Module>); // opens or focuses; `page` shows that module's page
pub fn is_open() -> bool;
pub fn is_dialog_message(msg: &MSG) -> bool; // always false now: the window handles its keys itself
pub mod autostart { pub fn is_enabled() -> bool; pub fn set(enabled: bool) -> windows::core::Result<()>; }
```

- Window is unowned top-level with its own taskbar button (the app's HWND is a child of explorer's taskbar — can't own).
- Changes apply live, as in the design (no OK/Apply): every edit is normalized and, if the config changed, handed to `on_apply`; the **caller** persists it (`Config::save`, on the app's config writer).
- `page` is how a flyout's "<Module> settings" button lands on its module, also in a window already open.
- Autostart is written by the window's registry worker (`window/worker.rs`): the toggle is drawn disabled until the first read, and again while a write is in flight; the value read back after the write goes into the config. On failure a message box says so. Closing during a write closes once it lands.
- `autostart::{is_enabled, set}` block on the registry: call them off a UI thread (the window calls them only from its worker).
- `autostart::is_enabled` also checks the Task Manager "disabled" flag (`StartupApproved\Run`); `set(true)` clears it, `set(false)` removes both values.

## Layout (design measures, in DIPs)

- Window 1000 × 700 (min 760 × 520), solid `--win` background (the design has no Mica), centered on the primary work area. Title bar 40 high: app glyph, "busy Settings", 46 × 40 caption buttons (Close hover `#C42B1C`; Minimize and Maximize work, unlike the prototype's).
- Nav 272 wide: app header (48 tile, "busy", "N readings on the taskbar" with a singular for 1), the search box, then General and one item per module in config order (color dot, On/Off; Processes is On when its top-process lists are). The selected item has `--hover` and a 3 × 18 accent pill.
- Content: padding 12 32 32 20, children 4 apart; page title 28/600, subtitle 13 `--fg2`; section headers 14/600 with 14 above; one card per setting (min 68 high, padding 12 16 12 20, title 14, description 12 `--fg2` wrapping, control right-aligned, or under the text when that would be narrower than 220). Wheel scrolls 48 per notch; a thin thumb shows when the page is taller than the pane.
- Pages: General (Behavior: Start with Windows, Widget position, Offset, Default update interval, History; Appearance: Theme; Taskbar order with ↑/↓, Processes left out as it has no cell) and one per module (Taskbar: Show on taskbar, Style — segments of `Module::allowed_styles()`, Io named "Read / write" for Disk and "Up / down" for Network —, Show label except for Io, module options, and for Sensors the °C/°F Unit; Color: Widget color swatches from the module palette, Color by load; Updates: Update interval with "Default (…)"). Processes' page has Top processes and its interval. Offset and History are not in the design; they keep settings the native window had.
- Dropdowns list the design's options; a current value that isn't one of them (a hand-edited file, a drive that is gone) is kept as an extra option, so opening the window never changes a setting.
- The popup opens 4 below its button (above it without room), at least as wide as the button, and scrolls by wheel when its options don't fit. A click outside it only closes it; Esc closes it, and without a popup Esc closes the window.

## Theme

Resolution: `BUSY_FORCE_DARK` env → `Config.theme` → `AppsUseLightTheme` (the *app* mode, as the design's "Flyouts and this window" follows Windows apps), the last read on the worker at open and on each `ImmersiveColorSet` broadcast. The window stays hidden until that first read, so it never flashes the wrong theme. Colors are `busy_ui::theme::Theme::new(dark)`; DWM's immersive dark mode is set for the frame.

## Gotchas

- The caption is removed in `WM_NCCALCSIZE` by restoring the top of the client rect after the default calculation, so Windows keeps the side/bottom resize borders, the shadow and the rounded corners; the top resize border is `HTTOP` from our `WM_NCHITTEST` over the top `SM_CYFRAME + SM_CXPADDEDBORDER` pixels. A maximized window hangs that much off screen, so the client top is pushed down by it. `SWP_FRAMECHANGED` after creation applies it.
- Caption buttons are client area (not `HTMINBUTTON`/`HTMAXBUTTON`/`HTCLOSE`), so we draw and click them ourselves; the cost is no Snap Layouts flyout on the maximize button.
- Linker warning 81010002 on `<dpiAwareness>` is benign; silenced with `#![allow(linker_messages)]`.
- Window icon loads from resource ID 1 of the exe (the app must embed its icon as ID 1).
- Testing: post messages to the demo's own HWNDs; never SendKeys (focus may be elsewhere). Mouse coordinates posted from a DPI-unaware process (PowerShell) are scaled by Windows to the window's DPI, so post DIPs, not pixels. A posted `WM_MOUSEMOVE` shows no hover: `TrackMouseEvent` sees the real cursor elsewhere and sends `WM_MOUSELEAVE` at once. While the session is locked a screen capture shows the lock screen; debug builds write each frame to `settings.bmp` in `BUSY_DUMP=<dir>` instead. Remove autostart after testing.
- Untested: dragging across monitors with different DPI; the autostart-failure message box (no safe way to make the `Run` key write fail).
