//! Small helpers that behave like the Python the LocOS filter is written in: `str.isspace`, `str.strip`,
//! `html.unescape`, `html.escape`.

use crate::entities::{HTML5, INVALID_CHARREFS, INVALID_CODEPOINTS};

/// Python `str.isspace` for one character.
pub fn is_space(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

pub fn trim(s: &str) -> &str {
    trim_end(trim_start(s))
}

/// Python `str.isalnum` for one character.
pub fn is_alnum(c: char) -> bool {
    bilingua_model::py_isalnum_char(c)
}

/// Python `str.isalpha` for one character.
pub fn is_alpha(c: char) -> bool {
    bilingua_model::py_isalpha_char(c)
}

/// LocOS `_PLACEHOLDER_RE`: `\u{27E6}/?[a-z]?\d+\u{27E7}`. Length in bytes of the token at the start of `s`.
pub fn placeholder_len(s: &str) -> Option<usize> {
    let rest = s.strip_prefix('\u{27E6}')?;
    let b = rest.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'/') {
        i += 1;
    }
    // `[a-z]?` is optional and `\d+` follows, so a letter is taken only when a digit comes next
    if b.get(i).is_some_and(|c| c.is_ascii_lowercase()) && b.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
        i += 1;
    }
    let d0 = i;
    while b.get(i).is_some_and(|c| c.is_ascii_digit()) {
        i += 1;
    }
    if i == d0 || !rest[i..].starts_with('\u{27E7}') {
        return None;
    }
    Some('\u{27E6}'.len_utf8() + i + '\u{27E7}'.len_utf8())
}

/// True when `s`, with placeholder tokens removed, has a letter or digit (LocOS `_is_translatable`).
pub fn translatable_text(s: &str) -> bool {
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if let Some(n) = placeholder_len(rest) {
            i += n;
            continue;
        }
        let Some(c) = rest.chars().next() else { break };
        if is_alnum(c) {
            return true;
        }
        i += c.len_utf8();
    }
    false
}

/// Python `html.unescape`.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let Some(p) = s[i..].find('&') else {
            out.push_str(&s[i..]);
            break;
        };
        out.push_str(&s[i..i + p]);
        i += p;
        let rest = &s[i + 1..];
        let rb = rest.as_bytes();
        if rb.first() == Some(&b'#') {
            let (start, radix) = if rb.get(1).is_some_and(|c| c.is_ascii_digit()) {
                (1, 10)
            } else if matches!(rb.get(1), Some(b'x' | b'X')) && rb.get(2).is_some_and(|c| c.is_ascii_hexdigit()) {
                (2, 16)
            } else {
                out.push('&');
                i += 1;
                continue;
            };
            let mut j = start;
            let mut num: u64 = 0;
            while let Some(d) = rb.get(j).and_then(|c| (*c as char).to_digit(radix)) {
                num = (num * radix as u64 + d as u64).min(0x1000_0000);
                j += 1;
            }
            if rb.get(j) == Some(&b';') {
                j += 1;
            }
            out.push_str(&numeric(num as u32));
            i += 1 + j;
            continue;
        }
        // a name: up to 32 characters that are not tab, newline, form feed, space, <, &, # or ;
        let mut n = 0;
        let mut end = 0;
        for (k, c) in rest.char_indices() {
            if n == 32 || matches!(c, '\t' | '\n' | '\x0c' | ' ' | '<' | '&' | '#' | ';') {
                break;
            }
            n += 1;
            end = k + c.len_utf8();
        }
        if n == 0 {
            out.push('&');
            i += 1;
            continue;
        }
        if rb.get(end) == Some(&b';') {
            end += 1;
        }
        let name = &rest[..end];
        if let Some(v) = lookup(name) {
            out.push_str(v);
        } else {
            let mut found = false;
            let chars: Vec<usize> = name.char_indices().map(|(k, _)| k).collect();
            // prefixes of length len-1 down to 2 (in characters)
            let len = chars.len();
            let mut x = len.saturating_sub(1);
            while x >= 2 {
                let cut = chars[x];
                if let Some(v) = lookup(&name[..cut]) {
                    out.push_str(v);
                    out.push_str(&name[cut..]);
                    found = true;
                    break;
                }
                x -= 1;
            }
            if !found {
                out.push('&');
                out.push_str(name);
            }
        }
        i += 1 + end;
    }
    out
}

fn lookup(name: &str) -> Option<&'static str> {
    HTML5.binary_search_by(|(k, _)| k.as_bytes().cmp(name.as_bytes())).ok().map(|i| HTML5[i].1)
}

/// LocOS `_xml_named_refs` (EPUB, Flare): every `&name;` (`&([A-Za-z][A-Za-z0-9]*);`) that is not one of XML's
/// five becomes numeric character references, or literal text (`&amp;name;`) for an unknown name, so a target
/// written into an XHTML page read without a DTD stays XML.
pub fn xml_named_refs(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'&' && b.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            let n = b[i + 1..].iter().take_while(|c| c.is_ascii_alphanumeric()).count();
            if b.get(i + 1 + n) == Some(&b';') {
                let name = &s[i + 1..i + 1 + n];
                out.push_str(&s[last..i]);
                if matches!(name, "amp" | "lt" | "gt" | "quot" | "apos") {
                    out.push_str(&s[i..i + n + 2]);
                } else {
                    match lookup(&format!("{name};")) {
                        Some(chars) => {
                            for c in chars.chars() {
                                out.push_str(&format!("&#{};", c as u32));
                            }
                        }
                        None => {
                            out.push_str("&amp;");
                            out.push_str(name);
                            out.push(';');
                        }
                    }
                }
                i += n + 2;
                last = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&s[last..]);
    out
}

fn numeric(num: u32) -> String {
    if let Some((_, v)) = INVALID_CHARREFS.iter().find(|(k, _)| *k == num) {
        return (*v).to_string();
    }
    if (0xD800..=0xDFFF).contains(&num) || num > 0x10FFFF {
        return "\u{fffd}".to_string();
    }
    if INVALID_CODEPOINTS.binary_search(&num).is_ok() {
        return String::new();
    }
    char::from_u32(num).map(String::from).unwrap_or_default()
}

/// Python `html.escape(s, quote=True)`.
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

/// LocOS `_escape_text(text, quotes=True)`: a bare `&` (one that does not start a character reference) and every
/// `<`, `>`, `"` and `'`.
pub fn escape_text(s: &str, out: &mut String) {
    escape_text_with(s, true, out)
}

/// LocOS `_escape_text(text, quotes=quotes)`: a bare `&` and every `<` and `>`; with `quotes` also `"` and `'`
/// (a text node needs only the first three, #672).
pub fn escape_text_with(s: &str, quotes: bool, out: &mut String) {
    let b = s.as_bytes();
    let mut last = 0;
    for (i, &c) in b.iter().enumerate() {
        let rep = match c {
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' if quotes => "&quot;",
            b'\'' if quotes => "&#x27;",
            b'&' if !entity_follows(&b[i + 1..]) => "&amp;",
            _ => continue,
        };
        out.push_str(&s[last..i]);
        out.push_str(rep);
        last = i + 1;
    }
    out.push_str(&s[last..]);
}

/// After `&`: `#[0-9]+;`, `#[xX][0-9a-fA-F]+;` or `[a-zA-Z][a-zA-Z0-9]+;`.
fn entity_follows(b: &[u8]) -> bool {
    let run = |from: usize, ok: fn(&u8) -> bool| b[from..].iter().take_while(|c| ok(c)).count();
    match b.first() {
        Some(b'#') => {
            if matches!(b.get(1), Some(b'x' | b'X')) {
                let n = run(2, u8::is_ascii_hexdigit);
                if n > 0 && b.get(2 + n) == Some(&b';') {
                    return true;
                }
            }
            let n = run(1, u8::is_ascii_digit);
            n > 0 && b.get(1 + n) == Some(&b';')
        }
        Some(c) if c.is_ascii_alphabetic() => {
            let n = run(1, u8::is_ascii_alphanumeric);
            n > 0 && b.get(1 + n) == Some(&b';')
        }
        _ => false,
    }
}

/// LocOS `strict_escape`: every character an HTML5 tag state reacts to becomes a character reference.
pub fn strict_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\t' | '\n' | '\x0c' | '\r' | ' ' | '/' | '=' | '`') {
            out.push_str(&format!("&#{};", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

/// LocOS `_escape_text(text, protected, quotes)`: the pieces of `s` that are one of the `protected` strings (the
/// template spans of the unit, tried in order at each position as Python's `re.split` alternation does) are kept
/// as they are; the rest is escaped piece by piece.
pub fn escape_protected(s: &str, protected: &[String], quotes: bool, out: &mut String) {
    let prot: Vec<&str> = protected.iter().map(String::as_str).filter(|p| !p.is_empty()).collect();
    if prot.is_empty() {
        return escape_text_with(s, quotes, out);
    }
    let mut last = 0;
    let mut i = 0;
    while i < s.len() {
        if let Some(p) = prot.iter().find(|p| s[i..].starts_with(**p)) {
            escape_text_with(&s[last..i], quotes, out);
            out.push_str(p);
            i += p.len();
            last = i;
            continue;
        }
        i += s[i..].chars().next().map_or(1, char::len_utf8);
    }
    escape_text_with(&s[last..], quotes, out);
}

/// ASCII case-insensitive substring search.
pub fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    find_ci(hay, 0, needle).is_some()
}

pub fn find_ci(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() || from > hay.len() - needle.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()].eq_ignore_ascii_case(needle))
}

pub fn find_bytes(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > hay.len() || hay.len() - from < needle.len() {
        return None;
    }
    if needle.len() == 1 {
        return hay[from..].iter().position(|&c| c == needle[0]).map(|p| p + from);
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_like_python() {
        assert_eq!(unescape("a &amp; b &lt;x&gt; &quot;q&quot; &#39;s&#x27;"), "a & b <x> \"q\" 's'");
        assert_eq!(unescape("&ampx &notit; &notin; &copy2 &nosuch; &"), "&x \u{ac}it; \u{2209} \u{a9}2 &nosuch; &");
        assert_eq!(unescape("&#0; &#xD800; &#x110000; &#128; &#x1; &#65"), "\u{fffd} \u{fffd} \u{fffd} \u{20ac}  A");
        assert_eq!(unescape("&#; &#x; &#xg"), "&#; &#x; &#xg");
    }

    #[test]
    fn escape_keeps_entities() {
        let mut s = String::new();
        escape_text("a & b &amp; c &#65; &xy; < > \" '", &mut s);
        assert_eq!(s, "a &amp; b &amp; c &#65; &xy; &lt; &gt; &quot; &#x27;");
    }

    #[test]
    fn placeholder_grammar() {
        assert_eq!(placeholder_len("\u{27E6}x1\u{27E7}rest"), Some(8));
        assert_eq!(placeholder_len("\u{27E6}/g12\u{27E7}"), Some(10));
        assert_eq!(placeholder_len("\u{27E6}12\u{27E7}"), Some(8));
        assert_eq!(placeholder_len("\u{27E6}x\u{27E7}"), None);
        assert!(!translatable_text("\u{27E6}x1\u{27E7} - "));
        assert!(translatable_text("\u{27E6}x1\u{27E7} a"));
    }
}
