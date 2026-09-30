# busy-settings

`crates/settings` — the settings window, custom-drawn with Direct2D to the design (`design/Meterbar.dc.html`, Settings), and autostart. Files: `lib.rs` (API), `autostart.rs`, `dark.rs` (theme resolution, DWM title-bar mode), and `window/`, one concern per file:

- `mod.rs` — `Ui` state, create/open, applying edits, autostart through the worker, rendering to an `ID2D1HwndRenderTarget`.
- `model.rs` — what each page shows (design `rows()`): headers, one card per setting with its control, the taskbar order; `choices.rs` — each dropdown's options and the value each sets; `edit.rs` — applying an `Edit` to the config. All pure and tested.
- `layout.rs` — the design's measures, the `View` (page, items, their places, scroll, hover, open popup) and hit-testing; `paint.rs` — drawing the whole window from the `View`.
- `controls/` — one file per control kind (`toggle`, `dropdown` with its popup, `segmented`, `swatch`, `preview`, `order` for the taskbar order list, `nav` for the nav column); each sizes and draws itself and names the part under the pointer.
- `input.rs` — pointer input: hover, clicks, wheel; a click on a control becomes a `model::Edit`; `keys.rs` — keyboard input and focus.
- `live.rs` — following the host: `sync` (configs it applied) and `refresh` (new readings: the preview, the machine lists); `clock.rs` — the preview's time and date in the user's formats.
- `uia/` — UI Automation: `node.rs` (the elements, their roles, names and state, read from the `View`; pure and tested), `tree.rs` (a frame's elements, `Tree` and `Entry`), `setup.rs` (setup's elements), `provider.rs` (the COM providers, answering from a published `Tree`), `mod.rs` (`WM_GETOBJECT`, publishing each frame, requests, events).
- `setup/` — setup (onboarding), the window's other mode: `mod.rs` (what it shows, layout, hit-testing, Tab stops), `paint.rs`, `input.rs` (entering setup, pointer, keyboard, finishing).
- `frame.rs` — the custom title bar and the non-client handling behind it; `wndproc.rs` — the window procedure; `worker.rs` — registry thread (autostart read/write, app theme); `dump.rs` — debug-only frame dump.

Standalone check: `cargo run -p busy-settings --example demo` (set `BUSY_FORCE_DARK=1` / `0` to force a theme, `BUSY_SETUP=1` to open setup). The demo's host lends fixed synthetic readings, so previews and machine lists show without the app; applied configs are printed, not saved.

## API (called from the app's UI thread)

```rust
pub trait Host {
    fn apply(&self, cfg: Config);                                   // after every edit: apply live, persist
    fn shown(&self, modules: &[Module]);                            // the live readings shown: sample them
    fn with_data(&self, f: &mut dyn FnMut(&Snapshot, &History));    // lend readings for a frame (may skip)
}
pub fn open(_owner: HWND, cfg: &Config, host: Rc<dyn Host>, page: Option<Module>); // opens or focuses; `page` shows that module's page
pub fn setup(cfg: &Config, host: Rc<dyn Host>);  // opens setup (onboarding), or turns the open window into it
pub fn sync(cfg: &Config);                       // the host applied a config (from anywhere): show it
pub fn refresh(snap: &Snapshot, hist: &History); // new readings: preview and machine lists
pub fn is_open() -> bool;
pub fn is_dialog_message(msg: &MSG) -> bool; // always false now: the window handles its keys itself
pub mod autostart { pub fn is_enabled() -> bool; pub fn set(enabled: bool) -> windows::core::Result<()>; }
```

- Window is unowned top-level with its own taskbar button (the app's HWND is a child of explorer's taskbar — can't own).
- Changes apply live, as in the design (no OK/Apply): every edit is normalized and, if the config changed, handed to `Host::apply`; the **host** persists it (`Config::save`, on the app's config writer) and calls `sync` with what it applied, which also brings in changes made elsewhere (the widget's menu).
- `Host::shown` is called from inside `open` too, and every `Host` call comes from the window's own message handling, so the host must not call back into the window synchronously; the app's host posts to its main window.
- `page` is how a flyout's "<Module> settings" button lands on its module, also in a window already open.
- Autostart is written by the window's registry worker (`window/worker.rs`): the toggle is drawn disabled until the first read, and again while a write is in flight; the value read back after the write goes into the config. On failure a message box says so. Closing during a write closes once it lands.
- `autostart::{is_enabled, set}` block on the registry: call them off a UI thread (the window calls them only from its worker).
- `autostart::is_enabled` also checks the Task Manager "disabled" flag (`StartupApproved\Run`); `set(true)` clears it, `set(false)` removes both values.

## Layout (design measures, in DIPs)

- Window 1000 × 700 (min 760 × 576, which fits the whole nav), solid `--win` background (the design has no Mica), centered on the primary work area. Title bar 40 high: app glyph, "busy Settings", 46 × 40 caption buttons (Close hover `#C42B1C`; Minimize and Maximize work, unlike the prototype's).
- Nav 272 wide: app header (48 tile, "busy", "N readings on the taskbar" with a singular for 1), the search box, then General, one item per module in config order (color dot, On/Off; Processes is On when its top-process lists are) and Advanced. The selected item has `--hover` and a 3 × 18 accent pill.
- Content: padding 12 32 32 20, children 4 apart; page title 28/600, subtitle 13 `--fg2`; section headers 14/600 with 14 above; one card per setting (min 68 high, padding 12 16 12 20, title 14, description 12 `--fg2` wrapping, control right-aligned, or under the text when that would be narrower than 220). Wheel scrolls 48 per notch; a thin thumb shows when the page is taller than the pane.
- Pages: General (Behavior: Start with Windows, Widget position, Offset, Default update interval, History; Appearance: Theme; Taskbar order with ↑/↓, Processes left out as it has no cell) and one per module (Taskbar: Show on taskbar, Style — segments of `Module::allowed_styles()`, Io named "Read / write" for Disk and "Up / down" for Network —, Show label except for Io, module options, and for Sensors the °C/°F Unit; Color: Widget color swatches from the module palette, Color by load; Updates: Update interval with "Default (…)"). Processes' page has Top processes and its interval. Offset and History are not in the design; they keep settings the native window had.
- Advanced (not in the design): "Opt-in sources", one toggle per opt-in that is built — today only Third-party sensor tools (`opt_in.third_party_sensors`) — with the plan's text for what it reads and its risk, verbatim, as the description. Turning one on asks first (a Yes/No warning box, No by default, repeating that text); turning it off doesn't. The box is modal with its own message loop, so the UI thread keeps pumping; no `RefCell` borrow is held across it, and a window closed meanwhile applies nothing. Opt-ins that aren't built aren't listed: they would be switches that do nothing.
- Module pages (not Processes) open with the design's preview card: "Preview" and "Live" (or "Hidden — turn on “Show on taskbar”") over a 48-high `--tb` strip with the module's cell, built and drawn by `busy_ui::cell` exactly as on the taskbar, and the clock. A module without a cell is drawn at 40 % (the strip color at 60 % over it). The host samples the shown page's module (`Host::shown`), so the preview is live without a cell; it redraws only when the cell's key or the clock text changes (`refresh`).
- Drive, Interface and Taskbar sensor list the machine's volumes ("Label (C:)"), adapters and temperature readings ("hardware · name") from the latest readings, after the design's options ("System drive"; Automatic, Wi‑Fi, Ethernet; CPU package, GPU, Drive). A list that comes back empty (its module isn't sampled) keeps the last one, and nothing changes under an open popup.
- Search ("Find a setting"; the design draws the box but gives it no behaviour): while the query isn't blank, the content is "Search results" — every page's rows whose page name, title or description contains it, in any case, under their page's name, with a count as subtitle — and no nav item is selected. Rows found there edit their own module. Picking a nav item ends the search. The box shows the query, a caret and WinUI's 2-DIP accent bottom edge while it has the focus; the query is capped at 64 characters.
- Dropdowns list the design's options; a current value that isn't one of them (a hand-edited file, a drive that is gone) is kept as an extra option, so opening the window never changes a setting.
- The popup opens 4 below its button (above it without room), at least as wide as the button, and scrolls by wheel when its options don't fit. A click outside it only closes it; Esc closes it, and without a popup Esc closes the window.

## Setup (onboarding)

Design "FIRST RUN" (`Meterbar.dc.html` onboarding): pick readings, pick a side, done. It is the window's second mode (`Mode::Setup`), sharing its render target, worker, theme and frame; the app opens it at startup while `Config::onboarded` is false (`busy_settings::setup`).
- Fixed 680-DIP client, height from the content (about 553 with the subtitle on one line), centered on the primary work area; no resize border, no maximize, Close only; title "busy". A minimized or maximized window is restored before it is measured; after a DPI change the window keeps its place and is sized to the layout again (the suggested size is for a standard frame).
- "Your PC’s vitals, right on the taskbar" (28/600) and its subtitle (14 `--fg2`, wrapping); "Show on taskbar": a 3-column grid of 52-high reading cards (radius 6, `--card`, border `--accent` when checked), one per module with a cell in taskbar order, each a 20-DIP checkbox, the name and the module's reading (`busy_ui::cell::sample`, 12/600 in its palette color, live); "Position": two radio cards, next to the tray (default) or at the left edge; the footer (`--footer`, a `--line` top edge) with Start with Windows, Skip and Start monitoring (accent).
- Every choice applies live (`Host::apply`), so the taskbar behind the window follows it, as the design means. The host samples every card's module while setup is up (`Host::shown`). Start with Windows shows and writes the registry through the worker, like the Settings toggle; it starts as the registry has it (unchecked on a fresh install; the design checks it).
- Skip, Start monitoring, Close, Esc and Enter all finish the same way: the choices stay, `onboarded` is set, the window closes (after a pending autostart write lands; the page takes no input meanwhile). A held Space, Enter or Esc acts once, so the Enter that pressed "Run setup" doesn't finish it as it repeats. Start with Windows is drawn in `--fg3` while the registry is busy. Closing it any other way (the app exiting) leaves `onboarded` false, so it comes back on the next start.
- Keyboard: Tab / Shift+Tab over the cards, the position group (one stop), Start with Windows, Skip, Start monitoring; Space checks, picks or presses the focused one; arrows move among the cards (3 wide) and pick the neighbouring position. The first card has the focus when it opens, without the ring.
- While setup is up, a flyout's "<Module> settings" only brings it forward.

## Keyboard

- Tab / Shift+Tab cycle through the stops: the search box, the nav (one stop: the focused, else the selected item), every row's control, then each usable ↑/↓ of the taskbar order. The window handles its keys itself (no `IsDialogMessage`), so `is_dialog_message` returns false.
- Nav: ↑/↓ move the focus, Space/Enter shows that page. Toggle: Space/Enter flips it. Dropdown: Space/Enter, F4, ↓ or Alt+↓ open it; in the popup ↑/↓/Home/End move a cursor (scrolling it into view), Space/Enter pick, Esc/Tab/F4 close. Segments and swatches: ←/→ pick the neighbour. Order ↑/↓: Space/Enter move the module, and the focus follows it.
- Ctrl+F focuses the search box. There, typing filters, Backspace deletes, Esc clears (and on an empty box closes the window, as anywhere), Enter or ↓ go to the first result.
- Page Up/Down scroll the page; a focused control is scrolled into view. Esc closes an open popup, else the window. Alt+F4 and Alt+Space go to the default handling.
- The focus ring (WinUI's focus visual: 2-DIP `--fg`, 3 outside the target) shows only once the keyboard is used; a click moves the focus without it.

## UI Automation

Screen readers see the window through UIA (plan: basic roles and names). `WM_GETOBJECT` for `UiaRootObjectId` returns a root provider, a fragment root over the window's own (host) provider, whose children are, in reading order:
- the caption buttons (Button: Minimize, Maximize or Restore, Close; invoked, not focusable, as they aren't Tab stops), the search box (Edit, with a Value that can be set), the nav items (ListItem; status On/Off and "selected");
- the page title (Text; the subtitle as help text), section headers (Text), each row's control named by its title with the description as help text: toggle → CheckBox (Toggle), dropdown → ComboBox (read-only Value), segments and swatches → Group (read-only Value: the choice, "Color n of 8"); the taskbar order's usable ↑/↓ (Button, "Move CPU up");
- while open, the popup (List) with its options (ListItem, "Selected" on the current one).
- In setup: Close, the headline (Text, the subtitle as help text) and the two headings, one CheckBox (Toggle) per reading card named by its module with the live reading as its status, the positions as RadioButton (SelectionItem: IsSelected, Select picks; also Invoke), Start with Windows (CheckBox), Skip and Start monitoring (Button). Its elements never change shape, so their keys are 0 (`uia/setup.rs`).

Buttons, nav items and options have Invoke. Invoke, Toggle, SetFocus and SetValue are queued and posted to the window, so they run as the same key would from its message loop, never inside a UIA call: a toggle's Toggle on an opt-in shows the confirmation, SetFocus and SetValue close an open popup as a key would. A disabled control (the autostart toggle while the registry is busy) answers Invoke and Toggle with `UIA_E_ELEMENTNOTENABLED`.

Nothing is built until a client first sends `WM_GETOBJECT`. From then on, after each frame (`WM_PAINT`), the window publishes its elements as a `node::Tree` and announces what changed since the last one: a structure change when the top-level elements differ (another page, a search, a row coming or going, a popup opening), the focus when it moved (the popup's cursor while it is open), else a change to the focused toggle or value as a property change (typing in the search box is value changes); focus and property events only while the window is in the foreground. Content elements are named by index plus a key (a hash of the page or search they were read from, the index and their name, also in the runtime id), so an element whose row went away, or whose popup closed, is "not available" rather than another row.

## Theme

Resolution: `BUSY_FORCE_DARK` env → `Config.theme` → `AppsUseLightTheme` (the *app* mode, as the design's "Flyouts and this window" follows Windows apps), the last read on the worker at open and on each `ImmersiveColorSet` broadcast. The window stays hidden until that first read, so it never flashes the wrong theme. Colors are `busy_ui::theme::Theme::new(dark)`; DWM's immersive dark mode is set for the frame.

## Gotchas

- The caption is removed in `WM_NCCALCSIZE` by restoring the top of the client rect after the default calculation, so Windows keeps the side/bottom resize borders, the shadow and the rounded corners; the top resize border is `HTTOP` from our `WM_NCHITTEST` over the top `SM_CYFRAME + SM_CXPADDEDBORDER` pixels. A maximized window hangs that much off screen, so the client top is pushed down by it. `SWP_FRAMECHANGED` after creation applies it.
- Caption buttons are client area (not `HTMINBUTTON`/`HTMAXBUTTON`/`HTCLOSE`), so we draw and click them ourselves; the cost is no Snap Layouts flyout on the maximize button.
- Linker warning 81010002 on `<dpiAwareness>` is benign; silenced with `#![allow(linker_messages)]`.
- Window icon loads from resource ID 1 of the exe (the app must embed its icon as ID 1).
- Testing: post messages to the demo's own HWNDs; never SendKeys (focus may be elsewhere). Mouse coordinates posted from a DPI-unaware process (PowerShell) are scaled by Windows to the window's DPI, so post DIPs, not pixels. A posted `WM_MOUSEMOVE` shows no hover: `TrackMouseEvent` sees the real cursor elsewhere and sends `WM_MOUSELEAVE` at once. While the session is locked a screen capture shows the lock screen; debug builds write each frame to `settings.bmp` in `BUSY_DUMP=<dir>` instead. Remove autostart after testing.
- UIA calls providers on its own threads (seen: every call but those inside `WM_GETOBJECT`), whether or not they are agile, as the UI thread has no COM apartment. The window's state is thread-local, so providers never touch it: they read the `Tree` published after each frame (an `Arc` behind a `Mutex` that only covers swapping or cloning it, with the scale; screen positions come from `ClientToScreen` at call time) and post what they are asked to do. No UIA thread ever waits on the UI thread, nor the other way round. When the window is destroyed the tree is cleared (every call then answers "not available") and all providers are disconnected.
- Checking UIA: from Windows PowerShell, `System.Windows.Automation` (UIAutomationClient) reads the demo's tree, invokes patterns and, from a compiled C# handler, logs events; PowerShell scriptblocks as event handlers never run (UIA calls them on its threads). Accessibility Insights or Narrator do the same by hand.
- Untested: dragging across monitors with different DPI; the autostart-failure message box (no safe way to make the `Run` key write fail).
