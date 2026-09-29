/// NUL-terminated UTF-16, for `PCWSTR` parameters.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// UTF-16 without a terminator, for length-counted `&[u16]` parameters (DirectWrite).
pub fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// UTF-16 up to the first NUL (or the whole slice), lossily decoded.
pub fn from_wide(s: &[u16]) -> String {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..n])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        assert_eq!(wide("ab"), [97, 98, 0]);
        assert_eq!(utf16("ab"), [97, 98]);
        assert_eq!(from_wide(&wide("°C")), "°C");
        assert_eq!(from_wide(&[97, 0, 98]), "a");
        assert_eq!(from_wide(&[97, 98]), "ab");
        assert_eq!(from_wide(&[]), "");
    }
}
