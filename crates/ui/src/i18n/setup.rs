//! Setup: the onboarding page of the settings window.

section! {
    /// Setup (onboarding), shown at first start: pick readings, pick a side, done. A fixed-width page (616 DIPs
    /// of content) that grows in height when a text wraps; shorter text that fits a line looks best.
    Setup {
        /// The page's headline, 28 px; one line in English, wraps when longer.
        headline: &'static str = "Your PC\u{2019}s vitals, right on the taskbar",
        /// Under the headline; wraps. "Settings" names the settings window.
        sub: &'static str =
            "Pick what to show. Click any reading for details \u{2014} you can change all of this later in Settings.",
        /// Heading over the reading cards (three columns, each a module's name and its live reading).
        readings: &'static str = "Show on taskbar",
        /// Heading over the two position cards.
        position: &'static str = "Position",
        /// The two positions (each card's text about 250 DIPs wide: a label over a description, one line each in
        /// English, wrapping when longer). First: next to the notification area at the taskbar's end.
        near_tray: &'static str = "Next to the system tray",
        near_tray_desc: &'static str = "Right side, beside the clock",
        /// Second: the taskbar's other end, where Windows' Widgets button usually is.
        left_edge: &'static str = "Left edge of the taskbar",
        left_edge_desc: &'static str = "Where Widgets usually sits",
        /// The first's description on a taskbar standing on the left or right edge of the screen.
        near_tray_desc_vertical: &'static str = "Bottom, just above the clock",
        /// The second on such a taskbar, whose other end is its top.
        top: &'static str = "Top of the taskbar",
        top_desc: &'static str = "Above the app icons",
        /// Footer: a checkbox, then two buttons sized to their text.
        startup: &'static str = "Start with Windows",
        skip: &'static str = "Skip",
        start: &'static str = "Start monitoring",
    }
}
