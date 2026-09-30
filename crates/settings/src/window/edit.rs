//! Applying an `Edit` to the config. Pure; the window normalizes the result and applies it live.

use super::choices::Pick;
use super::model::{Edit, Flag};
use busy_core::{Config, Module, ModuleCfg};

fn module(cfg: &mut Config, m: Module) -> Option<&mut ModuleCfg> {
    cfg.modules.iter_mut().find(|c| c.module == m)
}

pub(super) fn apply(cfg: &mut Config, e: &Edit) {
    match e {
        Edit::Flag(f, on) => {
            let on = *on;
            match *f {
                Flag::Autostart => cfg.autostart = on,
                Flag::Taskbar(m) => module(cfg, m).into_iter().for_each(|c| c.taskbar = on),
                Flag::Label(m) => module(cfg, m).into_iter().for_each(|c| c.show_label = on),
                Flag::ByLoad(m) => module(cfg, m).into_iter().for_each(|c| c.color_by_load = on),
                Flag::Remaining => cfg.options.battery.show_remaining = on,
                Flag::TopProcesses => module(cfg, Module::Processes).into_iter().for_each(|c| c.flyout = on),
                Flag::ThirdPartySensors => cfg.opt_in.third_party_sensors = on,
                Flag::UpdateCheck => cfg.opt_in.update_check = on,
            }
        }
        Edit::Pick(p) => match p.clone() {
            Pick::Anchor(a) => cfg.anchor = a,
            Pick::Offset(v) => cfg.offset_px = v,
            Pick::IntervalMs(v) => cfg.interval_ms = v,
            Pick::HistorySecs(v) => cfg.history_secs = v,
            Pick::Theme(t) => cfg.theme = t,
            Pick::CpuBar(b) => cfg.options.cpu.bar = b,
            Pick::Drive(d) => cfg.options.disk.drive = d,
            Pick::Units(u) => cfg.options.network.units = u,
            Pick::Interface(i) => cfg.options.network.interface = i,
            Pick::Sensor(s) => cfg.options.sensors.sensor = s,
            Pick::ModuleInterval(m, s) => module(cfg, m).into_iter().for_each(|c| c.interval_s = s),
            Pick::Style(m, s) => module(cfg, m).into_iter().for_each(|c| c.style = s),
            Pick::TempUnit(u) => cfg.temp_unit = u,
        },
        Edit::Color(m, i) => module(cfg, *m).into_iter().for_each(|c| c.color = Some(*i)),
        Edit::Move(m, up) => move_module(cfg, *m, *up),
    }
}

/// Swaps `m` with its neighbour in the taskbar order; Processes has no cell, so it is stepped over.
fn move_module(cfg: &mut Config, m: Module, up: bool) {
    let listed = |c: &ModuleCfg| c.module != Module::Processes;
    let Some(i) = cfg.modules.iter().position(|c| c.module == m) else { return };
    let j = if up {
        cfg.modules[..i].iter().rposition(listed)
    } else {
        cfg.modules.iter().skip(i + 1).position(listed).map(|k| i + 1 + k)
    };
    if let Some(j) = j {
        cfg.modules.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use busy_core::{Anchor, CellStyle, NetInterface, SensorPick, TempUnit};

    fn order(cfg: &Config) -> Vec<Module> {
        cfg.modules.iter().map(|c| c.module).collect()
    }

    #[test]
    fn flags_and_picks() {
        let mut cfg = Config::default();
        apply(&mut cfg, &Edit::Flag(Flag::Taskbar(Module::Disk), true));
        apply(&mut cfg, &Edit::Flag(Flag::Label(Module::Cpu), false));
        apply(&mut cfg, &Edit::Flag(Flag::TopProcesses, false));
        apply(&mut cfg, &Edit::Flag(Flag::ThirdPartySensors, true));
        apply(&mut cfg, &Edit::Pick(Pick::Anchor(Anchor::Left)));
        apply(&mut cfg, &Edit::Pick(Pick::Interface(NetInterface::Named("Wi-Fi".into()))));
        apply(&mut cfg, &Edit::Pick(Pick::Sensor(SensorPick::Gpu)));
        apply(&mut cfg, &Edit::Pick(Pick::ModuleInterval(Module::Gpu, Some(5))));
        apply(&mut cfg, &Edit::Pick(Pick::Style(Module::Memory, CellStyle::Graph)));
        apply(&mut cfg, &Edit::Pick(Pick::TempUnit(TempUnit::Fahrenheit)));
        apply(&mut cfg, &Edit::Color(Module::Cpu, 4));
        assert!(cfg.module(Module::Disk).unwrap().taskbar);
        assert!(!cfg.module(Module::Cpu).unwrap().show_label);
        assert!(!cfg.module(Module::Processes).unwrap().flyout);
        assert!(cfg.opt_in.third_party_sensors);
        assert_eq!(cfg.anchor, Anchor::Left);
        assert_eq!(cfg.options.network.interface, NetInterface::Named("Wi-Fi".into()));
        assert_eq!(cfg.options.sensors.sensor, SensorPick::Gpu);
        assert_eq!(cfg.module(Module::Gpu).unwrap().interval_s, Some(5));
        assert_eq!(cfg.module(Module::Memory).unwrap().style, CellStyle::Graph);
        assert_eq!(cfg.temp_unit, TempUnit::Fahrenheit);
        assert_eq!(cfg.module(Module::Cpu).unwrap().color_index(), 4);
    }

    #[test]
    fn moves_step_over_processes_and_stop_at_the_ends() {
        use Module::*;
        let mut cfg = Config::default();
        cfg.modules.swap(6, 7); // … Sensors, Processes, Battery
        assert_eq!(order(&cfg), [Cpu, Memory, Gpu, Network, Disk, Sensors, Processes, Battery]);
        apply(&mut cfg, &Edit::Move(Battery, true));
        assert_eq!(order(&cfg), [Cpu, Memory, Gpu, Network, Disk, Battery, Processes, Sensors]);
        apply(&mut cfg, &Edit::Move(Battery, false));
        assert_eq!(order(&cfg), [Cpu, Memory, Gpu, Network, Disk, Sensors, Processes, Battery]);
        let before = order(&cfg);
        apply(&mut cfg, &Edit::Move(Cpu, true));
        apply(&mut cfg, &Edit::Move(Battery, false));
        assert_eq!(order(&cfg), before);
    }
}
