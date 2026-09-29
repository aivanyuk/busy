//! What each settings page shows (design `rows()`): headers, one card per setting and its control, built from
//! the config. Pure: the window lays these out and draws them, and turns input on a control into an `Edit`.

use super::choices::{self, Choices, Opt};
use busy_core::{CellStyle, Config, Module};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    General,
    Module(Module),
}

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Module(m) => m.label(),
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
        }
    }
}

/// Nav order: General, then every module in config (taskbar) order.
pub(super) fn nav(cfg: &Config) -> Vec<Page> {
    std::iter::once(Page::General).chain(cfg.modules.iter().map(|c| Page::Module(c.module))).collect()
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
    Header(&'static str),
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
        Page::General => general(cfg),
        Page::Module(Module::Processes) => processes(cfg),
        Page::Module(m) => module(m, cfg, ch),
    }
}

fn general(cfg: &Config) -> Vec<Item> {
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
        Item::Header("Taskbar order"),
        Item::Order(
            cfg.modules.iter().filter(|c| c.module != Module::Processes).map(|c| (c.module, c.taskbar)).collect(),
        ),
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

    fn rows(items: &[Item]) -> Vec<&'static str> {
        items.iter().filter_map(|i| if let Item::Row(r) = i { Some(r.title) } else { None }).collect()
    }

    #[test]
    fn general_page() {
        let cfg = Config::default();
        let items = items(Page::General, &cfg, &Choices::default());
        assert_eq!(
            rows(&items),
            ["Start with Windows", "Widget position", "Offset", "Default update interval", "History", "Theme"]
        );
        let Some(Item::Order(order)) = items.last() else { panic!("no order list") };
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
        let Some(Item::Row(r)) = items(Page::Module(Module::Memory), &cfg, &ch).into_iter().nth(1) else {
            panic!("no row")
        };
        assert_eq!(r.desc, "Display the memory reading on the taskbar");
    }

    #[test]
    fn nav_and_status() {
        let cfg = Config::default();
        let nav = nav(&cfg);
        assert_eq!((nav[0], nav.len()), (Page::General, Module::ALL.len() + 1));
        assert!(is_on(&cfg, Module::Cpu) && !is_on(&cfg, Module::Disk) && is_on(&cfg, Module::Processes));
        assert_eq!(readings(&cfg), "4 readings on the taskbar");
        let mut one = cfg.clone();
        one.modules.iter_mut().for_each(|c| c.taskbar = c.module == Module::Cpu);
        assert_eq!(readings(&one), "1 reading on the taskbar");
    }
}
