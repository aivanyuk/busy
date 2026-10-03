//! What one module's flyout shows, in the design's detailed layout (`isDetailed`, design `fly()`). The modules
//! under `modules/` fill it from a `Ctx`; `draw.rs` lays it out and paints it. Rows without data are left out,
//! never faked.

use busy_core::{Module, RateUnit, SensorKind, TempUnit};
use busy_ui::fmt;
use busy_ui::history::Series;
use busy_ui::theme::Color;

pub(super) struct Detail<'a> {
    /// Whose flyout it is (the footer's settings button).
    pub(super) module: Module,
    pub(super) title: &'static str,
    /// Hardware line under the title.
    pub(super) sub: String,
    /// The headline value and what it is, right of the title.
    pub(super) big: String,
    pub(super) big_label: String,
    pub(super) chart: Option<Chart<'a>>,
    /// Composition bar: (share, color); shares are relative.
    pub(super) seg: Vec<(f32, Color)>,
    /// (label, value, swatch color).
    pub(super) legend: Vec<(String, String, Color)>,
    /// Per-logical-processor load 0..=100 and color.
    pub(super) cores: Vec<(f32, Color)>,
    /// Two-column (key, value) grid.
    pub(super) stats: Vec<(String, String)>,
    pub(super) bars_title: &'static str,
    pub(super) bars: Vec<BarRow>,
    pub(super) procs_title: &'static str,
    /// (process name, value), busiest first.
    pub(super) procs: Vec<(String, String)>,
    /// A paragraph under the header, e.g. why a module has nothing to show.
    pub(super) note: Option<String>,
}

/// The history chart: the first line is filled (area .22), later ones are lines only.
pub(super) struct Chart<'a> {
    pub(super) lines: Vec<(&'a Series, Color)>,
    pub(super) max: f32,
    /// Below-left: the time the chart spans.
    pub(super) span: String,
    /// Below-right: the scale ("0–100%", "Peak 3.2 MB/s").
    pub(super) max_label: String,
    /// How the hover readout formats a sample.
    pub(super) value: Value,
}

/// A labelled thin bar (a volume's fill, a temperature).
pub(super) struct BarRow {
    pub(super) label: String,
    pub(super) text: String,
    pub(super) frac: f32,
    pub(super) color: Color,
}

/// Formatting of chart samples for the hover readout.
#[derive(Clone, Copy)]
pub(super) enum Value {
    Pct,
    Rate(RateUnit),
    Sensor(SensorKind, TempUnit),
}

impl Value {
    pub(super) fn format(self, v: f32) -> String {
        match self {
            Value::Pct => fmt::pct(v),
            Value::Rate(unit) => fmt::rate_in(v as f64, unit),
            Value::Sensor(kind, unit) => fmt::sensor(v, kind, unit),
        }
    }
}

impl<'a> Detail<'a> {
    pub(super) fn new(module: Module) -> Self {
        Self {
            module,
            title: busy_ui::i18n::t().common.module(module),
            sub: String::new(),
            big: String::new(),
            big_label: String::new(),
            chart: None,
            seg: Vec::new(),
            legend: Vec::new(),
            cores: Vec::new(),
            stats: Vec::new(),
            bars_title: "",
            bars: Vec::new(),
            procs_title: "Top processes",
            procs: Vec::new(),
            note: None,
        }
    }

    /// A module whose first sample hasn't arrived.
    pub(super) fn waiting(module: Module) -> Self {
        Self { note: Some("Waiting for data…".into()), ..Self::new(module) }
    }

    /// Adds a stat row when there is a value.
    pub(super) fn stat(&mut self, key: &str, value: Option<String>) {
        if let Some(v) = value {
            self.stats.push((key.into(), v));
        }
    }
}

/// Design `span`: "Last 60 seconds" up to two minutes, then minutes.
pub(super) fn span(secs: u64) -> String {
    if secs < 120 { format!("Last {secs} seconds") } else { format!("Last {} minutes", secs / 60) }
}

/// The time a series spans when full.
pub(super) fn series_span(s: &Series) -> String {
    span(s.cap() as u64 * s.interval_ms() as u64 / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans() {
        assert_eq!(span(60), "Last 60 seconds");
        assert_eq!(span(120), "Last 2 minutes");
        assert_eq!(span(3600), "Last 60 minutes");
        assert_eq!(series_span(&Series::new(120, 1000)), "Last 2 minutes");
    }

    #[test]
    fn values_format_like_the_cells() {
        assert_eq!(Value::Pct.format(42.4), "42%");
        assert_eq!(Value::Rate(RateUnit::Bits).format(1024.0), "8.0 Kb/s");
        assert_eq!(Value::Sensor(SensorKind::Temperature, TempUnit::Celsius).format(61.2), "61°C");
    }
}
