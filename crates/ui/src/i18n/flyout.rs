//! The flyout: a module's details, opened by clicking its taskbar cell.

use super::Plural;

section! {
    /// The flyout, 360 DIPs wide. Stat keys ("Speed", "Threads", …) sit in a two-column grid, ~156 DIPs per
    /// column at 11 px, one line, cut off with "…" when too long: keep them short.
    Flyout {
        /// Heading of the process list.
        top_processes: &'static str = "Top processes",
        /// Heading of the process list when the network's processes are ranked by connection count.
        open_connections: &'static str = "Open connections",
        waiting: &'static str = "Waiting for data…",
        /// Under the chart, left: the time it spans.
        span_seconds: Plural = Plural { one: "Last {0} second", few: "Last {0} seconds", other: "Last {0} seconds" },
        span_minutes: Plural = Plural { one: "Last {0} minute", few: "Last {0} minutes", other: "Last {0} minutes" },
        /// Under the chart, right: its scale, {0} the highest rate ("3.2 MB/s").
        peak: &'static str = "Peak {0}",
        /// Readout over the chart under the pointer: {0} the values there ("42%  8%"), {1} how long ago the sample
        /// was taken ("5s", "2m").
        ago: &'static str = "{0}  · {1} ago",
        /// Over the CPU's grid of per-processor bars.
        logical_processors: &'static str = "Logical processors",
        /// Footer link.
        open_task_manager: &'static str = "Open Task Manager",
        /// Footer button: {0} the module's name ("CPU settings").
        module_settings: &'static str = "{0} settings",
        /// Headline label of CPU and GPU, next to the percentage.
        utilization: &'static str = "Utilization",
        temperature: &'static str = "Temperature",

        /// Line under the title: {0} the processor's name, {1} `cores`, {2} `threads`.
        cpu_sub: &'static str = "{0} · {1}, {2}",
        cores: Plural = Plural { one: "{0} core", few: "{0} cores", other: "{0} cores" },
        threads: Plural = Plural { one: "{0} thread", few: "{0} threads", other: "{0} threads" },
        /// CPU time split, a legend under the chart: kernel mode.
        cpu_system: &'static str = "System",
        /// User mode.
        cpu_user: &'static str = "User",
        cpu_idle: &'static str = "Idle",
        speed: &'static str = "Speed",
        /// Stat: how many processes run.
        process_count: &'static str = "Processes",
        /// Stat: how many threads run.
        thread_count: &'static str = "Threads",
        /// Stat: how many kernel handles are open.
        handle_count: &'static str = "Handles",
        /// Stat: time since Windows started.
        up_time: &'static str = "Up time",

        /// Line under the title: {0} the RAM installed ("32 GB").
        mem_installed: &'static str = "{0} installed",
        /// Line under the title when the installed RAM is unknown: {0} the RAM Windows can use.
        mem_usable: &'static str = "{0} usable",
        /// Right of the title, after the headline value drawn on its own in a large font ("12.4 GB"): {0} the
        /// RAM Windows can use.
        mem_of_in_use: &'static str = "of {0} in use",
        /// Memory composition, a legend under the bar (Task Manager's words).
        mem_in_use: &'static str = "In use",
        mem_modified: &'static str = "Modified",
        mem_standby: &'static str = "Standby",
        mem_free: &'static str = "Free",
        mem_available: &'static str = "Available",
        mem_committed: &'static str = "Committed",
        mem_compressed: &'static str = "Compressed",
        mem_paged_pool: &'static str = "Paged pool",
        mem_nonpaged_pool: &'static str = "Non-paged pool",
        mem_hardware_reserved: &'static str = "Hardware reserved",
        mem_cached: &'static str = "Cached",

        /// Line under the title: {0} the first disk's name, {1} how many other disks there are.
        disk_more: &'static str = "{0} and {1} more",
        /// Headline label: the share of time the busiest disk was busy.
        disk_active_time: &'static str = "Active time",
        disk_read: &'static str = "Read",
        disk_write: &'static str = "Write",
        /// Heading of the volumes' fill bars.
        disk_volumes: &'static str = "Volumes",
        /// Right of a volume's bar: {0} the free space, {1} the volume's size.
        disk_free_of: &'static str = "{0} free of {1}",
        /// Stat: the slowest disk's average response time.
        disk_avg_response: &'static str = "Avg. response",
        disk_read_total: &'static str = "Read since start",
        disk_written_total: &'static str = "Written since start",

        /// Line under the title with more than one GPU: {0} the shown GPU's name, {1} how many GPUs there are.
        gpu_busiest_of: &'static str = "{0} · busiest of {1}",
        gpu_dedicated_memory: &'static str = "Dedicated memory",
        gpu_shared_memory: &'static str = "Shared memory",
        /// Stat: the hottest point of the GPU die.
        gpu_hot_spot: &'static str = "Hot spot",
        gpu_power: &'static str = "Power",
        gpu_fan: &'static str = "Fan",
        gpu_core_clock: &'static str = "Core clock",
        gpu_memory_clock: &'static str = "Memory clock",
        /// Stat: the graphics driver's version.
        gpu_driver: &'static str = "Driver",

        /// The kind of network connection, under the title ("Wi‑Fi · HomeNet") and in the Interface stat.
        net_wifi: &'static str = "Wi‑Fi",
        net_ethernet: &'static str = "Ethernet",
        net_download: &'static str = "Download",
        net_upload: &'static str = "Upload",
        /// Stat: the connection's kind and band or link speed ("Wi‑Fi · 5 GHz").
        net_interface: &'static str = "Interface",
        net_signal: &'static str = "Signal",
        /// The Signal stat of a cable connection.
        net_wired: &'static str = "Wired",
        /// Stat: bytes received since Windows started.
        net_received: &'static str = "Received",
        net_sent: &'static str = "Sent",

        bat_internal: &'static str = "Internal battery",
        /// Line under the title: {0} the capacity the battery was designed for ("52.0 Wh").
        bat_internal_design: &'static str = "Internal battery · {0} design",
        bat_charging: &'static str = "Charging",
        /// On AC power, not charging.
        bat_plugged_in: &'static str = "Plugged in",
        bat_on_battery: &'static str = "On battery",
        /// Headline label: {0} the time left ("2 h 15 min").
        bat_remaining: &'static str = "{0} remaining",
        bat_state: &'static str = "State",
        bat_charge_rate: &'static str = "Charge rate",
        bat_power_draw: &'static str = "Power draw",
        /// Stat: full-charge capacity as a share of the design capacity.
        bat_health: &'static str = "Health",
        bat_cycle_count: &'static str = "Cycle count",
        bat_design_capacity: &'static str = "Design capacity",
        /// Stat: the capacity the battery holds when full today.
        bat_full_charge: &'static str = "Full charge",

        /// Line under the Sensors title.
        sens_sub: &'static str = "Temperatures, fans and power",
        /// Heading of the temperatures' bars.
        sens_temperatures: &'static str = "Temperatures",
        /// A paragraph in place of the readings; LibreHardwareMonitor and HWiNFO are programs' names.
        sens_none: &'static str = "No sensors available. Run LibreHardwareMonitor or HWiNFO (with shared memory \
            enabled) for CPU temperatures.",
        /// As `sens_none`, while reading those programs is turned off; `opt_in.third_party_sensors` and
        /// `config.json` are a setting's and a file's names, kept as they are.
        sens_none_off: &'static str = "No sensors available. Reading LibreHardwareMonitor / HWiNFO is off; enable \
            opt_in.third_party_sensors in config.json for CPU temperatures.",
    }
}
