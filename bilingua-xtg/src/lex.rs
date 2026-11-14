//! The line walker and the tag scanner. Everything here is iterative: no recursion, every loop advances.

use bilingua_model::{Error, LimitError, LimitKind};

/// Longest single tag, in characters (a hyperlink or rubi `<A(...)>` can be long, never this long).
pub const MAX_TAG_CHARS: usize = 65536;
/// Deepest parenthesis nesting inside one tag.
pub const MAX_PAREN_DEPTH: usize = 64;
/// Tags allowed per allowed unit, counted before lexing (LocOS `_TAGS_PER_SEGMENT`).
pub const TAGS_PER_UNIT: usize = 200;

/// One piece of a paragraph line: a run of text (or one escaped literal character) or one tag.
#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub is_tag: bool,
    /// Byte range in the text.
    pub a: usize,
    pub b: usize,
    /// For an escaped character (`<\@>` `<\<>` `<\\>` `<\>>`): the character it stands for.
    pub lit: Option<char>,
}

pub fn utf8_len(lead: u8) -> usize {
    match lead {
        0xF0..=0xFF => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}

pub enum ScanError {
    Unclosed,
    Deep,
}

/// `<\#nnn>` (one to seven digits) and `<\X>` (one character): the end of the tag, or `None`.
fn special(b: &[u8], i: usize, end: usize) -> Option<usize> {
    if i + 2 >= end || b[i + 1] != b'\\' {
        return None;
    }
    let k = i + 2;
    if b[k] == b'#' {
        let mut m = k + 1;
        while m < end && m < k + 8 && b[m].is_ascii_digit() {
            m += 1;
        }
        if m > k + 1 && m < end && b[m] == b'>' {
            return Some(m + 1);
        }
    }
    let l = utf8_len(b[k]);
    if k + l < end && b[k + l] == b'>' {
        return Some(k + l + 1);
    }
    None
}

/// `b[i] == b'<'`: the index just past the tag's closing `>`. Quotes and parentheses are honoured (a rubi or
/// hyperlink `<A(...)>` nests tags); a backslash inside a tag protects the next character.
pub fn scan_tag(b: &[u8], i: usize, end: usize) -> Result<usize, ScanError> {
    if let Some(e) = special(b, i, end) {
        return Ok(e);
    }
    let (mut depth, mut quoted) = (0usize, false);
    let (mut j, mut seen) = (i + 1, 1usize);
    while j < end && seen < MAX_TAG_CHARS {
        let ch = b[j];
        if ch == b'\\' {
            j += 1;
            if j < end {
                j += utf8_len(b[j]);
            }
            seen += 2;
            continue;
        }
        if quoted {
            if ch == b'"' {
                quoted = false;
            }
        } else if ch == b'"' {
            quoted = true;
        } else if ch == b'(' {
            depth += 1;
            if depth > MAX_PAREN_DEPTH {
                return Err(ScanError::Deep);
            }
        } else if ch == b')' {
            depth = depth.saturating_sub(1);
        } else if ch == b'>' && depth == 0 {
            return Ok(j + 1);
        }
        j += 1;
        if ch & 0xC0 != 0x80 {
            seen += 1;
        }
    }
    Err(ScanError::Unclosed)
}

fn limit_deep(line_no: usize) -> Error {
    LimitError::new(
        LimitKind::XmlDepth,
        format!("An XPress Tags tag on line {line_no} nests more than {MAX_PAREN_DEPTH} parentheses."),
    )
    .into()
}

fn unclosed(line_no: usize) -> Error {
    Error::Parse {
        format: "xtg".into(),
        msg: format!(
            "This file could not be read as XPress Tags: the tag starting on line {line_no} is not closed with '>' on the same line."
        ),
    }
}

/// Lexes `text[start..end]` (one line, no line break inside) into `items`.
pub fn lex(text: &str, start: usize, end: usize, line_no: usize, items: &mut Vec<Item>) -> Result<(), Error> {
    let b = text.as_bytes();
    let mut i = start;
    while i < end {
        let Some(p) = b[i..end].iter().position(|&c| c == b'<') else {
            items.push(Item { is_tag: false, a: i, b: end, lit: None });
            break;
        };
        let lt = i + p;
        if lt > i {
            items.push(Item { is_tag: false, a: i, b: lt, lit: None });
        }
        let j = scan_tag(b, lt, end).map_err(|e| match e {
            ScanError::Deep => limit_deep(line_no),
            ScanError::Unclosed => unclosed(line_no),
        })?;
        let lit = if j == lt + 4 && b[lt + 1] == b'\\' && b[lt + 3] == b'>' {
            match b[lt + 2] {
                b'@' => Some('@'),
                b'<' => Some('<'),
                b'\\' => Some('\\'),
                b'>' => Some('>'),
                _ => None,
            }
        } else {
            None
        };
        items.push(Item { is_tag: lit.is_none(), a: lt, b: j, lit });
        i = j;
    }
    Ok(())
}

fn class_ok(c: char) -> bool {
    !matches!(c, '"' | ':' | '=' | '@' | '<' | '\r' | '\n')
}

/// `@Name=` (a style sheet or font set definition) at the start of `s`: not text.
/// `~Name=<` (a bookmark definition) likewise.
pub fn is_definition(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some('@') => {
            let mut n = 0usize;
            for c in it {
                if !class_ok(c) {
                    return c == '=';
                }
                n += 1;
                if n > 255 {
                    return false;
                }
            }
            false
        }
        Some('~') => {
            let mut n = 0usize;
            while let Some(c) = it.next() {
                if matches!(c, '<' | '>' | '=' | '"' | '\r' | '\n') {
                    return n >= 1 && c == '=' && it.next() == Some('<');
                }
                n += 1;
                if n > 255 {
                    return false;
                }
            }
            false
        }
        _ => false,
    }
}

/// `@Name:` at the start of `s`: the style name and the byte length of the whole `@Name:`.
pub fn style_apply(s: &str) -> Option<(&str, usize)> {
    let rest = s.strip_prefix('@')?;
    let mut n = 0usize;
    for (k, c) in rest.char_indices() {
        if !class_ok(c) {
            return (c == ':').then(|| (&rest[..k], 1 + k + 1));
        }
        n += 1;
        if n > 255 {
            return None;
        }
    }
    None
}

/// The header tags `<vNN.NN>` and `<eN>` at the start of the first line: the byte length consumed.
pub fn header_len(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    loop {
        let Some(k) = header_tag(&b[i..]) else { return i };
        i += k;
    }
}

fn header_tag(b: &[u8]) -> Option<usize> {
    if b.first() != Some(&b'<') {
        return None;
    }
    match b.get(1)? {
        b'v' => {
            if !b.get(2)?.is_ascii_digit() {
                return None;
            }
            let mut k = 3;
            while k < 3 + 15 && k < b.len() && (b[k].is_ascii_digit() || b[k] == b'.') {
                k += 1;
            }
            (b.get(k) == Some(&b'>')).then_some(k + 1)
        }
        b'e' => {
            let mut k = 2;
            while k < 2 + 3 && k < b.len() && b[k].is_ascii_digit() {
                k += 1;
            }
            (k > 2 && b.get(k) == Some(&b'>')).then_some(k + 1)
        }
        _ => None,
    }
}

/// The `<eN>` tag of a header: `(optional <vNN.NN>)` then spaces or tabs then `<eN>`, anchored at the start of
/// `b`. Returns the byte range of the `<eN>` tag and its number.
pub fn e_tag(b: &[u8]) -> Option<(usize, usize, u32)> {
    let mut i = 0;
    if b.starts_with(b"<v") {
        if let Some(k) = header_tag(b).filter(|_| true) {
            i = k;
        }
    }
    // the optional <v..> group may also be absent although the text starts with `<v`: both readings are tried
    for start in [i, 0] {
        let mut j = start;
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
            j += 1;
        }
        if b.get(j) == Some(&b'<') && b.get(j + 1) == Some(&b'e') {
            if let Some(k) = header_tag(&b[j..]) {
                let digits = &b[j + 2..j + k - 1];
                let n = digits.iter().fold(0u32, |a, d| a * 10 + (d - b'0') as u32);
                return Some((j, j + k, n));
            }
        }
        if i == 0 {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_plain_quoted_and_nested_tags() {
        let s = br#"<B>x<A(3,"a>b",\#002)[2]>y"#;
        assert_eq!(scan_tag(s, 0, s.len()).ok(), Some(3));
        assert_eq!(scan_tag(s, 4, s.len()).ok(), Some(s.len() - 1));
    }

    #[test]
    fn deep_and_unclosed_are_errors() {
        let mut s = b"<A".to_vec();
        s.extend(std::iter::repeat_n(b'(', 80));
        s.push(b'>');
        assert!(matches!(scan_tag(&s, 0, s.len()), Err(ScanError::Deep)));
        assert!(matches!(scan_tag(b"<B tag", 0, 6), Err(ScanError::Unclosed)));
    }

    #[test]
    fn definitions_and_applies() {
        assert!(is_definition("@Normal=[S\"\"]"));
        assert!(!is_definition("@Body:text"));
        assert!(is_definition("~Mark=<x>"));
        assert!(!is_definition("~Mark=x"));
        assert_eq!(style_apply("@Body:Hello"), Some(("Body", 6)));
        assert_eq!(style_apply("@:x"), Some(("", 2)));
        assert_eq!(style_apply("@a=b:"), None);
    }

    #[test]
    fn header_tags() {
        assert_eq!(header_len("<v19.00><e9>\r\n"), 12);
        assert_eq!(header_len("<e9><v1>x"), 8);
        assert_eq!(header_len("<v>x"), 0);
        assert_eq!(e_tag(b"<v19.00> <e9>@x"), Some((9, 13, 9)));
        assert_eq!(e_tag(b"<e19>"), Some((0, 5, 19)));
        assert_eq!(e_tag(b"<v1>x<e9>"), None);
    }
}
