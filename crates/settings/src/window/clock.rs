//! The preview's clock (design: time and date beside the widget, as on the taskbar), in the user's formats.

use windows::Win32::Globalization::{
    DATE_SHORTDATE, ENUM_DATE_FORMATS_FLAGS, GetDateFormatEx, GetLocaleInfoEx, GetTimeFormatEx, LOCALE_SSHORTDATE,
    TIME_NOSECONDS,
};
use windows::core::PCWSTR;

fn text(buf: &[u16], n: i32) -> String {
    String::from_utf16_lossy(&buf[..(n.max(1) as usize - 1).min(buf.len())])
}

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
    (text(&t, nt), text(&d, nd))
}

/// (time, date) as a vertical taskbar's narrow column shows them (design `dateShort`, "10/1/26"): the user's
/// short date with a two-digit year, else the short date as it is.
pub(super) fn now_short() -> (String, String) {
    let (time, date) = now();
    (time, short_date().unwrap_or(date))
}

fn short_date() -> Option<String> {
    let mut pic = [0u16; 80];
    // SAFETY: the buffer is valid for its length; a null locale is the user default.
    let n = unsafe { GetLocaleInfoEx(None, LOCALE_SSHORTDATE, Some(&mut pic)) };
    let pic = text(&pic, n);
    if !pic.contains("yyyy") {
        return None;
    }
    let pic = busy_win::wide(&pic.replace("yyyy", "yy"));
    let mut d = [0u16; 64];
    // SAFETY: `pic` is NUL-terminated and outlives the call; the buffer is valid for its length.
    let n =
        unsafe { GetDateFormatEx(None, ENUM_DATE_FORMATS_FLAGS(0), None, PCWSTR(pic.as_ptr()), Some(&mut d), None) };
    (n > 1).then(|| text(&d, n))
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_something() {
        let (t, d) = super::now();
        assert!(t.chars().any(|c| c.is_ascii_digit()), "{t}");
        assert!(d.chars().any(|c| c.is_ascii_digit()), "{d}");
        let (_, ds) = super::now_short();
        assert!(ds.chars().any(|c| c.is_ascii_digit()) && ds.len() <= d.len(), "{ds}");
    }
}
