//! Words more than one window shows.

use busy_core::Module;

section! {
    /// Words more than one window shows: the modules' names (flyout titles, settings pages, the menu, tooltips).
    Common {
        cpu: &'static str = "CPU",
        memory: &'static str = "Memory",
        disk: &'static str = "Disk",
        network: &'static str = "Network",
        gpu: &'static str = "GPU",
        battery: &'static str = "Battery",
        sensors: &'static str = "Sensors",
        processes: &'static str = "Processes",
    }
}

impl Common {
    /// A module's name.
    pub fn module(&self, m: Module) -> &'static str {
        match m {
            Module::Cpu => self.cpu,
            Module::Memory => self.memory,
            Module::Disk => self.disk,
            Module::Network => self.network,
            Module::Gpu => self.gpu,
            Module::Battery => self.battery,
            Module::Sensors => self.sensors,
            Module::Processes => self.processes,
        }
    }
}
