//! The taskbar cell's labels, keys and tooltip.

section! {
    /// The taskbar cell (`cell/`): the caption above a module's value, the keys of a disk's read/write rows, and
    /// the tooltip.
    Cell {
        /// Taskbar cell captions, drawn in 9 px letters above the value: at most 4–5 characters, uppercase in
        /// Latin and Cyrillic scripts. The cell is as wide as its caption or value, whichever is wider.
        cpu: &'static str = "CPU",
        /// Caption of the memory cell.
        ram: &'static str = "RAM",
        gpu: &'static str = "GPU",
        /// Caption of the network cell.
        net: &'static str = "NET",
        /// Caption of the disks' read/write cell.
        disk: &'static str = "DISK",
        /// Caption of the battery cell.
        bat: &'static str = "BAT",
        /// Caption of the sensor cell showing a temperature.
        temp: &'static str = "TEMP",
        /// Key of the disks' read-rate row, drawn bold before the rate: one character if at all possible.
        read: &'static str = "R",
        /// Key of the disks' write-rate row, as `read`.
        write: &'static str = "W",
        /// Tooltip of a cell: `{0}` the module's name ("Memory"), `{1}` its reading ("35%").
        tip: &'static str = "{0}: {1}",
        /// Tooltip of a two-row cell: `{0}` the module's name, then each row's key and rate: `{1}` "↑" or the
        /// read key, `{2}` its rate ("24 KB/s"), `{3}` "↓" or the write key, `{4}` its rate.
        tip_rows: &'static str = "{0}: {1} {2}  {3} {4}",
    }
}
