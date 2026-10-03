//! The widget's context menu, opened by right-clicking it on the taskbar.

section! {
    /// The widget's context menu.
    Menu {
        /// Opens the settings window; the "…" says it opens a window.
        settings: &'static str = "Settings…",
        /// Submenu listing the modules, each checked when its cell is on the taskbar.
        show_on_taskbar: &'static str = "Show on taskbar",
        /// Submenu: where on the taskbar the widget sits.
        position: &'static str = "Position",
        /// Position: beside the notification area (the tray, by the clock).
        next_to_tray: &'static str = "Next to notification area",
        /// Position on a horizontal taskbar: its other end, away from the tray.
        left_edge: &'static str = "Left edge",
        /// Position on a vertical taskbar: its other end, away from the tray.
        top: &'static str = "Top",
        /// Quits busy.
        exit: &'static str = "Exit",
    }
}
