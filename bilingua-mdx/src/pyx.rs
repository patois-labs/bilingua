//! Python string behaviour the LocOS filter relies on (`str.isalnum`, `isspace`, `strip`, `splitlines`), so the
//! Rust port cuts lines and tests letters the way the Python does. The character tables come from `unitab.rs`.

use crate::unitab;

fn in_table(t: &[(u32, u32)], c: char) -> bool {
    let u = c as u32;
    t.binary_search_by(|&(a, b)| {
        if u < a {
            std::cmp::Ordering::Greater
        } else if u > b {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    })
    .is_ok()
}

/// Python `str.isspace` for one character (also what `\s` and `strip()` use).
pub(crate) fn is_space(c: char) -> bool {
    if c.is_ascii() {
        return matches!(c, '\t'..='\r' | '\x1c'..=' ');
    }
    in_table(unitab::SPACE, c)
}

/// Python `str.isalnum` for one character.
pub(crate) fn is_alnum(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric();
    }
    in_table(unitab::ALNUM, c)
}

/// Unicode general category P* or S* (non-ASCII part of `md_emphasis._punct`).
pub(crate) fn is_punct_symbol(c: char) -> bool {
    in_table(unitab::PUNCT_SYMBOL, c)
}

/// True when the text has a letter or digit.
pub(crate) fn has_alnum(s: &str) -> bool {
    s.chars().any(is_alnum)
}

/// Python `str.strip()`.
pub(crate) fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}


/// Python `str.rstrip()`.
pub(crate) fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// Python line boundaries (`str.splitlines`): `\n \r \r\n \x0b \x0c \x1c \x1d \x1e \x85    `.
pub(crate) fn is_eol_char(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// One line of a text: `start..end` is its content (the terminator follows it).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Line {
    pub start: usize,
    pub end: usize,
}

/// `text.splitlines(keepends=True)` as offsets. An empty text has no lines; a final terminator does not start a
/// further (empty) line.
pub(crate) fn split_lines(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        // fast path: ASCII bytes
        let c = b[i];
        let (eol, len) = match c {
            b'\n' | 0x0b | 0x0c | 0x1c | 0x1d | 0x1e => (true, 1),
            b'\r' => (true, if b.get(i + 1) == Some(&b'\n') { 2 } else { 1 }),
            0xC2 if b.get(i + 1) == Some(&0x85) => (true, 2),
            0xE2 if b.get(i + 1) == Some(&0x80) && matches!(b.get(i + 2), Some(0xA8) | Some(0xA9)) => (true, 3),
            _ => (false, 1),
        };
        if eol {
            out.push(Line { start, end: i });
            start = i + len;
            i += len;
        } else {
            i += 1;
        }
    }
    if start < b.len() {
        out.push(Line { start, end: b.len() });
    }
    out
}

/// The char starting at byte `i` (which must be a boundary).
pub(crate) fn char_at(s: &str, i: usize) -> Option<char> {
    s.get(i..).and_then(|r| r.chars().next())
}

/// The char ending at byte `i` (the one before it).
pub(crate) fn char_before(s: &str, i: usize) -> Option<char> {
    s.get(..i).and_then(|r| r.chars().next_back())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_like_python() {
        let t = "a\r\nb\rc\nd\u{2028}e\x0bf";
        let v: Vec<&str> = split_lines(t).iter().map(|l| &t[l.start..l.end]).collect();
        assert_eq!(v, ["a", "b", "c", "d", "e", "f"]);
        assert!(split_lines("").is_empty());
        assert_eq!(split_lines("a\n").len(), 1);
        assert_eq!(split_lines("\n\n").len(), 2);
    }

    #[test]
    fn classes() {
        assert!(is_space('\u{a0}') && is_space('\x1f') && !is_space('a'));
        assert!(is_alnum('\u{e9}') && is_alnum('\u{65e5}') && !is_alnum('\u{3001}') && !is_alnum('\u{301}'));
        assert!(is_punct_symbol('\u{3001}') && is_punct_symbol('\u{ff08}') && !is_punct_symbol('a'));
    }
}
