//! Character facts the way Python answers them (the LocOS filter's `str.isspace`, `isalpha`, regex `\w`, `\d`,
//! `unicodedata`), from tables generated out of the LocOS venv. Rust's own `char` methods are wider (combining
//! marks count as alphanumeric there), which would move unit boundaries.

use crate::tables::*;

fn in_ranges(t: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    t.binary_search_by(|&(a, b)| {
        if c < a {
            std::cmp::Ordering::Greater
        } else if c > b {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    })
    .is_ok()
}

/// `str.isspace` for one character.
pub fn is_space(c: char) -> bool {
    if (c as u32) < 0x80 {
        return matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1f');
    }
    in_ranges(SPACE, c)
}

/// `str.isalpha`.
pub fn is_alpha(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic();
    }
    in_ranges(ALPHA, c)
}

/// Regex `\w` (Unicode): alphanumeric or underscore.
pub fn is_word(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_';
    }
    in_ranges(ALNUM, c)
}

/// Regex `\d` (Unicode decimal digits).
pub fn is_digit(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    in_ranges(DECIMAL, c)
}

/// `str.strip()` / `lstrip()` / `rstrip()` with no argument.
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}
pub fn lstrip(s: &str) -> &str {
    s.trim_start_matches(is_space)
}
pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `not s.strip()`.
pub fn blank(s: &str) -> bool {
    s.chars().all(is_space)
}

/// `str.split()` with no argument.
pub fn split_ws(s: &str) -> impl Iterator<Item = &str> {
    s.split(is_space).filter(|w| !w.is_empty())
}

/// Number of leading spaces and tabs (`len(s) - len(s.lstrip(" \t"))`), in bytes (they are ASCII).
pub fn indent(s: &str) -> usize {
    s.len() - s.trim_start_matches([' ', '\t']).len()
}

/// docutils' `column_width` per character: combining marks 0, wide and full-width 2, the rest 1.
pub fn cw(c: char) -> usize {
    if (c as u32) < 0x300 {
        return 1;
    }
    if in_ranges(COMBINING, c) {
        0
    } else if in_ranges(WIDE, c) {
        2
    } else {
        1
    }
}

pub fn column_width(s: &str) -> usize {
    s.chars().map(cw).sum()
}

/// True when the character's general category is one of the `P*` categories in `mask`
/// (Ps=1 Pe=2 Pi=4 Pf=8 Pd=16 Po=32).
pub fn punct_in(c: char, mask: u8) -> bool {
    let cp = c as u32;
    PUNCT
        .binary_search_by(|&(a, b, _)| {
            if cp < a {
                std::cmp::Ordering::Greater
            } else if cp > b {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .map(|i| PUNCT[i].2 & mask != 0)
        .unwrap_or(false)
}

/// Byte index after the first `n` characters of `s` (clamped to the end).
pub fn adv(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len())
}

pub fn prev_char(s: &str, i: usize) -> Option<char> {
    s[..i].chars().next_back()
}

pub fn next_char(s: &str, i: usize) -> Option<char> {
    s[i..].chars().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_facts() {
        assert!(is_space('\u{a0}') && is_space('\x1c') && !is_space('a'));
        assert!(is_word('é') && is_word('_') && !is_word('\u{301}'));
        assert!(is_alpha('日') && !is_alpha('1'));
        assert_eq!(column_width("日本語"), 6);
        assert_eq!(column_width("e\u{301}"), 1);
        assert!(punct_in('(', 1) && punct_in('-', 16) && !punct_in('a', 63));
        assert!(is_digit('٣') && !is_digit('x'));
    }
}
