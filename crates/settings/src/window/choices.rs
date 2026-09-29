//! The options of each dropdown (design `sel()` lists), each with the value it sets. A current value that isn't
//! among the design's options (a hand-edited file, a drive that is gone) is kept as an extra option, so opening
//! the window never changes a setting by itself.

use busy_core::{Anchor, CpuBar, Module, NetInterface, RateUnit, SensorPick, ThemeMode};

/// A value a dropdown option sets.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Pick {
    Anchor(Anchor),
    Offset(i32),
    IntervalMs(u32),
    HistorySecs(u32),
    Theme(ThemeMode),
    CpuBar(CpuBar),
    Drive(Option<String>),
    Units(RateUnit),
    Interface(NetInterface),
    Sensor(SensorPick),
    /// A module's own interval in seconds; `None` = the default.
    ModuleInterval(Module, Option<u32>),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Opt {
    pub(super) label: String,
    pub(super) pick: Pick,
}

/// What the machine has, for the options that list it: drives, adapters, sensors. Empty until the window has
/// readings.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Choices {
    /// (mount, label), e.g. ("D:", "Backup").
    pub(super) volumes: Vec<(String, String)>,
    /// Adapter aliases (`NetIf::name`).
    pub(super) adapters: Vec<String>,
    /// Temperature readings as "hardware/name", the form `SensorPick::Named` matches.
    pub(super) sensors: Vec<String>,
}

/// `opts` as (label, pick) with `current` selected; appended as `extra(current)` when not among them.
fn list(opts: Vec<(String, Pick)>, current: Pick, extra: impl FnOnce(&Pick) -> String) -> (Vec<Opt>, usize) {
    let mut opts: Vec<Opt> = opts.into_iter().map(|(label, pick)| Opt { label, pick }).collect();
    let i = match opts.iter().position(|o| o.pick == current) {
        Some(i) => i,
        None => {
            opts.push(Opt { label: extra(&current), pick: current });
            opts.len() - 1
        }
    };
    (opts, i)
}

fn fixed(opts: &[(&str, Pick)], current: Pick) -> (Vec<Opt>, usize) {
    list(opts.iter().map(|(l, p)| (l.to_string(), p.clone())).collect(), current, |_| String::new())
}

pub(super) fn anchor(a: Anchor) -> (Vec<Opt>, usize) {
    fixed(
        &[
            ("Next to system tray", Pick::Anchor(Anchor::NearTray)),
            ("Left edge of taskbar", Pick::Anchor(Anchor::Left)),
        ],
        Pick::Anchor(a),
    )
}

fn px(v: i32) -> String {
    if v == 0 { "None".into() } else { format!("{v} px") }
}

pub(super) fn offset(v: i32) -> (Vec<Opt>, usize) {
    let opts = [0, 8, 16, 32, 64, 128].map(|v| (px(v), Pick::Offset(v))).to_vec();
    list(opts, Pick::Offset(v), |_| px(v))
}

fn every(secs: f64) -> String {
    if secs == 1.0 { "Every second".into() } else { format!("Every {secs} seconds") }
}

pub(super) fn interval(ms: u32) -> (Vec<Opt>, usize) {
    let opts = [1000, 2000, 5000].map(|v| (every(v as f64 / 1000.0), Pick::IntervalMs(v))).to_vec();
    list(opts, Pick::IntervalMs(ms), |_| every(ms as f64 / 1000.0))
}

fn minutes(secs: u32) -> String {
    match secs {
        60 => "1 minute".into(),
        s if s.is_multiple_of(60) => format!("{} minutes", s / 60),
        s => format!("{s} seconds"),
    }
}

pub(super) fn history(secs: u32) -> (Vec<Opt>, usize) {
    let opts = [60, 120, 300, 600].map(|v| (minutes(v), Pick::HistorySecs(v))).to_vec();
    list(opts, Pick::HistorySecs(secs), |_| minutes(secs))
}

pub(super) fn theme(t: ThemeMode) -> (Vec<Opt>, usize) {
    fixed(
        &[
            ("Use system setting", Pick::Theme(ThemeMode::System)),
            ("Dark", Pick::Theme(ThemeMode::Dark)),
            ("Light", Pick::Theme(ThemeMode::Light)),
        ],
        Pick::Theme(t),
    )
}

pub(super) fn cpu_bar(b: CpuBar) -> (Vec<Opt>, usize) {
    fixed(&[("Each core", Pick::CpuBar(CpuBar::Cores)), ("Total", Pick::CpuBar(CpuBar::Total))], Pick::CpuBar(b))
}

fn volume_label((mount, label): &(String, String)) -> String {
    if label.is_empty() { mount.clone() } else { format!("{label} ({mount})") }
}

pub(super) fn drive(current: Option<&str>, ch: &Choices) -> (Vec<Opt>, usize) {
    let mut opts = vec![("System drive".to_string(), Pick::Drive(None))];
    opts.extend(ch.volumes.iter().map(|v| (volume_label(v), Pick::Drive(Some(v.0.clone())))));
    let current = current.map(str::to_string);
    list(opts, Pick::Drive(current.clone()), |_| current.unwrap_or_default())
}

pub(super) fn units(u: RateUnit) -> (Vec<Opt>, usize) {
    fixed(
        &[("Bytes (MB/s)", Pick::Units(RateUnit::Bytes)), ("Bits (Mb/s)", Pick::Units(RateUnit::Bits))],
        Pick::Units(u),
    )
}

pub(super) fn interface(current: &NetInterface, ch: &Choices) -> (Vec<Opt>, usize) {
    let mut opts = vec![
        ("Automatic".to_string(), Pick::Interface(NetInterface::Auto)),
        ("Wi\u{2011}Fi".to_string(), Pick::Interface(NetInterface::WiFi)),
        ("Ethernet".to_string(), Pick::Interface(NetInterface::Ethernet)),
    ];
    opts.extend(ch.adapters.iter().map(|a| (a.clone(), Pick::Interface(NetInterface::Named(a.clone())))));
    let name = match current {
        NetInterface::Named(n) => n.clone(),
        _ => String::new(),
    };
    list(opts, Pick::Interface(current.clone()), |_| name)
}

/// "hardware/name" as "hardware · name".
fn sensor_label(key: &str) -> String {
    key.replacen('/', " \u{b7} ", 1)
}

pub(super) fn sensor(current: &SensorPick, ch: &Choices) -> (Vec<Opt>, usize) {
    let mut opts = vec![
        ("CPU package".to_string(), Pick::Sensor(SensorPick::Cpu)),
        ("GPU".to_string(), Pick::Sensor(SensorPick::Gpu)),
        ("Drive".to_string(), Pick::Sensor(SensorPick::Storage)),
    ];
    opts.extend(ch.sensors.iter().map(|s| (sensor_label(s), Pick::Sensor(SensorPick::Named(s.clone())))));
    let name = match current {
        SensorPick::Named(n) => sensor_label(n),
        _ => String::new(),
    };
    list(opts, Pick::Sensor(current.clone()), |_| name)
}

/// A module's own interval; the first option follows the default (design "Default (1 second)").
pub(super) fn module_interval(m: Module, secs: Option<u32>, default_ms: u32) -> (Vec<Opt>, usize) {
    let d = default_ms as f64 / 1000.0;
    let default = if d == 1.0 { "Default (1 second)".to_string() } else { format!("Default ({d} seconds)") };
    let mut opts = vec![(default, Pick::ModuleInterval(m, None))];
    opts.extend([1, 2, 5].map(|s| (every(s as f64), Pick::ModuleInterval(m, Some(s)))));
    list(opts, Pick::ModuleInterval(m, secs), |_| every(secs.unwrap_or(1) as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels((opts, i): (Vec<Opt>, usize)) -> (Vec<String>, usize) {
        (opts.into_iter().map(|o| o.label).collect(), i)
    }

    #[test]
    fn design_options() {
        assert_eq!(
            labels(interval(2000)),
            (vec!["Every second".into(), "Every 2 seconds".into(), "Every 5 seconds".into()], 1)
        );
        assert_eq!(labels(theme(ThemeMode::Light)).1, 2);
        assert_eq!(
            labels(anchor(Anchor::Left)),
            (vec!["Next to system tray".into(), "Left edge of taskbar".into()], 1)
        );
        let (opts, i) = module_interval(Module::Cpu, None, 5000);
        assert_eq!((opts[0].label.as_str(), i, opts.len()), ("Default (5 seconds)", 0, 4));
        assert_eq!(module_interval(Module::Cpu, Some(2), 1000).1, 2);
    }

    #[test]
    fn values_off_the_list_are_kept() {
        assert_eq!(
            labels(interval(500)),
            (
                vec![
                    "Every second".into(),
                    "Every 2 seconds".into(),
                    "Every 5 seconds".into(),
                    "Every 0.5 seconds".into()
                ],
                3
            )
        );
        assert_eq!(labels(history(90)).0.last().map(String::as_str), Some("90 seconds"));
        assert_eq!(labels(offset(-24)).0.last().map(String::as_str), Some("-24 px"));
        let (opts, i) = module_interval(Module::Gpu, Some(10), 1000);
        assert_eq!(
            (opts[i].label.as_str(), opts[i].pick.clone()),
            ("Every 10 seconds", Pick::ModuleInterval(Module::Gpu, Some(10)))
        );
    }

    #[test]
    fn machine_lists() {
        let ch = Choices {
            volumes: vec![("C:".into(), "Windows".into()), ("E:".into(), String::new())],
            adapters: vec!["Wi-Fi 2".into()],
            sensors: vec!["GPU/Hot Spot".into()],
        };
        assert_eq!(
            labels(drive(Some("E:"), &ch)),
            (vec!["System drive".into(), "Windows (C:)".into(), "E:".into()], 2)
        );
        // A drive that is gone stays selectable until another is picked.
        assert_eq!(labels(drive(Some("Z:"), &ch)).0.last().map(String::as_str), Some("Z:"));
        let (opts, i) = interface(&NetInterface::Named("Wi-Fi 2".into()), &ch);
        assert_eq!((opts.len(), i), (4, 3));
        let (opts, i) = sensor(&SensorPick::Named("CPU/Core Max".into()), &ch);
        assert_eq!((opts[3].label.as_str(), opts[i].label.as_str()), ("GPU \u{b7} Hot Spot", "CPU \u{b7} Core Max"));
        assert_eq!(sensor(&SensorPick::Storage, &Choices::default()).1, 2);
    }
}
