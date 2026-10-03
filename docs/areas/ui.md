# busy-ui

`crates/ui` — what busy's windows show and how, shared by the app (taskbar widget, flyout) and the settings window, so both draw from one source.

- Pure logic, unit-tested: `fmt.rs` (human-readable values), `select.rs` (what to show: the taskbar sensor, the busiest GPU, the disk volume and network interface a module reads, CPU core bars), `history.rs` (rolling series per module, sized by `history_secs` at the module's interval), `tone.rs` (which design color: module palette, load colors).
- Drawing: `theme.rs` (the design's tokens and palettes, light/dark resolution), `render.rs` (`Gfx`: D2D factory, DWrite formats with tabular figures and tracking; `Canvas`: fills, outlines, text, bars, sparklines), `ctx.rs` (`Ctx`, the read-only view a window draws from: config, snapshot, history, theme, `Gfx`).
- The taskbar cell (design `MeterWidget`): `cell/mod.rs` decides a module's label, value and colors as a `Cell` (`cell` for one module whether or not it is on the taskbar, `cells` for the taskbar's), `cell/styles.rs` measures and draws it in its style across a horizontal taskbar, `cell/column.rs` down a vertical one (`column_width`, `column_height`, `draw_column`). The taskbar widget lays cells out; the settings window draws one as a module page's preview. `sample` gives a module's reading as one short value whatever the style (the percentage, Disk's drive fullness, Network's download rate, the taskbar sensor), for onboarding's reading cards.

## Text and languages (`i18n/`)

Every string a window shows comes from `i18n::t()`, the current language's `Strings`; none is a literal at the call site. `Strings` is split into sections by the part of the UI that shows them (`common.rs`: module names, …), each declared once with `section!`: the field, its type and its English text. The ten translations are `i18n/lang/<lang>.rs`, each a whole `Strings` value, so a string added to a section without its translations doesn't build. All tables are compiled in: no file is read, no lookup can fail, and a `&'static str` API stays one.

- Language: `i18n::init(system, cfg.language)` at startup, `system` being `Lang::pick` over `busy_win::ui_languages()` (Windows' display languages, read before the widget is embedded); `i18n::select(cfg.language)` when the config changes. One process-wide value; windows read it when they build their text and formats.
- Values in a string: a pattern with numbered slots, `"{0} free of {1}"`, filled by `i18n::fill`, so a translation can put them in its own order. Never `format!` a translated word into a sentence, or concatenate pieces: word order and case differ by language.
- Counts: a `Plural` (`one`, `few`, `other`) and `i18n::plural(&p, n)`, or `i18n::plural_with(&p, n, &[&n, &more])` when the string holds other values too (`{1}`…); the rules (`form`) cover English, German, Spanish, Italian (1 is `one`), French and Portuguese (0 and 1), Russian and Polish (`few` for 2–4 except 12–14), and Japanese, Korean and Chinese (one form).
- Tests check every language for every string, non-empty and with the same slots as English (a `Plural` form may drop `{0}`: "one minute"). Unit tests elsewhere assert English text: they never select another language, and the default is English.
- DirectWrite formats take the language's tag as their locale (`Gfx::format_weight`), which decides font fallback's glyphs for Han characters.
- Adding a string: a field in its section's `section!` with the English text, then the same field in each `lang/*.rs`.

Everything here runs on the UI thread and never blocks, except `Theme::resolve`, which reads the registry for `ThemeMode::System` (call it on a worker, or at startup before a window exists).

The app's area doc (`docs/areas/app.md`) describes how these choices appear on the taskbar and in the flyout.
