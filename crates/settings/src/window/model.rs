//! What each settings page shows (design `rows()`): headers, one card per setting and its control, built from
//! the config. Pure: the window lays these out and draws them, and turns input on a control into an `Edit`.

use super::choices::{self, Choices, Opt};
use busy_core::{CellStyle, Config, Module};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Page {
    General,
    Module(Module),
    /// Opt-in sources (not in the design): each off until turned on, with its risk stated.
    Advanced,
}

/// Third-party sensor tools: what the opt-in reads and its risk, verbatim from the plan's Opt-in sources.
pub(super) const THIRD_PARTY: &str = "Reading LibreHardwareMonitor (WMI) / HWiNFO (shared memory). Reads data \
     published by another program you installed. Needed for CPU temps, fans, SSD temp/health, CPU power, \
     throttling.";

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Module(m) => m.label(),
            Page::Advanced => "Advanced",
        }
    }

    pub(super) fn sub(self) -> &'static str {
        match self {
            Page::General => "Startup, placement, refresh rate and theme",
            Page::Module(m) => match m {
                Module::Cpu => "Processor usage",
                Module::Gpu => "Graphics processor usage",
                Module::Memory => "Memory in use",
                Module::Disk => "Drive activity and free space",
                Module::Network => "Transfer rates",
                Module::Battery => "Charge and time remaining",
                Module::Sensors => "Temperatures, fans and power",
                Module::Processes => "The busiest programs, listed in flyouts",
            },
            Page::Advanced => "Optional data sources, each off until you turn it on",
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
    match cfg.modules.iter().filter(|c| c.taskbar).count() {
        1 => "1 reading on the taskbar".into(),
        n => format!("{n} readings on the taskbar"),
    }
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
            Command::RunSetup => "Run setup",
            Command::Releases => "Releases",
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
    /// Processes' `flyout`: the top-process lists in the CPU, Memory and Disk flyouts.
    TopProcesses,
    /// `opt_in.third_party_sensors`; turning it on asks first.
    ThirdPartySensors,
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
        Page::Advanced => advanced(cfg),
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

/// General's notice for a config file a newer busy wrote (`Config::newer`), which is never saved over.
pub(super) const NEWER: &str =
    "These settings were saved by a newer version of busy. Changes apply now, but aren\u{2019}t saved.";

fn general(cfg: &Config, ch: &Choices) -> Vec<Item> {
    let notice = cfg.newer.then_some(Item::Notice(NEWER));
    notice.into_iter().chain(general_rows(cfg, ch)).collect()
}

/// About's line: this build's version, and a newer one when the update check found it.
fn version(ch: &Choices) -> String {
    let this = env!("CARGO_PKG_VERSION");
    match &ch.newer {
        Some(r) => format!("busy {this} \u{2014} {} is available", r.version),
        None => format!("busy {this}"),
    }
}

fn general_rows(cfg: &Config, ch: &Choices) -> Vec<Item> {
    vec![
        Item::Header("Behavior"),
        toggle("Start with Windows", "Launch busy when you sign in", Flag::Autostart, cfg.autostart),
        dropdown("Widget position", "Where readings sit on the taskbar", choices::anchor(cfg.anchor)),
        dropdown("Offset", "Extra space between the readings and that edge", choices::offset(cfg.offset_px)),
        dropdown(
            "Default update interval",
            "Each widget can override this on its own page",
            choices::interval(cfg.interval_ms),
        ),
        dropdown("History", "How far back the flyout charts go", choices::history(cfg.history_secs)),
        Item::Header("Appearance"),
        dropdown("Theme", "Flyouts and this window", choices::theme(cfg.theme)),
        Item::Row(Row {
            title: "Setup",
            desc: "Walk through choosing widgets again".into(),
            control: Control::Button(Command::RunSetup),
        }),
        Item::Header("Taskbar order"),
        Item::Order(
            cfg.modules.iter().filter(|c| c.module != Module::Processes).map(|c| (c.module, c.taskbar)).collect(),
        ),
        Item::Header("About"),
        Item::Row(Row { title: "Version", desc: version(ch), control: Control::Button(Command::Releases) }),
    ]
}

fn module(m: Module, cfg: &Config, ch: &Choices) -> Vec<Item> {
    let Some(c) = cfg.module(m) else { return Vec::new() };
    let o = &cfg.options;
    let name = match m {
        Module::Cpu | Module::Gpu => m.label().to_string(),
        _ => m.label().to_lowercase(),
    };
    let mut r = vec![
        Item::Preview(m),
        Item::Header("Taskbar"),
        toggle("Show on taskbar", format!("Display the {name} reading on the taskbar"), Flag::Taskbar(m), c.taskbar),
    ];
    r.push(segmented("Style", "How the reading is drawn", choices::style(m, c.style)));
    if c.style != CellStyle::Io {
        r.push(toggle("Show label", "Small caption above the value", Flag::Label(m), c.show_label));
    }
    match m {
        Module::Cpu => r.push(dropdown("Bar shows", "Used by the Bar style", choices::cpu_bar(o.cpu.bar))),
        Module::Disk => {
            r.push(dropdown("Drive", "Used by the Text and Bar styles", choices::drive(o.disk.drive.as_deref(), ch)));
        }
        Module::Network => {
            r.push(dropdown("Units", "Transfer rate units", choices::units(o.network.units)));
            r.push(dropdown("Interface", "Which adapter to measure", choices::interface(&o.network.interface, ch)));
        }
        Module::Battery => r.push(toggle(
            "Show time remaining",
            "Replaces the label with hours:minutes left",
            Flag::Remaining,
            o.battery.show_remaining,
        )),
        Module::Sensors => {
            r.push(dropdown("Taskbar sensor", "Which temperature to show", choices::sensor(&o.sensors.sensor, ch)));
            r.push(segmented("Unit", "Applies everywhere in busy", choices::temp_unit(cfg.temp_unit)));
        }
        _ => {}
    }
    let rises = if m == Module::Sensors { "temperature" } else { "usage" };
    r.push(Item::Header("Color"));
    r.push(Item::Row(Row {
        title: "Widget color",
        desc: "Used for graphs, bars and the flyout chart".into(),
        control: Control::Swatches(m, c.color_index()),
    }));
    r.push(toggle(
        "Color by load",
        format!("Shift green \u{2192} amber \u{2192} red as {rises} rises"),
        Flag::ByLoad(m),
        c.color_by_load,
    ));
    r.extend(updates(m, cfg));
    r
}

fn processes(cfg: &Config) -> Vec<Item> {
    let on = cfg.module(Module::Processes).is_some_and(|c| c.flyout);
    let mut r = vec![
        Item::Header("Flyouts"),
        toggle(
            "Top processes",
            "List the busiest programs in the CPU, Memory and Disk flyouts",
            Flag::TopProcesses,
            on,
        ),
    ];
    r.extend(updates(Module::Processes, cfg));
    r
}

/// Only the opt-ins that are built: the others would be switches that do nothing.
fn advanced(cfg: &Config) -> Vec<Item> {
    vec![
        Item::Header("Opt-in sources"),
        toggle("Third-party sensor tools", THIRD_PARTY, Flag::ThirdPartySensors, cfg.opt_in.third_party_sensors),
    ]
}

fn updates(m: Module, cfg: &Config) -> [Item; 2] {
    let iv = cfg.module(m).and_then(|c| c.interval_s);
    [
        Item::Header("Updates"),
        dropdown(
            "Update interval",
            "How often this reading refreshes",
            choices::module_interval(m, iv, cfg.interval_ms),
        ),
    ]
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
        assert!(matches!(items(Page::General, &cfg, &Choices::default())[0], Item::Notice(NEWER)));
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
        assert_eq!(rows(&off), ["Third-party sensor tools"]);
        let Some(Item::Row(Row { control: Control::Toggle(Flag::ThirdPartySensors, false), desc, .. })) = off.get(1)
        else {
            panic!("no opt-in toggle")
        };
        assert!(desc.contains("Reads data published by another program you installed."));
        cfg.opt_in.third_party_sensors = true;
        let on = items(Page::Advanced, &cfg, &ch);
        assert!(matches!(on.get(1), Some(Item::Row(Row { control: Control::Toggle(_, true), .. }))));
        assert_eq!(rows(&search("hwinfo", &cfg, &ch)), ["Third-party sensor tools"]);
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
        // A page's name finds all of its rows.
        assert_eq!(rows(&search("processes", &cfg, &ch)), ["Top processes", "Update interval"]);
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
