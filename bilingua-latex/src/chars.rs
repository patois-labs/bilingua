//! Character tests that follow Python's `str` methods (the LocOS filter's), not Rust's near-equivalents.

use regex::Regex;
use std::sync::OnceLock;

/// Python `str.isspace` for one character: Unicode White_Space plus the four separators U+001C..U+001F.
pub fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn letter_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\p{L}$").expect("letter pattern"))
}

/// Python `str.isalpha` for one character: a letter in the Unicode sense (categories L*). Rust's
/// `char::is_alphabetic` is wider (it counts combining marks and letter numbers), so non-ASCII goes through `\p{L}`.
pub fn py_alpha(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphabetic()
    } else {
        let mut buf = [0u8; 4];
        letter_re().is_match(c.encode_utf8(&mut buf))
    }
}

/// The character at byte `i` of `t` and its length in bytes. `i` must be a character boundary.
pub fn char_at(t: &str, i: usize) -> (char, usize) {
    let b = t.as_bytes()[i];
    if b < 0x80 {
        (b as char, 1)
    } else {
        let c = t[i..].chars().next().unwrap_or('\u{fffd}');
        (c, c.len_utf8())
    }
}

/// Python `str.strip()` (no argument).
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(py_space)
}
