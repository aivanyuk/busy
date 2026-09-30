//! The preview's clock (design: time and date beside the widget, as on the taskbar), in the user's formats.

use windows::Win32::Globalization::{DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx, TIME_NOSECONDS};

/// (time, date) now, e.g. ("2:07 PM", "9/30/2026"); empty strings if the formatting fails.
pub(super) fn now() -> (String, String) {
    let mut t = [0u16; 64];
    let mut d = [0u16; 64];
    // SAFETY: the buffers are valid for their length; null locale/time/format mean the user default and now.
    let (nt, nd) = unsafe {
        (
            GetTimeFormatEx(None, TIME_NOSECONDS, None, None, Some(&mut t)),
            GetDateFormatEx(None, DATE_SHORTDATE, None, None, Some(&mut d), None),
        )
    };
    let text = |buf: &[u16], n: i32| String::from_utf16_lossy(&buf[..(n.max(1) as usize - 1).min(buf.len())]);
    (text(&t, nt), text(&d, nd))
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_something() {
        let (t, d) = super::now();
        assert!(t.chars().any(|c| c.is_ascii_digit()), "{t}");
        assert!(d.chars().any(|c| c.is_ascii_digit()), "{d}");
    }
}
