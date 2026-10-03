//! What each settings page shows (design `rows()`): headers, one card per setting and its control, built from
//! the config. Pure: the window lays these out and draws them, and turns input on a control into an `Edit`.

use super::choices::{self, Choices, Opt};
use busy_core::{CellStyle, Config, Module};
use busy_ui::i18n::{self, t};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Page {
    General,
    Module(Module),
    /// Opt-in sources (not in the design): each off until turned on, with its risk stated.
    Advanced,
}

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Page::General => t().settings.general,
            Page::Module(m) => t().common.module(m),
            Page::Advanced => t().settings.advanced,
        }
    }

    pub(super) fn sub(self) -> &'static str {
        let s = &t().settings;
        match self {
            Page::General => s.general_sub,
            Page::Module(m) => s.module_sub(m),
            Page::Advanced => s.advanced_sub,
        }
    }
}

/// Nav order: General, every module in config (taskbar) order, then Advanced.
pub(super) fn nav(cfg: &Config) -> Vec<Page> {
    std::iter::once(Page::General)
        .chain(cfg.modules.iter().map(|c| Page::Module(c.module)))
        .chain([Page::Advanced])
        .collect()
}

/// A module's nav status: whether it shows anywhere (its cell; Processes: its flyout lists).
pub(super) fn is_on(cfg: &Config, m: Module) -> bool {
    cfg.module(m).is_some_and(|c| if m == Module::Processes { c.flyout } else { c.taskbar })
}

/// The nav header's subtitle (design `enabledText`), with a singular the design lacks.
pub(super) fn readings(cfg: &Config) -> String {
    i18n::plural(&t().settings.readings, cfg.modules.iter().filter(|c| c.taskbar).count() as u64)
}

pub(super) enum Item {
    /// A module page's live preview of its cell.
    Preview(Module),
    Header(&'static str),
    /// A card of text only: General's notice that the config came from a newer busy.
    Notice(&'static str),
    Row(Row),
    /// General: the modules with a taskbar cell, in order, each with ↑/↓ (design "Taskbar order").
    Order(Vec<(Module, bool)>),
}

pub(super) struct Row {
    pub(super) title: &'static str,
    pub(super) desc: String,
    pub(super) control: Control,
}

pub(super) enum Control {
    Toggle(Flag, bool),
    /// Options and the selected one.
    Dropdown(Vec<Opt>, usize),
    /// The same, drawn as side-by-side segments.
    Segmented(Vec<Opt>, usize),
    /// The module palette, with the module's color selected.
    Swatches(Module, u8),
    /// A button that does something rather than edit the config.
    Button(Command),
    /// Nothing: the row only tells (About's version, from the Store).
    None,
}

/// What a row's button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    /// Turns the window into setup (onboarding).
    RunSetup,
    /// Opens the newer release's page when the update check found one, else the releases page.
    Releases,
}

impl Command {
    pub(super) fn label(self) -> &'static str {
        match self {
            Command::RunSetup => t().settings.run_setup,
            Command::Releases => t().settings.releases,
        }
    }
}

/// What a toggle switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Flag {
    /// Written to the registry by the window's worker, then reflected in the config.
    Autostart,
    Taskbar(Module),
    Label(Module),
    ByLoad(Module),
    Remaining,
    /// Processes' `flyout`: the top-process lists in the CPU, Memory, Disk and Network flyouts.
    TopProcesses,
    /// `opt_in.third_party_sensors`; turning it on asks first.
    ThirdPartySensors,
    /// `opt_in.update_check`; turning it on asks first.
    UpdateCheck,
    /// `opt_in.process_network`; turning it on asks first.
    ProcessNetwork,
}

impl Flag {
    /// An opt-in's risk, stated when it is turned on.
    pub(super) fn risk(self) -> Option<&'static str> {
        let s = &t().settings;
        match self {
            Flag::ThirdPartySensors => Some(s.third_party_desc),
            Flag::UpdateCheck => Some(s.update_check_desc),
            Flag::ProcessNetwork => Some(s.process_network_desc),
            _ => None,
        }
    }
}

/// A change the user made, applied to the config by `edit::apply`.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Edit {
    Flag(Flag, bool),
    Pick(choices::Pick),
    /// A palette index for a module's color.
    Color(Module, u8),
    /// Moves a module one place up (earlier) or down in the taskbar order.
    Move(Module, bool),
}

fn toggle(title: &'static str, desc: impl Into<String>, flag: Flag, on: bool) -> Item {
    Item::Row(Row { title, desc: desc.into(), control: Control::Toggle(flag, on) })
}

fn segmented(title: &'static str, desc: impl Into<String>, (opts, i): (Vec<Opt>, usize)) -> Item {
    Item::Row(Row { title, desc: desc.into(), control: Control::Segmented(opts, i) })
}

fn dropdown(title: &'static str, desc: impl Into<String>, (opts, i): (Vec<Opt>, usize)) -> Item {
    Item::Row(Row { title, desc: desc.into(), control: Control::Dropdown(opts, i) })
}

pub(super) fn items(page: Page, cfg: &Config, ch: &Choices) -> Vec<Item> {
    match page {
        Page::General => general(cfg, ch),
        Page::Module(Module::Processes) => processes(cfg),
        Page::Module(m) => module(m, cfg, ch),
        Page::Advanced => advanced(cfg, ch),
    }
}

/// "Find a setting": the rows of every page whose page name, title or description contains `query` in any
/// case, each page's under its name. Edits from these rows work as on their own page, since a row's control
/// names its module.
pub(super) fn search(query: &str, cfg: &Config, ch: &Choices) -> Vec<Item> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    for page in nav(cfg) {
        let whole = page.title().to_lowercase().contains(&q);
        let rows: Vec<Item> = items(page, cfg, ch)
            .into_iter()
            .filter(|i| match i {
                Item::Row(r) => whole || r.title.to_lowercase().contains(&q) || r.desc.to_lowercase().contains(&q),
                _ => false,
            })
            .collect();
        if !rows.is_empty() {
            out.push(Item::Header(page.title()));
            out.extend(rows);
        }
    }
    out
}

/// General opens with a notice for a config file a newer busy wrote (`Config::newer`), which is never saved over.
fn general(cfg: &Config, ch: &Choices) -> Vec<Item> {
    let notice = cfg.newer.then(|| Item::Notice(t().settings.newer));
    notice.into_iter().chain(general_rows(cfg, ch)).collect()
}

/// About's line: this build's version, and a newer one when the update check found it.
fn version(ch: &Choices) -> String {
    let (s, this) = (&t().settings, env!("CARGO_PKG_VERSION"));
    match &ch.newer {
        Some(r) => i18n::fill(s.newer_version, &[&this, &r.version]),
        None => i18n::fill(s.this_version, &[&this]),
    }
}

fn general_rows(cfg: &Config, ch: &Choices) -> Vec<Item> {
    let s = &t().settings;
    vec![
        Item::Header(s.behavior),
        toggle(s.autostart, s.autostart_desc, Flag::Autostart, cfg.autostart),
        dropdown(s.position, s.position_desc, choices::anchor(cfg.anchor, ch.edge.is_vertical())),
        dropdown(s.offset, s.offset_desc, choices::offset(cfg.offset_px)),
        dropdown(s.default_interval, s.default_interval_desc, choices::interval(cfg.interval_ms)),
        dropdown(s.history, s.history_desc, choices::history(cfg.history_secs)),
        Item::Header(s.appearance),
        dropdown(s.theme, s.theme_desc, choices::theme(cfg.theme)),
        dropdown(s.language, s.language_desc, choices::language(cfg.language)),
        Item::Row(Row { title: s.setup, desc: s.setup_desc.into(), control: Control::Button(Command::RunSetup) }),
        Item::Header(s.taskbar_order),
        Item::Order(
            cfg.modules.iter().filter(|c| c.module != Module::Processes).map(|c| (c.module, c.taskbar)).collect(),
        ),
        Item::Header(s.about),
        Item::Row(Row {
            title: s.version,
            desc: version(ch),
            control: if ch.packaged { Control::None } else { Control::Button(Command::Releases) },
        }),
    ]
}

fn module(m: Module, cfg: &Config, ch: &Choices) -> Vec<Item> {
    let s = &t().settings;
    let (Some(c), Some(show)) = (cfg.module(m), s.show_desc(m)) else { return Vec::new() };
    let o = &cfg.options;
    let mut r =
        vec![Item::Preview(m), Item::Header(s.taskbar), toggle(s.show_on_taskbar, show, Flag::Taskbar(m), c.taskbar)];
    r.push(segmented(s.style, s.style_desc, choices::style(m, c.style)));
    if c.style != CellStyle::Io {
        r.push(toggle(s.show_label, s.show_label_desc, Flag::Label(m), c.show_label));
    }
    match m {
        Module::Cpu => r.push(dropdown(s.cpu_bar, s.cpu_bar_desc, choices::cpu_bar(o.cpu.bar))),
        Module::Disk => r.push(dropdown(s.drive, s.drive_desc, choices::drive(o.disk.drive.as_deref(), ch))),
        Module::Network => {
            r.push(dropdown(s.units, s.units_desc, choices::units(o.network.units)));
            r.push(dropdown(s.interface, s.interface_desc, choices::interface(&o.network.interface, ch)));
        }
        Module::Battery => r.push(toggle(s.remaining, s.remaining_desc, Flag::Remaining, o.battery.show_remaining)),
        Module::Sensors => {
            r.push(dropdown(s.sensor, s.sensor_desc, choices::sensor(&o.sensors.sensor, ch)));
            r.push(segmented(s.unit, s.unit_desc, choices::temp_unit(cfg.temp_unit)));
        }
        _ => {}
    }
    r.push(Item::Header(s.color));
    r.push(Item::Row(Row {
        title: s.widget_color,
        desc: s.widget_color_desc.into(),
        control: Control::Swatches(m, c.color_index()),
    }));
    let by_load = if m == Module::Sensors { s.by_load_temperature } else { s.by_load_usage };
    r.push(toggle(s.by_load, by_load, Flag::ByLoad(m), c.color_by_load));
    r.extend(updates(m, cfg));
    r
}

fn processes(cfg: &Config) -> Vec<Item> {
    let (s, on) = (&t().settings, cfg.module(Module::Processes).is_some_and(|c| c.flyout));
    let mut r = vec![Item::Header(s.flyouts), toggle(s.top_processes, s.top_processes_desc, Flag::TopProcesses, on)];
    r.extend(updates(Module::Processes, cfg));
    r
}

/// Only the opt-ins that are built: the others would be switches that do nothing. From the Store, which updates
/// busy itself, there is no update check.
fn advanced(cfg: &Config, ch: &Choices) -> Vec<Item> {
    let (s, o) = (&t().settings, &cfg.opt_in);
    let mut r = vec![
        Item::Header(s.opt_in_sources),
        toggle(s.third_party, s.third_party_desc, Flag::ThirdPartySensors, o.third_party_sensors),
        toggle(s.process_network, s.process_network_desc, Flag::ProcessNetwork, o.process_network),
    ];
    if !ch.packaged {
        r.push(toggle(s.update_check, s.update_check_desc, Flag::UpdateCheck, o.update_check));
    }
    r
}

fn updates(m: Module, cfg: &Config) -> [Item; 2] {
    let (s, iv) = (&t().settings, cfg.module(m).and_then(|c| c.interval_s));
    [Item::Header(s.updates), dropdown(s.interval, s.interval_desc, choices::module_interval(m, iv, cfg.interval_ms))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use busy_core::release::Release;

    fn rows(items: &[Item]) -> Vec<&'static str> {
        items.iter().filter_map(|i| if let Item::Row(r) = i { Some(r.title) } else { None }).collect()
    }

    #[test]
    fn a_newer_config_is_noticed_on_general() {
        let mut cfg = Config::default();
        assert!(!items(Page::General, &cfg, &Choices::default()).iter().any(|i| matches!(i, Item::Notice(_))));
        cfg.newer = true;
        assert!(
            matches!(items(Page::General, &cfg, &Choices::default())[0], Item::Notice(n) if n == t().settings.newer)
        );
    }

    #[test]
    fn about_names_a_newer_release() {
        let this = env!("CARGO_PKG_VERSION");
        assert_eq!(version(&Choices::default()), format!("busy {this}"));
        let newer =
            Release { version: "9.0.0".into(), url: "https://github.com/aivanyuk/busy/releases/tag/v9.0.0".into() };
        let ch = Choices { newer: Some(newer), ..Choices::default() };
        assert_eq!(version(&ch), format!("busy {this} \u{2014} 9.0.0 is available"));
    }

    #[test]
    fn from_the_store_there_is_no_update_check_or_releases_link() {
        let cfg = Config::default();
        let releases = |ch: &Choices| {
            items(Page::General, &cfg, ch).iter().any(|i| {
                matches!(i, Item::Row(Row { title: "Version", control: Control::Button(Command::Releases), .. }))
            })
        };
        let portable = Choices::default();
        let store = Choices { packaged: true, ..Choices::default() };
        assert!(releases(&portable) && !releases(&store));
        assert_eq!(
            rows(&items(Page::Advanced, &cfg, &portable)),
            ["Third-party sensor tools", "Per-process network traffic", "Check for updates"]
        );
        assert_eq!(
            rows(&items(Page::Advanced, &cfg, &store)),
            ["Third-party sensor tools", "Per-process network traffic"]
        );
    }

    #[test]
    fn general_page() {
        let cfg = Config::default();
        let items = items(Page::General, &cfg, &Choices::default());
        assert_eq!(
            rows(&items),
            [
                "Start with Windows",
                "Widget position",
                "Offset",
                "Default update interval",
                "History",
                "Theme",
                "Language",
                "Setup",
                "Version"
            ]
        );
        assert!(matches!(items.last(), Some(Item::Row(Row { desc, .. })) if *desc == version(&Choices::default())));
        let Some(Item::Order(order)) = items.iter().find(|i| matches!(i, Item::Order(_))) else {
            panic!("no order list")
        };
        assert_eq!(order.len(), Module::ALL.len() - 1);
        assert!(order.iter().all(|&(m, _)| m != Module::Processes));
        assert_eq!(order[0], (Module::Cpu, true));
        // Down a vertical taskbar "left" is its top.
        let position = |edge| {
            let ch = Choices { edge, ..Choices::default() };
            super::items(Page::General, &cfg, &ch).into_iter().find_map(|i| match i {
                Item::Row(Row { title: "Widget position", control: Control::Dropdown(opts, _), .. }) => {
                    opts.get(1).map(|o| o.label.clone())
                }
                _ => None,
            })
        };
        assert_eq!(position(busy_win::Edge::Bottom).as_deref(), Some("Left edge of taskbar"));
        assert_eq!(position(busy_win::Edge::Right).as_deref(), Some("Top of taskbar"));
    }

    #[test]
    fn module_pages_follow_the_design() {
        let mut cfg = Config::default();
        let ch = Choices::default();
        assert_eq!(
            rows(&items(Page::Module(Module::Cpu), &cfg, &ch)),
            ["Show on taskbar", "Style", "Show label", "Bar shows", "Widget color", "Color by load", "Update interval"]
        );
        // Network's default style is Io, which has no label.
        assert_eq!(
            rows(&items(Page::Module(Module::Network), &cfg, &ch)),
            ["Show on taskbar", "Style", "Units", "Interface", "Widget color", "Color by load", "Update interval"]
        );
        cfg.modules.iter_mut().for_each(|c| c.style = CellStyle::Text);
        let sensors = rows(&items(Page::Module(Module::Sensors), &cfg, &ch));
        assert!(sensors.contains(&"Taskbar sensor") && sensors.contains(&"Unit"));
        assert_eq!(rows(&items(Page::Module(Module::Processes), &cfg, &ch)), ["Top processes", "Update interval"]);
        let memory = items(Page::Module(Module::Memory), &cfg, &ch);
        assert!(matches!(memory[0], Item::Preview(Module::Memory)));
        let Item::Row(r) = &memory[2] else { panic!("no row") };
        assert_eq!(r.desc, "Display the memory reading on the taskbar");
        assert!(!items(Page::Module(Module::Processes), &cfg, &ch).iter().any(|i| matches!(i, Item::Preview(_))));
    }

    #[test]
    fn advanced_page_lists_the_built_opt_ins() {
        let mut cfg = Config::default();
        let ch = Choices::default();
        let off = items(Page::Advanced, &cfg, &ch);
        assert_eq!(rows(&off), ["Third-party sensor tools", "Per-process network traffic", "Check for updates"]);
        let Some(Item::Row(Row { control: Control::Toggle(Flag::ThirdPartySensors, false), desc, .. })) = off.get(1)
        else {
            panic!("no opt-in toggle")
        };
        assert!(desc.contains("Reads data published by another program you installed."));
        cfg.opt_in.third_party_sensors = true;
        let on = items(Page::Advanced, &cfg, &ch);
        assert!(matches!(on.get(1), Some(Item::Row(Row { control: Control::Toggle(_, true), .. }))));
        assert_eq!(rows(&search("hwinfo", &cfg, &ch)), ["Third-party sensor tools"]);
        assert_eq!(rows(&search("github", &cfg, &ch)), ["Check for updates"]);
        assert_eq!(rows(&search("administrator", &cfg, &ch)), ["Per-process network traffic"]);
        cfg.opt_in.process_network = true;
        let on = items(Page::Advanced, &cfg, &ch);
        assert!(matches!(on.get(2), Some(Item::Row(Row { control: Control::Toggle(Flag::ProcessNetwork, true), .. }))));
        assert_eq!(Flag::ProcessNetwork.risk(), Some(t().settings.process_network_desc));
        assert_eq!(Flag::UpdateCheck.risk(), Some(t().settings.update_check_desc));
        assert_eq!(Flag::Autostart.risk(), None);
    }

    #[test]
    fn search_finds_rows_on_every_page() {
        let (cfg, ch) = (Config::default(), Choices::default());
        let found = search("  LABEL ", &cfg, &ch);
        let headers: Vec<_> =
            found.iter().filter_map(|i| if let Item::Header(h) = i { Some(*h) } else { None }).collect();
        // Network's default style is Io, which has no label row; Battery's time left "replaces the label".
        assert_eq!(headers, ["CPU", "Memory", "GPU", "Disk", "Sensors", "Battery"]);
        assert!(rows(&found).iter().all(|&t| t == "Show label" || t == "Show time remaining"));
        // A page's name finds all of its rows, and other pages' rows that mention it.
        assert_eq!(
            rows(&search("processes", &cfg, &ch)),
            ["Top processes", "Update interval", "Per-process network traffic"]
        );
        assert!(search("zzz", &cfg, &ch).is_empty());
    }

    #[test]
    fn nav_and_status() {
        let cfg = Config::default();
        let nav = nav(&cfg);
        assert_eq!((nav[0], nav.len()), (Page::General, Module::ALL.len() + 2));
        assert_eq!(nav.last(), Some(&Page::Advanced));
        assert!(is_on(&cfg, Module::Cpu) && !is_on(&cfg, Module::Disk) && is_on(&cfg, Module::Processes));
        assert_eq!(readings(&cfg), "4 readings on the taskbar");
        let mut one = cfg.clone();
        one.modules.iter_mut().for_each(|c| c.taskbar = c.module == Module::Cpu);
        assert_eq!(readings(&one), "1 reading on the taskbar");
    }
}
