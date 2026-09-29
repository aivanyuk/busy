//! Win32 plumbing shared by the busy crates: strings, registry reads, System32-only DLL loading, PDH.
//!
//! Depends on `windows` only and holds no policy: callers decide what to read and what a failure means.

mod dll;
pub mod pdh;
mod reg;
mod text;

pub use dll::Dll;
pub use reg::{reg_bytes, reg_dword, reg_string};
pub use text::{from_wide, utf16, wide};
