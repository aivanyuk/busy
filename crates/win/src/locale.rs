//! The user's language settings.

use crate::from_wide;
use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
use windows::core::PWSTR;

/// Windows' display languages as BCP 47 tags, most preferred first ("de-DE", "en-US"). Empty on failure.
pub fn ui_languages() -> Vec<String> {
    let (mut count, mut len) = (0, 0);
    // SAFETY: a length query: a null buffer with `len` 0.
    if unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut len) }.is_err() {
        return Vec::new();
    }
    let mut buf = vec![0u16; len as usize];
    // SAFETY: `buf` holds the `len` characters the first call asked for.
    if unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, Some(PWSTR(buf.as_mut_ptr())), &mut len) }
        .is_err()
    {
        return Vec::new();
    }
    // A double-NUL-terminated list.
    buf.split(|&c| c == 0).filter(|s| !s.is_empty()).map(from_wide).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_something() {
        assert!(!ui_languages().is_empty());
    }
}
