//! The settings window's pages: their titles, section headers and the rows on them.

use super::Plural;
use busy_core::Module;

section! {
    /// The settings window's pages (the nav and each page's title and subtitle), their section headers and
    /// rows. A row's title is one line, cut off with "…" when too long; its description wraps. Search matches
    /// both as shown.
    Settings {
        /// The window's title bar and taskbar button; "busy" is the product's name.
        window_title: &'static str = "busy Settings",
        /// The search box's placeholder, over the nav.
        find: &'static str = "Find a setting",
        /// A module's state: a nav item's status (small, right of its name) and a toggle's label. Short.
        on: &'static str = "On",
        off: &'static str = "Off",
        /// A page: nav item (about 170 DIPs beside its status) and page title.
        general: &'static str = "General",
        /// A page (not in the design): the opt-in data sources.
        advanced: &'static str = "Advanced",
        /// Page subtitles, one line under the page title.
        general_sub: &'static str = "Startup, placement, refresh rate and theme",
        cpu_sub: &'static str = "Processor usage",
        gpu_sub: &'static str = "Graphics processor usage",
        memory_sub: &'static str = "Memory in use",
        disk_sub: &'static str = "Drive activity and free space",
        network_sub: &'static str = "Transfer rates",
        battery_sub: &'static str = "Charge and time remaining",
        sensors_sub: &'static str = "Temperatures, fans and power",
        processes_sub: &'static str = "The busiest programs, listed in flyouts",
        advanced_sub: &'static str = "Optional data sources, each off until you turn it on",
        /// The nav header's line under "busy" (about 170 DIPs): how many readings the taskbar shows.
        readings: Plural = Plural {
            one: "{0} reading on the taskbar",
            few: "{0} readings on the taskbar",
            other: "{0} readings on the taskbar",
        },
        /// The page title while searching.
        search_results: &'static str = "Search results",
        /// Its subtitle when nothing matches; {0} is what was typed.
        no_matches: &'static str = "No settings match \u{201c}{0}\u{201d}",
        /// Its subtitle otherwise: {0} is the number of settings found (1 or more), {1} what was typed.
        matches: Plural = Plural {
            one: "{0} setting matches \u{201c}{1}\u{201d}",
            few: "{0} settings match \u{201c}{1}\u{201d}",
            other: "{0} settings match \u{201c}{1}\u{201d}",
        },
        /// General, above everything when the settings file came from a newer busy, which is never saved over.
        newer: &'static str =
            "These settings were saved by a newer version of busy. Changes apply now, but aren\u{2019}t saved.",

        // Section headers.
        behavior: &'static str = "Behavior",
        appearance: &'static str = "Appearance",
        taskbar_order: &'static str = "Taskbar order",
        about: &'static str = "About",
        taskbar: &'static str = "Taskbar",
        color: &'static str = "Color",
        updates: &'static str = "Updates",
        flyouts: &'static str = "Flyouts",
        opt_in_sources: &'static str = "Opt-in sources",

        /// The Taskbar order card's first line, one line; the vertical form is for a taskbar on a side edge.
        order_note: &'static str = "Widgets appear left to right in this order.",
        order_note_vertical: &'static str = "Widgets appear top to bottom in this order.",
        /// A module's state in the Taskbar order card, between its name and its ↑/↓ buttons. Short.
        shown: &'static str = "Shown",
        hidden: &'static str = "Hidden",
        /// A module page's preview card: its heading, left, and its state, right, on one line.
        preview: &'static str = "Preview",
        live: &'static str = "Live",
        /// The state when the module isn't on the taskbar, quoting the "Show on taskbar" row's title.
        preview_hidden: &'static str = "Hidden \u{2014} turn on \u{201c}Show on taskbar\u{201d}",

        // General's rows.
        autostart: &'static str = "Start with Windows",
        autostart_desc: &'static str = "Launch busy when you sign in",
        position: &'static str = "Widget position",
        position_desc: &'static str = "Where readings sit on the taskbar",
        offset: &'static str = "Offset",
        offset_desc: &'static str = "Extra space between the readings and that edge",
        default_interval: &'static str = "Default update interval",
        default_interval_desc: &'static str = "Each widget can override this on its own page",
        history: &'static str = "History",
        history_desc: &'static str = "How far back the flyout charts go",
        theme: &'static str = "Theme",
        theme_desc: &'static str = "Flyouts and this window",
        setup: &'static str = "Setup",
        setup_desc: &'static str = "Walk through choosing widgets again",
        /// A button (sized to its text): turns the window into setup.
        run_setup: &'static str = "Run setup",
        version: &'static str = "Version",
        /// The version row's text: {0} is this build's version, "busy" the product's name.
        this_version: &'static str = "busy {0}",
        /// The same once the update check found a newer release: {0} this build's version, {1} the newer one.
        newer_version: &'static str = "busy {0} \u{2014} {1} is available",
        /// A button (sized to its text): opens the releases page on GitHub.
        releases: &'static str = "Releases",

        // A module's rows.
        show_on_taskbar: &'static str = "Show on taskbar",
        /// "Show on taskbar"'s description, one per module page.
        show_cpu: &'static str = "Display the CPU reading on the taskbar",
        show_gpu: &'static str = "Display the GPU reading on the taskbar",
        show_memory: &'static str = "Display the memory reading on the taskbar",
        show_disk: &'static str = "Display the disk reading on the taskbar",
        show_network: &'static str = "Display the network reading on the taskbar",
        show_battery: &'static str = "Display the battery reading on the taskbar",
        show_sensors: &'static str = "Display the sensors reading on the taskbar",
        style: &'static str = "Style",
        style_desc: &'static str = "How the reading is drawn",
        show_label: &'static str = "Show label",
        show_label_desc: &'static str = "Small caption above the value",
        /// CPU: what the Bar style's bars stand for.
        cpu_bar: &'static str = "Bar shows",
        cpu_bar_desc: &'static str = "Used by the Bar style",
        drive: &'static str = "Drive",
        drive_desc: &'static str = "Used by the Text and Bar styles",
        units: &'static str = "Units",
        units_desc: &'static str = "Transfer rate units",
        interface: &'static str = "Interface",
        interface_desc: &'static str = "Which adapter to measure",
        remaining: &'static str = "Show time remaining",
        remaining_desc: &'static str = "Replaces the label with hours:minutes left",
        sensor: &'static str = "Taskbar sensor",
        sensor_desc: &'static str = "Which temperature to show",
        /// Sensors: °C or °F.
        unit: &'static str = "Unit",
        unit_desc: &'static str = "Applies everywhere in busy",
        widget_color: &'static str = "Widget color",
        widget_color_desc: &'static str = "Used for graphs, bars and the flyout chart",
        by_load: &'static str = "Color by load",
        /// "Color by load"'s description on every module page but Sensors.
        by_load_usage: &'static str = "Shift green \u{2192} amber \u{2192} red as usage rises",
        /// The same on the Sensors page.
        by_load_temperature: &'static str = "Shift green \u{2192} amber \u{2192} red as temperature rises",
        interval: &'static str = "Update interval",
        interval_desc: &'static str = "How often this reading refreshes",
        top_processes: &'static str = "Top processes",
        top_processes_desc: &'static str = "List the busiest programs in the CPU, Memory, Disk and Network flyouts",

        // Advanced's rows: each opt-in's title and what it reads and risks, also repeated when turning it on.
        third_party: &'static str = "Third-party sensor tools",
        /// LibreHardwareMonitor, HWiNFO, WMI: names, not translated.
        third_party_desc: &'static str = "Reading LibreHardwareMonitor (WMI) / HWiNFO (shared memory). Reads data \
            published by another program you installed. Needed for CPU temps, fans, SSD temp/health, CPU power, \
            throttling.",
        process_network: &'static str = "Per-process network traffic",
        process_network_desc: &'static str = "Top processes by traffic in the Network flyout. Requires running \
            busy as administrator; starts a kernel event tracing session while the Network flyout is open. \
            Without it, the flyout lists processes by open connections.",
        update_check: &'static str = "Check for updates",
        update_check_desc: &'static str = "Contacts api.github.com once a day while busy runs, to see whether a \
            newer release exists; GitHub sees your IP address. Nothing is downloaded or installed.",
    }
}

impl Settings {
    /// A module page's subtitle.
    pub fn module_sub(&self, m: Module) -> &'static str {
        match m {
            Module::Cpu => self.cpu_sub,
            Module::Gpu => self.gpu_sub,
            Module::Memory => self.memory_sub,
            Module::Disk => self.disk_sub,
            Module::Network => self.network_sub,
            Module::Battery => self.battery_sub,
            Module::Sensors => self.sensors_sub,
            Module::Processes => self.processes_sub,
        }
    }

    /// "Show on taskbar"'s description on a module's page; None for Processes, which has no cell.
    pub fn show_desc(&self, m: Module) -> Option<&'static str> {
        Some(match m {
            Module::Cpu => self.show_cpu,
            Module::Gpu => self.show_gpu,
            Module::Memory => self.show_memory,
            Module::Disk => self.show_disk,
            Module::Network => self.show_network,
            Module::Battery => self.show_battery,
            Module::Sensors => self.show_sensors,
            Module::Processes => return None,
        })
    }
}
