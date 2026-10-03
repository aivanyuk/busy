//! Message boxes and the errors they show.

section! {
    /// The settings window's message boxes (titled "busy") and the reasons they give. "\n\n" is a blank line.
    Dialogs {
        /// Asked before an opt-in source is turned on: {0} is the opt-in's description from Advanced, which
        /// names the settings page.
        confirm_opt_in: &'static str = "Turn this on?\n\n{0}\n\nYou can turn it off again under Advanced.",
        /// Start with Windows couldn't be changed: {0} is the reason, from Windows or one of the three below.
        autostart_failed: &'static str = "Couldn't update the startup entry.\n\n{0}",
        /// busy from the Microsoft Store, turned off by the user in Windows' Settings; the path is as Windows
        /// names it in this language.
        startup_off_by_user: &'static str = "Start with Windows is turned off for busy in Windows Settings \
            \u{2192} Apps \u{2192} Startup. Turn it on there.",
        /// The same, kept off by the organization's policy.
        startup_off_by_policy: &'static str = "Your organization keeps busy from starting with Windows.",
        /// The same, left off for another reason.
        startup_off: &'static str = "Windows left the startup task off.",
    }
}
