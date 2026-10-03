//! The settings window's dropdown and segment options.

use super::Plural;

section! {
    /// The options the settings window's dropdowns list (one line each; the dropdown is as wide as its longest)
    /// and its segmented controls show (short: they sit side by side in a row's control column).
    Choices {
        /// Widget position: next to the notification area at the taskbar's end.
        near_tray: &'static str = "Next to system tray",
        /// Widget position: the taskbar's other end, on a taskbar at the bottom or top of the screen.
        left_edge: &'static str = "Left edge of taskbar",
        /// The same on a taskbar standing on the left or right edge of the screen, whose other end is its top.
        top: &'static str = "Top of taskbar",
        /// Offset of 0.
        no_offset: &'static str = "None",
        /// An offset; {0} is a number of pixels, "px" the unit's symbol.
        px: &'static str = "{0} px",
        /// An update interval in whole seconds; `one` may drop {0} ("Every second").
        every: Plural = Plural { one: "Every second", few: "Every {0} seconds", other: "Every {0} seconds" },
        /// The same for a fraction of seconds, such as 0.5 (from a hand-edited settings file).
        every_fraction: &'static str = "Every {0} seconds",
        /// A module's interval when it follows the default; {0} is the default in whole seconds.
        default_every: Plural = Plural {
            one: "Default ({0} second)",
            few: "Default ({0} seconds)",
            other: "Default ({0} seconds)",
        },
        /// The same for a default in a fraction of seconds, such as 0.5.
        default_every_fraction: &'static str = "Default ({0} seconds)",
        /// How far back the flyout charts go, in minutes.
        minutes: Plural = Plural { one: "{0} minute", few: "{0} minutes", other: "{0} minutes" },
        /// The same in seconds, when it isn't whole minutes (from a hand-edited settings file).
        seconds: Plural = Plural { one: "{0} second", few: "{0} seconds", other: "{0} seconds" },
        /// Theme: follow Windows' app mode.
        theme_system: &'static str = "Use system setting",
        dark: &'static str = "Dark",
        light: &'static str = "Light",
        /// What CPU's bars stand for: one bar per core, or one for the whole processor.
        each_core: &'static str = "Each core",
        total: &'static str = "Total",
        /// Disk: the drive Windows is on; the machine's other drives follow by name.
        system_drive: &'static str = "System drive",
        /// A drive: {0} is its label, {1} its letter ("Windows (C:)").
        volume: &'static str = "{0} ({1})",
        /// Network's units; MB/s and Mb/s are unit symbols.
        bytes: &'static str = "Bytes (MB/s)",
        bits: &'static str = "Bits (Mb/s)",
        /// Network's interface: all connected adapters together, or every adapter of a kind; the machine's
        /// adapters follow by name.
        automatic: &'static str = "Automatic",
        wifi: &'static str = "Wi\u{2011}Fi",
        ethernet: &'static str = "Ethernet",
        /// Sensors' taskbar reading: the CPU's package temperature, the GPU's, a drive's.
        cpu_package: &'static str = "CPU package",
        gpu: &'static str = "GPU",
        drive: &'static str = "Drive",
        /// Cell styles (segments): a number, a graph over time, a bar.
        text: &'static str = "Text",
        graph: &'static str = "Graph",
        bar: &'static str = "Bar",
        /// Disk's two-line style: read and write rates.
        read_write: &'static str = "Read / write",
        /// Network's two-line style: upload and download rates.
        up_down: &'static str = "Up / down",
    }
}
