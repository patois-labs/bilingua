//! Python's character tests, exactly: `str.isalpha`, `str.isalnum` and `str.isspace` of Python 3.13 (Unicode
//! 15.1), which Localization OS uses for its "has a letter or digit" and whitespace rules.
//!
//! Rust's `char::is_alphanumeric` and friends follow a newer Unicode and differ: they count combining marks
//! (U+0301, U+0903, ...) and Unicode 16 letters as alphanumeric where Python does not, and `is_whitespace` leaves
//! out U+001C..U+001F, which Python treats as whitespace. A format crate that must skip or keep the same values as
//! Localization OS calls these instead. The tables come from `tools/gen_pytables.py`.

use crate::pytables::{ALNUM, ALPHA, DECIMAL, PUNCT_SYMBOL, SPACE};

fn in_table(t: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    t.binary_search_by(|&(lo, hi)| {
        if c < lo {
            std::cmp::Ordering::Greater
        } else if c > hi {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    })
    .is_ok()
}

/// `str.isalpha` for one character: a letter (category L*).
pub fn py_isalpha_char(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic();
    }
    in_table(ALPHA, c)
}

/// `str.isalnum` for one character: a letter or a number (decimal, digit or numeric).
pub fn py_isalnum_char(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric();
    }
    in_table(ALNUM, c)
}

/// `str.isdecimal` for one character: a decimal digit (category Nd), what `\d` matches in a Python `re` pattern.
pub fn py_isdecimal_char(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    in_table(DECIMAL, c)
}

/// What `\w` matches in a Python `re` pattern on a `str`: [`py_isalnum_char`] or `_`.
pub fn py_is_word_char(c: char) -> bool {
    c == '_' || py_isalnum_char(c)
}

/// What `[^\W\d]` matches: a word character that is not a decimal digit (a letter, `_`, or a number such as a
/// superscript or roman numeral). The first character of a Python `re` identifier pattern.
pub fn py_is_word_start_char(c: char) -> bool {
    py_is_word_char(c) && !py_isdecimal_char(c)
}

/// `str.isspace` for one character.
pub fn py_isspace_char(c: char) -> bool {
    if c.is_ascii() {
        return matches!(c, '\t'..='\r' | '\x1c'..='\x1f' | ' ');
    }
    in_table(SPACE, c)
}

/// `unicodedata.category(c)[0] in "PS"`: a punctuation or symbol character (CommonMark's Unicode punctuation, as
/// Localization OS's Markdown emphasis repair tests it).
pub fn py_is_punct_or_symbol_char(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_punctuation();
    }
    in_table(PUNCT_SYMBOL, c)
}

/// Python `s.isalpha()`: not empty and every character a letter.
pub fn py_isalpha(s: &str) -> bool {
    !s.is_empty() && s.chars().all(py_isalpha_char)
}

/// Python `s.isalnum()`: not empty and every character a letter or number.
pub fn py_isalnum(s: &str) -> bool {
    !s.is_empty() && s.chars().all(py_isalnum_char)
}

/// Python `s.isspace()`: not empty and every character whitespace.
pub fn py_isspace(s: &str) -> bool {
    !s.is_empty() && s.chars().all(py_isspace_char)
}

/// Python `any(ch.isalnum() for ch in s)`: the string has a letter or digit anywhere. This is the usual
/// "does this value carry text" test; combining marks, punctuation and symbols alone do not count.
pub fn py_has_alnum(s: &str) -> bool {
    s.chars().any(py_isalnum_char)
}

/// Python `any(ch.isalpha() for ch in s)`.
pub fn py_has_alpha(s: &str) -> bool {
    s.chars().any(py_isalpha_char)
}

/// Python `s.strip()` (no argument): strips [`py_isspace_char`] characters from both ends.
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(py_isspace_char)
}

/// Python `s.lstrip()`.
pub fn py_lstrip(s: &str) -> &str {
    s.trim_start_matches(py_isspace_char)
}

/// Python `s.rstrip()`.
pub fn py_rstrip(s: &str) -> &str {
    s.trim_end_matches(py_isspace_char)
}

/// True for the characters Python's `str.splitlines` breaks at (`\r\n` counts as one break): `\n`, `\r`, `\x0b`,
/// `\x0c`, `\x1c`, `\x1d`, `\x1e`, U+0085, U+2028, U+2029.
pub fn py_is_line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Python `s.splitlines(keepends=True)`: every line with its own terminator (`\r\n` counts as one).
pub fn py_splitlines_keepends(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if py_is_line_break(c) {
            let mut end = i + c.len_utf8();
            if c == '\r' && matches!(it.peek(), Some(&(_, '\n'))) {
                it.next();
                end += 1;
            }
            out.push(&s[start..end]);
            start = end;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Splits one line of [`py_splitlines_keepends`] into its body and its terminator (`""`, one break, or `\r\n`).
pub fn py_split_line_end(line: &str) -> (&str, &str) {
    if let Some(body) = line.strip_suffix("\r\n") {
        return (body, "\r\n");
    }
    match line.chars().next_back() {
        Some(c) if py_is_line_break(c) => line.split_at(line.len() - c.len_utf8()),
        _ => (line, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_is_pythons() {
        assert_eq!(py_splitlines_keepends("a\r\nb\rc\u{2028}d\x0ce\n\nf"), ["a\r\n", "b\r", "c\u{2028}", "d\x0c", "e\n", "\n", "f"]);
        assert!(py_splitlines_keepends("").is_empty());
        assert_eq!(py_split_line_end("x\r\n"), ("x", "\r\n"));
        assert_eq!(py_split_line_end("\n"), ("", "\n"));
        assert_eq!(py_split_line_end("x\u{85}"), ("x", "\u{85}"));
        assert_eq!(py_split_line_end("x"), ("x", ""));
    }

    #[test]
    fn combining_marks_are_not_alphanumeric() {
        assert!(!py_has_alnum("\u{0301}"));
        assert!(!py_has_alnum("\u{0903}\u{200D}"));
        assert!(!py_isalnum("\u{0301}"));
        assert!(py_has_alnum("e\u{0301}"));
        // Rust says alphanumeric, Python does not
        assert!('\u{0345}'.is_alphabetic() && !py_isalpha_char('\u{0345}'));
        assert!('\u{05B0}'.is_alphabetic() && !py_isalpha_char('\u{05B0}'));
    }

    #[test]
    fn numbers_count_as_alnum_but_not_alpha() {
        for c in ['5', '\u{0665}', '\u{00B2}', '\u{00BD}', '\u{2160}', '\u{4E00}', '\u{3007}'] {
            assert!(py_isalnum_char(c), "{c:?}");
        }
        assert!(!py_isalpha_char('5') && !py_isalpha_char('\u{00BD}'));
        assert!(py_isalpha_char('\u{4E00}') && !py_isalpha_char('\u{3007}') );
    }

    #[test]
    fn word_chars_follow_python_re() {
        assert!(py_is_word_char('_') && py_is_word_char('\u{00B2}') && !py_is_word_char('-'));
        // superscript two is \w but not \d, so it may start a JSON5 identifier; an Arabic-Indic digit may not
        assert!(py_is_word_start_char('\u{00B2}') && !py_is_word_start_char('\u{0665}') && !py_is_word_start_char('7'));
        assert!(!py_is_word_char('\u{0301}'));
    }

    #[test]
    fn whitespace_matches_python() {
        for c in ['\t', '\n', '\x0b', '\x0c', '\r', '\x1c', '\x1d', '\x1e', '\x1f', ' ', '\u{85}', '\u{A0}', '\u{1680}', '\u{2003}', '\u{2028}', '\u{2029}', '\u{202F}', '\u{205F}', '\u{3000}'] {
            assert!(py_isspace_char(c), "{c:?}");
        }
        for c in ['\u{200B}', '\u{FEFF}', '\u{180E}', '\u{2060}', 'a', '\u{0}'] {
            assert!(!py_isspace_char(c), "{c:?}");
        }
        assert!(!'\x1c'.is_whitespace());
        assert_eq!(py_strip("\x1c a \u{3000}"), "a");
        assert!(!py_isspace("") && !py_isalnum("") && !py_isalpha(""));
    }

    #[test]
    fn rust_and_python_differ_on_unicode_16_letters() {
        // Garay (Unicode 16) is not in Python 3.13's Unicode 15.1
        assert!(!py_isalpha_char('\u{10D4A}'));
        assert!(py_isalpha_char('\u{1E900}'));
    }

    #[test]
    fn tables_are_sorted_and_the_ascii_fast_path_agrees() {
        for t in [ALPHA, ALNUM, DECIMAL, SPACE] {
            assert!(t.windows(2).all(|w| w[0].1 < w[1].0 && w[0].0 <= w[0].1));
        }
        for c in (0u8..128).map(char::from) {
            assert_eq!(py_isalpha_char(c), in_table(ALPHA, c), "{c:?}");
            assert_eq!(py_isalnum_char(c), in_table(ALNUM, c), "{c:?}");
            assert_eq!(py_isspace_char(c), in_table(SPACE, c), "{c:?}");
        }
        // every letter is alphanumeric
        for &(lo, hi) in ALPHA {
            assert!(in_table(ALNUM, char::from_u32(lo).unwrap()) && in_table(ALNUM, char::from_u32(hi).unwrap()));
        }
    }
}
