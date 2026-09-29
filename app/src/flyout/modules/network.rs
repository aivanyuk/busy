use super::super::detail::{Chart, Detail, Value, series_span};
use crate::ctx::Ctx;
use crate::render::nice_max;
use crate::tone::{self, SECOND};
use busy_core::{Module, ModuleCfg, NetKind};
use busy_ui::{fmt, select};

/// Design `fly('net')`: download and upload of the chosen interface (`options.network.interface`, as on the
/// cell) in the chosen units, and what that interface is. No process list: per-process network needs the ETW
/// opt-in.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    let (snap, cfg) = (ctx.snap, ctx.cfg);
    let (Some(n), Some((rx, tx))) = (&snap.net, select::net_rates(snap, cfg)) else {
        return Detail::waiting(Module::Network);
    };
    let (t, hist, units) = (ctx.theme, ctx.hist, cfg.options.network.units);
    let (color, second) = (t.color(tone::module(mc)), t.color(SECOND));
    let rate = |bps: f64| fmt::rate_in(bps, units);
    let nif = select::net_interface(snap, cfg);
    let mut d = Detail::new(Module::Network);
    if let Some(i) = nif {
        d.sub = match (i.kind, &i.wifi) {
            (NetKind::Wifi, Some(w)) if !w.ssid.is_empty() => format!("Wi‑Fi · {}", w.ssid),
            (NetKind::Ethernet, _) => format!("Ethernet · {}", i.name),
            _ => i.name.clone(),
        };
    }
    d.big = rate(rx);
    d.big_label = "Download".into();
    let peak = hist.net_rx.max().max(hist.net_tx.max());
    d.chart = Some(Chart {
        lines: vec![(&hist.net_rx, color), (&hist.net_tx, second)],
        max: nice_max(peak),
        span: series_span(&hist.net_rx),
        max_label: format!("Peak {}", rate(peak as f64)),
        value: Value::Rate(units),
    });
    d.legend = vec![("Download".into(), rate(rx), color), ("Upload".into(), rate(tx), second)];
    if let Some(i) = nif {
        let medium = match i.kind {
            NetKind::Wifi => "Wi‑Fi",
            NetKind::Ethernet => "Ethernet",
            NetKind::Other => "",
        };
        let detail = match (&i.wifi, i.link_speed_bps) {
            (Some(w), _) if w.channel_mhz.is_some() => w.channel_mhz.map(fmt::wifi_band),
            (_, 0) => None,
            (_, bps) => Some(fmt::link_speed(bps)),
        };
        let interface = [Some(medium.to_string()).filter(|m| !m.is_empty()), detail].into_iter().flatten();
        d.stat("Interface", Some(interface.collect::<Vec<_>>().join(" · ")).filter(|s| !s.is_empty()));
        let signal = match (&i.wifi, i.kind) {
            // Design: a real minus sign ("−52 dBm").
            (Some(w), _) => Some(
                w.rssi_dbm.map_or(format!("{}%", w.signal_pct), |dbm| format!("{dbm} dBm").replace('-', "\u{2212}")),
            ),
            (None, NetKind::Ethernet) => Some("Wired".into()),
            _ => None,
        };
        d.stat("Signal", signal);
        d.stat("IPv4", i.ipv4.first().cloned());
    }
    d.stat("Received", Some(fmt::bytes(n.rx_total)));
    d.stat("Sent", Some(fmt::bytes(n.tx_total)));
    d
}
