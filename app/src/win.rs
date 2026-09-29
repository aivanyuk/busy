//! Plumbing shared by our windows: class registration, and routing their input to the router.
//!
//! Window modules report input as [`Event`]s through [`raise`]; `app.rs` registers the one handler with
//! [`set_handler`]. Window modules therefore never name the router (docs/architecture.md § Layering).

use busy_core::Module;
use std::cell::Cell;
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{IDC_ARROW, LoadCursorW, RegisterClassExW, WNDCLASSEXW, WNDPROC};
use windows::core::PCWSTR;

/// Input from our windows, handled synchronously by the router on the UI thread.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// The taskbar cell under the pointer, `None` once it left the cells.
    WidgetHover(Option<Module>),
    /// A taskbar cell was clicked.
    WidgetClick(Module),
    WidgetMenu,
    /// DPI or display change seen by the widget.
    WidgetRerender,
    /// The flyout lost activation; can arrive while the router is mid-call.
    FlyoutDeactivated,
    FlyoutEscape,
    FlyoutWheel(i16),
    /// Pointer position in client pixels, `None` once it left.
    FlyoutPointer(Option<(f32, f32)>),
    FlyoutClick(f32, f32),
    FlyoutPaint,
}

thread_local! {
    static HANDLER: Cell<Option<fn(Event)>> = const { Cell::new(None) };
}

/// Installs the router's handler; call before creating any window.
pub fn set_handler(f: fn(Event)) {
    HANDLER.set(Some(f));
}

pub fn raise(ev: Event) {
    if let Some(f) = HANDLER.get() {
        f(ev);
    }
}

pub fn hinstance() -> HINSTANCE {
    unsafe { GetModuleHandleW(None).map(Into::into).unwrap_or_default() }
}

pub fn register_class(name: PCWSTR, proc: WNDPROC) {
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: proc,
        hInstance: hinstance(),
        lpszClassName: name,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        ..Default::default()
    };
    // Fails harmlessly with ERROR_CLASS_ALREADY_EXISTS when re-creating windows.
    unsafe { RegisterClassExW(&wc) };
}
