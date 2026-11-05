//! Small text helpers that follow Python's `str` rules, because the Localization OS filter is the spec:
//! `str.isspace`, `str.isalnum`, `str.strip`, `str.splitlines` and `html.unescape`.

use crate::data::ENTITIES;
use regex::Regex;
use std::sync::OnceLock;

/// Python's `str.isspace` / regex `\s` for `str`: Unicode White_Space plus the four separators U+001C..U+001F.
pub fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

pub fn py_trim(s: &str) -> &str {
    s.trim_matches(py_space)
}

pub fn py_trim_start(s: &str) -> &str {
    s.trim_start_matches(py_space)
}

fn alnum_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[\p{L}\p{N}]").expect("alnum pattern"))
}

/// True when `value` has a letter or digit: Python `any(c.isalnum() for c in value)`. Rust's
/// `char::is_alphanumeric` also counts combining marks; the Unicode categories L and N do not.
pub fn has_alnum(value: &str) -> bool {
    if value.bytes().any(|b| b.is_ascii_alphanumeric()) {
        return true;
    }
    !value.is_ascii() && alnum_re().is_match(value)
}

fn zs_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\p{Zs}$").expect("zs pattern"))
}

/// Unicode category Zs (space separator).
pub fn is_zs(c: char) -> bool {
    let mut b = [0u8; 4];
    zs_re().is_match(c.encode_utf8(&mut b))
}

/// Python `str.splitlines(keepends=True)`.
pub fn split_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let b = text.as_bytes();
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let is_break = matches!(c, '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}');
        if !is_break {
            continue;
        }
        let mut end = i + c.len_utf8();
        if c == '\r' && b.get(end) == Some(&b'\n') {
            it.next();
            end += 1;
        }
        out.push(&text[start..end]);
        start = end;
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

fn entity(name: &str) -> Option<&'static str> {
    ENTITIES.binary_search_by(|(k, _)| k.as_bytes().cmp(name.as_bytes())).ok().map(|i| ENTITIES[i].1)
}

fn numeric(num: u64) -> String {
    const C1: [char; 32] = [
        '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2c6}', '\u{2030}', '\u{160}',
        '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}',
        '\u{2013}', '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
    ];
    if num == 0 {
        return "\u{fffd}".into();
    }
    if num == 0x0d {
        return "\r".into();
    }
    if (0x80..=0x9f).contains(&num) {
        return C1[(num - 0x80) as usize].to_string();
    }
    if (0xd800..=0xdfff).contains(&num) || num > 0x10ffff {
        return "\u{fffd}".into();
    }
    let n = num as u32;
    let invalid = matches!(n, 0x1..=0x8 | 0xb | 0xe..=0x1f | 0x7f..=0x9f | 0xfdd0..=0xfdef) || (n & 0xfffe) == 0xfffe;
    if invalid {
        return String::new();
    }
    char::from_u32(n).map(String::from).unwrap_or_default()
}

fn parse_num(digits: &str, radix: u32) -> u64 {
    let mut v: u64 = 0;
    for d in digits.chars() {
        v = v.saturating_mul(radix as u64).saturating_add(d.to_digit(radix).unwrap_or(0) as u64);
        if v > 0x10ffff {
            return u64::MAX;
        }
    }
    v
}

/// Python `html.unescape`.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p + 1..];
        // after the ampersand
        if let Some(r) = rest.strip_prefix('#') {
            let (radix, body) = match r.strip_prefix(['x', 'X']) {
                Some(h) if h.chars().next().is_some_and(|c| c.is_ascii_hexdigit()) => (16, h),
                _ => (10, r),
            };
            let n = body.chars().take_while(|c| c.is_digit(radix) && c.is_ascii()).count();
            if n > 0 && (radix == 16 || r.chars().next().is_some_and(|c| c.is_ascii_digit())) {
                out.push_str(&numeric(parse_num(&body[..n], radix)));
                rest = &body[n..];
                if let Some(r2) = rest.strip_prefix(';') {
                    rest = r2;
                }
                continue;
            }
            out.push('&');
            continue;
        }
        let name_len: usize = rest.chars().take(32).take_while(|c| !matches!(c, '\t' | '\n' | '\u{0c}' | ' ' | '<' | '&' | '#' | ';')).map(char::len_utf8).sum();
        if name_len == 0 {
            out.push('&');
            continue;
        }
        let semi = rest[name_len..].starts_with(';');
        let s_end = name_len + usize::from(semi);
        let name = &rest[..s_end];
        if let Some(v) = entity(name) {
            out.push_str(v);
            rest = &rest[s_end..];
            continue;
        }
        // longest legacy prefix (no semicolon), at least two characters
        let idx: Vec<usize> = name.char_indices().map(|(i, _)| i).chain(std::iter::once(name.len())).collect();
        let nchars = idx.len() - 1;
        let mut done = false;
        for x in (2..nchars).rev() {
            if let Some(v) = entity(&name[..idx[x]]) {
                out.push_str(v);
                rest = &rest[idx[x]..];
                done = true;
                break;
            }
        }
        if !done {
            out.push('&');
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_like_python() {
        assert_eq!(unescape("Caf\u{e9} &amp; &quot;Bar&quot; &lt;tips&gt;"), "Caf\u{e9} & \"Bar\" <tips>");
        assert_eq!(unescape("&reg;"), "\u{ae}");
        assert_eq!(unescape("&regx"), "\u{ae}x");
        assert_eq!(unescape("&#65;&#x42;&#X43"), "ABC");
        assert_eq!(unescape("&#150;"), "\u{2013}");
        assert_eq!(unescape("&#0;"), "\u{fffd}");
        assert_eq!(unescape("&#1;x"), "x");
        assert_eq!(unescape("&#xD800;"), "\u{fffd}");
        assert_eq!(unescape("&#99999999999999999999;"), "\u{fffd}");
        assert_eq!(unescape("a & b &; &#; &#x; &nosuch;"), "a & b &; &#; &#x; &nosuch;");
        assert_eq!(unescape("&notit;"), "\u{ac}it;");
        assert_eq!(unescape("&amp"), "&");
    }

    #[test]
    fn splitlines_like_python() {
        assert_eq!(split_lines("a\r\nb\rc\nd\u{2028}e"), vec!["a\r\n", "b\r", "c\n", "d\u{2028}", "e"]);
        assert!(split_lines("").is_empty());
    }
}
