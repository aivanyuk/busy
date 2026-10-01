//! Win32 plumbing shared by the busy crates: strings, registry reads, System32-only DLL loading, PDH, the
//! taskbar's edge, the package busy runs from.
//!
//! Depends on `windows` only and holds no policy: callers decide what to read and what a failure means.

mod dll;
mod package;
pub mod pdh;
mod reg;
mod taskbar;
mod text;

pub use dll::Dll;
pub use package::package_family;
pub use reg::{reg_bytes, reg_dword, reg_qword, reg_string, reg_subkeys};
pub use taskbar::{Edge, edge_of, taskbar_edge, window_edge};
pub use text::{from_wide, utf16, wide};
