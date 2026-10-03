//! The options of each dropdown (design `sel()` lists), each with the value it sets. A current value that isn't
//! among the design's options (a hand-edited file, a drive that is gone) is kept as an extra option, so opening
//! the window never changes a setting by itself.

use busy_core::release::Release;
use busy_core::{
    Anchor, CellStyle, CpuBar, Lang, Module, NetInterface, RateUnit, SensorKind, SensorPick, Snapshot, TempUnit,
    ThemeMode,
};
use busy_ui::i18n::{self, Plural, t};
use busy_win::Edge;

/// A value a dropdown option sets.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Pick {
    Anchor(Anchor),
    Offset(i32),
    IntervalMs(u32),
    HistorySecs(u32),
    Theme(ThemeMode),
    /// `None` = Windows' display language.
    Language(Option<Lang>),
    CpuBar(CpuBar),
    Drive(Option<String>),
    Units(RateUnit),
    Interface(NetInterface),
    Sensor(SensorPick),
    /// A module's own interval in seconds; `None` = the default.
    ModuleInterval(Module, Option<u32>),
    Style(Module, CellStyle),
    TempUnit(TempUnit),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Opt {
    pub(super) label: String,
    pub(super) pick: Pick,
}

/// What the machine has, for the options that list it: drives, adapters, sensors. Empty until the window has
/// readings. Also a newer release of busy, when the host's update check found one (`crate::release`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Choices {
    /// (mount, label), e.g. ("D:", "Backup").
    pub(super) volumes: Vec<(String, String)>,
    /// Adapter aliases (`NetIf::name`).
    pub(super) adapters: Vec<String>,
    /// Temperature readings as "hardware/name", the form `SensorPick::Named` matches.
    pub(super) sensors: Vec<String>,
    /// Shown in General's About row; readings leave it alone.
    pub(super) newer: Option<Release>,
    /// The taskbar's screen edge (`busy_win::taskbar_edge`), which words and draws what is "left" and the
    /// preview; readings leave it alone too.
    pub(super) edge: Edge,
    /// Installed from the Microsoft Store, which updates busy: About has no Releases button and Advanced no
    /// update check.
    pub(super) packaged: bool,
}

impl Choices {
    /// The lists in `snap`, in its order, without repeats: every volume, every adapter, every temperature.
    pub(super) fn from_snapshot(snap: &Snapshot) -> Self {
        fn unique(it: impl Iterator<Item = String>) -> Vec<String> {
            let mut v: Vec<String> = Vec::new();
            for s in it {
                if !v.contains(&s) {
                    v.push(s);
                }
            }
            v
        }
        Self {
            volumes: snap.volumes.iter().map(|v| (v.mount.clone(), v.label.clone())).collect(),
            adapters: unique(snap.net.iter().flat_map(|n| &n.interfaces).map(|i| i.name.clone())),
            sensors: unique(
                snap.sensors
                    .iter()
                    .filter(|s| s.kind == SensorKind::Temperature)
                    .map(|s| format!("{}/{}", s.hardware, s.name)),
            ),
            newer: None,
            edge: Edge::default(),
            packaged: false,
        }
    }
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

/// Down a vertical taskbar the far end from the tray is its top (design `isVert()`).
pub(super) fn anchor(a: Anchor, vertical: bool) -> (Vec<Opt>, usize) {
    let c = &t().choices;
    let left = if vertical { c.top } else { c.left_edge };
    fixed(&[(c.near_tray, Pick::Anchor(Anchor::NearTray)), (left, Pick::Anchor(Anchor::Left))], Pick::Anchor(a))
}

fn px(v: i32) -> String {
    let c = &t().choices;
    if v == 0 { c.no_offset.into() } else { i18n::fill(c.px, &[&v]) }
}

pub(super) fn offset(v: i32) -> (Vec<Opt>, usize) {
    let opts = [0, 8, 16, 32, 64, 128].map(|v| (px(v), Pick::Offset(v))).to_vec();
    list(opts, Pick::Offset(v), |_| px(v))
}

/// `secs` in `whole`'s form for it when a whole number, else in `fraction`.
fn in_seconds(whole: &Plural, fraction: &str, secs: f64) -> String {
    if secs >= 0.0 && secs.fract() == 0.0 { i18n::plural(whole, secs as u64) } else { i18n::fill(fraction, &[&secs]) }
}

fn every(secs: f64) -> String {
    let c = &t().choices;
    in_seconds(&c.every, c.every_fraction, secs)
}

pub(super) fn interval(ms: u32) -> (Vec<Opt>, usize) {
    let opts = [1000, 2000, 5000].map(|v| (every(v as f64 / 1000.0), Pick::IntervalMs(v))).to_vec();
    list(opts, Pick::IntervalMs(ms), |_| every(ms as f64 / 1000.0))
}

fn minutes(secs: u32) -> String {
    let c = &t().choices;
    if secs.is_multiple_of(60) {
        i18n::plural(&c.minutes, u64::from(secs / 60))
    } else {
        i18n::plural(&c.seconds, u64::from(secs))
    }
}

pub(super) fn history(secs: u32) -> (Vec<Opt>, usize) {
    let opts = [60, 120, 300, 600].map(|v| (minutes(v), Pick::HistorySecs(v))).to_vec();
    list(opts, Pick::HistorySecs(secs), |_| minutes(secs))
}

pub(super) fn theme(theme: ThemeMode) -> (Vec<Opt>, usize) {
    let c = &t().choices;
    fixed(
        &[
            (c.theme_system, Pick::Theme(ThemeMode::System)),
            (c.dark, Pick::Theme(ThemeMode::Dark)),
            (c.light, Pick::Theme(ThemeMode::Light)),
        ],
        Pick::Theme(theme),
    )
}

/// Each language under its own name, so it can be found whatever the window is shown in.
pub(super) fn language(lang: Option<Lang>) -> (Vec<Opt>, usize) {
    let system = std::iter::once((t().choices.theme_system.to_string(), Pick::Language(None)));
    let langs = Lang::ALL.map(|l| (l.native_name().to_string(), Pick::Language(Some(l))));
    list(system.chain(langs).collect(), Pick::Language(lang), |_| String::new())
}

pub(super) fn cpu_bar(b: CpuBar) -> (Vec<Opt>, usize) {
    let c = &t().choices;
    fixed(&[(c.each_core, Pick::CpuBar(CpuBar::Cores)), (c.total, Pick::CpuBar(CpuBar::Total))], Pick::CpuBar(b))
}

fn volume_label((mount, label): &(String, String)) -> String {
    if label.is_empty() { mount.clone() } else { i18n::fill(t().choices.volume, &[label, mount]) }
}

pub(super) fn drive(current: Option<&str>, ch: &Choices) -> (Vec<Opt>, usize) {
    let mut opts = vec![(t().choices.system_drive.to_string(), Pick::Drive(None))];
    opts.extend(ch.volumes.iter().map(|v| (volume_label(v), Pick::Drive(Some(v.0.clone())))));
    let current = current.map(str::to_string);
    list(opts, Pick::Drive(current.clone()), |_| current.unwrap_or_default())
}

pub(super) fn units(u: RateUnit) -> (Vec<Opt>, usize) {
    let c = &t().choices;
    fixed(&[(c.bytes, Pick::Units(RateUnit::Bytes)), (c.bits, Pick::Units(RateUnit::Bits))], Pick::Units(u))
}

pub(super) fn interface(current: &NetInterface, ch: &Choices) -> (Vec<Opt>, usize) {
    let c = &t().choices;
    let mut opts = vec![
        (c.automatic.to_string(), Pick::Interface(NetInterface::Auto)),
        (c.wifi.to_string(), Pick::Interface(NetInterface::WiFi)),
        (c.ethernet.to_string(), Pick::Interface(NetInterface::Ethernet)),
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
    let c = &t().choices;
    let mut opts = vec![
        (c.cpu_package.to_string(), Pick::Sensor(SensorPick::Cpu)),
        (c.gpu.to_string(), Pick::Sensor(SensorPick::Gpu)),
        (c.drive.to_string(), Pick::Sensor(SensorPick::Storage)),
    ];
    opts.extend(ch.sensors.iter().map(|s| (sensor_label(s), Pick::Sensor(SensorPick::Named(s.clone())))));
    let name = match current {
        SensorPick::Named(n) => sensor_label(n),
        _ => String::new(),
    };
    list(opts, Pick::Sensor(current.clone()), |_| name)
}

/// Design `styleName`: Io reads "Read / write" for Disk, "Up / down" for Network.
fn style_name(m: Module, s: CellStyle) -> &'static str {
    let c = &t().choices;
    match s {
        CellStyle::Text => c.text,
        CellStyle::Graph => c.graph,
        CellStyle::Bar => c.bar,
        CellStyle::Io if m == Module::Disk => c.read_write,
        CellStyle::Io => c.up_down,
    }
}

/// The module's allowed styles, preferred first (design `M.styles`).
pub(super) fn style(m: Module, s: CellStyle) -> (Vec<Opt>, usize) {
    let opts = m.allowed_styles().iter().map(|&a| (style_name(m, a).to_string(), Pick::Style(m, a))).collect();
    list(opts, Pick::Style(m, s), |_| style_name(m, s).to_string())
}

pub(super) fn temp_unit(u: TempUnit) -> (Vec<Opt>, usize) {
    let opts = [("\u{b0}C", Pick::TempUnit(TempUnit::Celsius)), ("\u{b0}F", Pick::TempUnit(TempUnit::Fahrenheit))];
    fixed(&opts, Pick::TempUnit(u))
}

/// A module's own interval; the first option follows the default (design "Default (1 second)").
pub(super) fn module_interval(m: Module, secs: Option<u32>, default_ms: u32) -> (Vec<Opt>, usize) {
    let c = &t().choices;
    let default = in_seconds(&c.default_every, c.default_every_fraction, default_ms as f64 / 1000.0);
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
        let (langs, i) = labels(language(Some(Lang::Ja)));
        assert_eq!((langs.len(), langs[0].as_str(), langs[i].as_str()), (12, "Use system setting", "日本語"));
        assert_eq!(labels(language(None)).1, 0);
        assert_eq!(
            labels(anchor(Anchor::Left, false)),
            (vec!["Next to system tray".into(), "Left edge of taskbar".into()], 1)
        );
        assert_eq!(
            labels(anchor(Anchor::Left, true)),
            (vec!["Next to system tray".into(), "Top of taskbar".into()], 1)
        );
        let (opts, i) = module_interval(Module::Cpu, None, 5000);
        assert_eq!((opts[0].label.as_str(), i, opts.len()), ("Default (5 seconds)", 0, 4));
        assert_eq!(module_interval(Module::Cpu, Some(2), 1000).1, 2);
        assert_eq!(
            labels(style(Module::Disk, CellStyle::Bar)),
            (vec!["Read / write".into(), "Text".into(), "Bar".into()], 2)
        );
        assert_eq!(labels(style(Module::Network, CellStyle::Io)).0, ["Up / down", "Graph"]);
        assert_eq!(labels(temp_unit(TempUnit::Fahrenheit)), (vec!["\u{b0}C".into(), "\u{b0}F".into()], 1));
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
    fn lists_from_readings() {
        use busy_core::{NetIf, NetInfo, SensorReading, VolumeInfo};
        let temp = |h: &str, n: &str, kind| SensorReading {
            source: String::new(),
            hardware: h.into(),
            name: n.into(),
            kind,
            value: 0.0,
        };
        let snap = Snapshot {
            volumes: vec![VolumeInfo { mount: "C:".into(), label: "Windows".into(), ..Default::default() }],
            net: Some(NetInfo {
                interfaces: ["Wi-Fi", "Ethernet", "Wi-Fi"]
                    .map(|n| NetIf { name: n.into(), ..Default::default() })
                    .to_vec(),
                ..Default::default()
            }),
            sensors: vec![
                temp("CPU", "Package", SensorKind::Temperature),
                temp("CPU", "Fan", SensorKind::Fan),
                temp("GPU", "Hot Spot", SensorKind::Temperature),
            ],
            ..Default::default()
        };
        let ch = Choices::from_snapshot(&snap);
        assert_eq!(ch.volumes, [("C:".to_string(), "Windows".to_string())]);
        assert_eq!(ch.adapters, ["Wi-Fi", "Ethernet"]);
        assert_eq!(ch.sensors, ["CPU/Package", "GPU/Hot Spot"]);
    }

    #[test]
    fn machine_lists() {
        let ch = Choices {
            volumes: vec![("C:".into(), "Windows".into()), ("E:".into(), String::new())],
            adapters: vec!["Wi-Fi 2".into()],
            sensors: vec!["GPU/Hot Spot".into()],
            ..Choices::default()
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
