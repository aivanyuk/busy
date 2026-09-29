//! What busy's windows show and how they draw it, shared by the app and the settings window: formatting,
//! the choices of what to show, the rolling history, the design's colors, the Direct2D/DirectWrite helpers and
//! the taskbar cell.

pub mod cell;
pub mod ctx;
pub mod fmt;
pub mod history;
pub mod render;
pub mod select;
pub mod theme;
pub mod tone;
