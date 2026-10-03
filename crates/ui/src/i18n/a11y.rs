//! Names and states screen readers read out (UI Automation) that the settings window doesn't draw as text.

section! {
    /// Names and states of the settings window's elements as UI Automation gives them to screen readers; never
    /// drawn, so no width limit.
    A11y {
        /// The title bar's buttons (drawn as glyphs). Restore replaces Maximize while the window is maximized.
        minimize: &'static str = "Minimize",
        maximize: &'static str = "Maximize",
        restore: &'static str = "Restore",
        close: &'static str = "Close",
        /// State of the page shown in the nav, or of a dropdown's current option.
        selected: &'static str = "Selected",
        /// State of the page shown when it also has one: {0} is it (On or Off).
        status_selected: &'static str = "{0}, selected",
        /// The Taskbar order card's arrow buttons: {0} is a module's name.
        move_up: &'static str = "Move {0} up",
        move_down: &'static str = "Move {0} down",
        /// A module's color swatches, their value: {0} is the chosen one's place, {1} how many there are.
        color_of: &'static str = "Color {0} of {1}",
    }
}
