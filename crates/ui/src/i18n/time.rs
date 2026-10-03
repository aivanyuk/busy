//! Durations in words (`fmt`).

section! {
    /// Durations in words (`fmt::hours_minutes_long`, `fmt::duration`). All are abbreviations that don't change
    /// with the count, as in English; `{0}` and `{1}` are whole numbers.
    Time {
        /// Battery time left under an hour, in the flyout ("10 min remaining"): `{0}` minutes.
        minutes_long: &'static str = "{0} min",
        /// Battery time left from an hour: `{0}` hours, `{1}` minutes (0–59).
        hours_minutes_long: &'static str = "{0} h {1} min",
        /// Compact age, e.g. how long ago something happened: `{0}` days, `{1}` hours (0–23).
        days_hours: &'static str = "{0}d {1}h",
        /// Compact age: `{0}` hours, `{1}` minutes (0–59).
        hours_minutes: &'static str = "{0}h {1}m",
        /// Compact age: `{0}` minutes.
        minutes: &'static str = "{0}m",
        /// Compact age: `{0}` seconds.
        seconds: &'static str = "{0}s",
    }
}
