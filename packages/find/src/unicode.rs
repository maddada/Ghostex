//! Minimal UTF-8 decoding and terminal display-width helpers (port of
//! zehn's `src/unicode.zig`). Used by the TUI for column-accurate truncation.

/// Number of bytes in the UTF-8 sequence starting at `first`. Returns 1 for
/// invalid lead bytes so callers always make progress.
pub fn seq_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    }
}

/// Decode the codepoint at the start of `s`, returning (codepoint, byte length).
/// Invalid sequences decode as U+FFFD over 1 byte.
pub fn decode(s: &[u8]) -> (u32, usize) {
    if s.is_empty() {
        return (0, 0);
    }
    let n = seq_len(s[0]);
    if n == 1 || n > s.len() {
        return (s[0] as u32, 1);
    }
    match std::str::from_utf8(&s[..n]) {
        Ok(text) => match text.chars().next() {
            Some(c) => (c as u32, n),
            None => (0xFFFD, 1),
        },
        Err(_) => (0xFFFD, 1),
    }
}

/// Terminal display width of a codepoint: 0 (combining/zero-width), 1 (normal),
/// or 2 (wide CJK / most emoji). Compact wcwidth approximation.
pub fn char_width(cp: u32) -> usize {
    if cp == 0 {
        return 0;
    }
    if cp < 0x20 || (0x7f..0xa0).contains(&cp) {
        return 0; // control
    }
    if (0x0300..=0x036f).contains(&cp)
        || (0x1ab0..=0x1aff).contains(&cp)
        || (0x1dc0..=0x1dff).contains(&cp)
        || (0x20d0..=0x20ff).contains(&cp)
        || (0xfe20..=0xfe2f).contains(&cp)
        || cp == 0x200b
        || cp == 0x200c
        || cp == 0x200d
        || cp == 0xfeff
    {
        return 0;
    }
    if (0x1100..=0x115f).contains(&cp)
        || (0x2e80..=0x303e).contains(&cp)
        || (0x3041..=0x33ff).contains(&cp)
        || (0x3400..=0x4dbf).contains(&cp)
        || (0x4e00..=0x9fff).contains(&cp)
        || (0xa000..=0xa4cf).contains(&cp)
        || (0xac00..=0xd7a3).contains(&cp)
        || (0xf900..=0xfaff).contains(&cp)
        || (0xfe30..=0xfe4f).contains(&cp)
        || (0xff00..=0xff60).contains(&cp)
        || (0xffe0..=0xffe6).contains(&cp)
        || (0x1f300..=0x1faff).contains(&cp)
        || (0x20000..=0x3fffd).contains(&cp)
    {
        return 2;
    }
    1
}

/// True for codepoints the TUI renders as a single space instead of the glyph.
pub fn is_control(cp: u32) -> bool {
    cp < 0x20 || (0x7f..0xa0).contains(&cp)
}

/// Display width of an entire UTF-8 string.
pub fn width(s: &[u8]) -> usize {
    let mut i = 0;
    let mut w = 0;
    while i < s.len() {
        let (cp, len) = decode(&s[i..]);
        w += char_width(cp);
        i += len;
    }
    w
}

/// Byte index of the character before `pos`.
pub fn prev_char(bytes: &[u8], pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut p = pos - 1;
    while p > 0 && (bytes[p] & 0xC0) == 0x80 {
        p -= 1;
    }
    p
}

/// Byte index of the character after `pos`.
pub fn next_char(bytes: &[u8], pos: usize) -> usize {
    if pos >= bytes.len() {
        return bytes.len();
    }
    pos + decode(&bytes[pos..]).1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_counts_wide_and_zero_width_codepoints() {
        assert_eq!(width("abc".as_bytes()), 3);
        assert_eq!(width("日本".as_bytes()), 4);
        assert_eq!(width("e\u{0301}".as_bytes()), 1);
    }

    #[test]
    fn char_navigation_is_utf8_aware() {
        let s = "aé日".as_bytes();
        assert_eq!(next_char(s, 0), 1);
        assert_eq!(next_char(s, 1), 3);
        assert_eq!(next_char(s, 3), 6);
        assert_eq!(prev_char(s, 6), 3);
        assert_eq!(prev_char(s, 3), 1);
    }
}
