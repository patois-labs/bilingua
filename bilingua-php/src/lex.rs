//! The PHP lexer: string literals (single, double, heredoc, nowdoc), numbers, variables, identifiers, comments and the
//! few punctuation marks an array file uses. It reads text and never runs it.

use bilingua_model::{Error, LimitError, LimitKind, Limits};
use std::borrow::Cow;

pub const FORMAT: &str = "php";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Str,
    Num,
    Var,
    Ident,
    Punct,
    Close,
}

/// How a string literal was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quote {
    Single,
    Double,
    Heredoc,
    Nowdoc,
}

impl Quote {
    pub fn as_char(self) -> char {
        match self {
            Quote::Single => '\'',
            Quote::Double => '"',
            Quote::Heredoc => 'h',
            Quote::Nowdoc => 'n',
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tok<'a> {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub raw: &'a str,
    /// The decoded value of a string literal.
    pub value: Cow<'a, str>,
    pub quote: Quote,
}

/// The 1-based line of a byte offset.
pub fn line_of(text: &str, pos: usize) -> usize {
    text.as_bytes()[..pos.min(text.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

pub fn refuse(text: &str, pos: usize, why: &str) -> Error {
    Error::Parse {
        format: FORMAT.into(),
        msg: format!(
            "This PHP file is not a plain localization array (line {}: {why}). Only `return [...]` or `$lang['key'] = '...'` files whose values are string literals can be translated; the file is never executed.",
            line_of(text, pos)
        ),
    }
}

/// Python's `\s`: Unicode white space plus the four information separators.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

/// Python's `\w`.
pub fn is_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

fn is_ident_start(c: char) -> bool {
    is_word(c) && !c.is_ascii_digit()
}

/// A heredoc label character: `[A-Za-z0-9_\x80-￿]`.
fn is_label_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || ('\u{80}'..='\u{ffff}').contains(&c)
}

fn is_label_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || ('\u{80}'..='\u{ffff}').contains(&c)
}

/// Starts after the opening quote; returns the offset after the closing quote.
fn quoted_end(b: &[u8], mut i: usize, q: u8) -> Option<usize> {
    while i < b.len() {
        match b[i] {
            b'\\' => {
                if i + 1 >= b.len() {
                    return None;
                }
                i += 2;
            }
            c if c == q => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn decode_sq(body: &str) -> Cow<'_, str> {
    if !body.contains('\\') {
        return Cow::Borrowed(body);
    }
    let mut out = String::with_capacity(body.len());
    let mut it = body.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' && matches!(it.peek(), Some('\\' | '\'')) {
            out.push(it.next().unwrap_or('\\'));
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// Double-quoted or heredoc body to its value. Escapes produce bytes (a PHP string is bytes) and the result must be
/// UTF-8. Any variable interpolation is refused.
fn decode_dq<'a>(
    text: &str,
    pos: usize,
    body: &'a str,
    in_dq: bool,
) -> Result<Cow<'a, str>, Error> {
    // `\\` and `\$` are not interpolation starts
    let masked;
    let probe: &str = if body.contains('\\') {
        masked = body.replace("\\\\", "\0\0").replace("\\$", "\0\0");
        &masked
    } else {
        body
    };
    let pb = probe.as_bytes();
    let mut k = 0;
    while let Some(p) = probe[k..].find(['$', '{']) {
        let at = k + p;
        let rest = &probe[at + 1..];
        let next = rest.chars().next();
        let hit = if pb[at] == b'$' {
            // PHP: a label starts with [a-zA-Z_] or any byte 0x80..0xff, so any non-ASCII character starts one
            // (LocOS stops at U+FFFF and misses "$" before an emoji, which PHP interpolates)
            next.is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '{' || !c.is_ascii())
        } else {
            next == Some('$')
        };
        if hit {
            return Err(refuse(text, pos, "a variable is interpolated into a string"));
        }
        k = at + 1;
    }
    if !body.contains('\\') {
        return Ok(Cow::Borrowed(body));
    }
    let b = body.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let Some(&c) = b.get(i + 1) else {
            out.push(b'\\');
            i += 1;
            continue;
        };
        match c {
            b'n' | b't' | b'r' | b'v' | b'e' | b'f' | b'\\' | b'$' | b'"' => {
                if c == b'"' && !in_dq {
                    out.extend_from_slice(b"\\\"");
                } else {
                    out.push(match c {
                        b'n' => 0x0A,
                        b't' => 0x09,
                        b'r' => 0x0D,
                        b'v' => 0x0B,
                        b'e' => 0x1B,
                        b'f' => 0x0C,
                        other => other,
                    });
                }
                i += 2;
            }
            b'0'..=b'7' => {
                let mut j = i + 1;
                let mut v = 0u32;
                while j < b.len() && j < i + 4 && (b'0'..=b'7').contains(&b[j]) {
                    v = v * 8 + (b[j] - b'0') as u32;
                    j += 1;
                }
                out.push((v & 0xFF) as u8);
                i = j;
            }
            b'x' if b.get(i + 2).is_some_and(u8::is_ascii_hexdigit) => {
                let mut j = i + 2;
                let mut v = 0u32;
                while j < b.len() && j < i + 4 && b[j].is_ascii_hexdigit() {
                    v = v * 16 + (b[j] as char).to_digit(16).unwrap_or(0);
                    j += 1;
                }
                out.push(v as u8);
                i = j;
            }
            b'u' if b.get(i + 2) == Some(&b'{') => {
                let mut j = i + 3;
                while j < b.len() && b[j].is_ascii_hexdigit() {
                    j += 1;
                }
                if j > i + 3 && b.get(j) == Some(&b'}') {
                    // LocOS reads the first eight hex digits only
                    let digits = &body[i + 3..(i + 3 + 8).min(j)];
                    let cp = u32::from_str_radix(digits, 16).unwrap_or(u32::MAX);
                    match char::from_u32(cp) {
                        Some(ch) if cp <= 0x10FFFF => {
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return Err(refuse(text, pos, "an invalid \\u{...} escape")),
                    }
                    i = j + 1;
                } else {
                    out.push(b'\\');
                    i += 1;
                }
            }
            _ => {
                out.push(b'\\');
                i += 1;
            }
        }
    }
    String::from_utf8(out)
        .map(Cow::Owned)
        .map_err(|_| refuse(text, pos, "a string escape that is not valid UTF-8"))
}

fn heredoc<'a>(text: &'a str, pos: usize) -> Result<Tok<'a>, Error> {
    let b = text.as_bytes();
    let mut i = pos + 3;
    while i < b.len() && matches!(b[i], b' ' | b'\t') {
        i += 1;
    }
    let bad = || refuse(text, pos, "a malformed heredoc");
    let (quote_ch, nowdoc) = match b.get(i) {
        Some(b'"') => (Some('"'), false),
        Some(b'\'') => (Some('\''), true),
        _ => (None, false),
    };
    if quote_ch.is_some() {
        i += 1;
    }
    let label_start = i;
    let mut it = text[i..].chars();
    match it.next() {
        Some(c) if is_label_start(c) => i += c.len_utf8(),
        _ => return Err(bad()),
    }
    for c in it {
        if !is_label_char(c) {
            break;
        }
        i += c.len_utf8();
    }
    let label = &text[label_start..i];
    if let Some(q) = quote_ch {
        if b.get(i).map(|&x| x as char) != Some(q) {
            return Err(bad());
        }
        i += 1;
    }
    if b.get(i) == Some(&b'\r') {
        i += 1;
    }
    if b.get(i) != Some(&b'\n') {
        return Err(bad());
    }
    let body_start = i + 1;
    // the closing label: first line (after a "\n") that is optional indent, the label, then a non-label character
    let mut p = body_start;
    let close = loop {
        let mut j = p;
        while j < b.len() && matches!(b[j], b' ' | b'\t') {
            j += 1;
        }
        if text[j..].starts_with(label)
            && !text[j + label.len()..]
                .chars()
                .next()
                .is_some_and(is_label_char)
        {
            break Some((p, j - p, j + label.len()));
        }
        match b[p..].iter().position(|&x| x == b'\n') {
            Some(k) => p += k + 1,
            None => break None,
        }
    };
    let Some((cstart, indent_len, cend)) = close else {
        return Err(refuse(text, pos, "a heredoc with no closing label"));
    };
    // PHP also ends a body line at a bare CR, so a closing label after one closes the heredoc there; this scanner
    // looks after LF only, so a bare CR in the body is refused (else code between two heredocs would read as text)
    let region = &b[body_start..cstart];
    if region
        .iter()
        .enumerate()
        .any(|(k, &x)| x == b'\r' && region.get(k + 1) != Some(&b'\n'))
    {
        return Err(refuse(text, pos, "a heredoc with a bare CR line break"));
    }
    let indent = &text[cstart..cstart + indent_len];
    let mut body = &text[body_start..cstart];
    if let Some(r) = body.strip_suffix("\r\n") {
        body = r;
    } else if let Some(r) = body.strip_suffix('\n') {
        body = r;
    }
    let stripped: Cow<'_, str> = if indent.is_empty() {
        Cow::Borrowed(body)
    } else {
        let mut out = String::with_capacity(body.len());
        for (n, ln) in body.split('\n').enumerate() {
            if n > 0 {
                out.push('\n');
            }
            if let Some(rest) = ln.strip_prefix(indent) {
                out.push_str(rest);
            } else if !ln.trim_matches('\r').is_empty() {
                return Err(refuse(
                    text,
                    pos,
                    "a heredoc line indented less than its closing label",
                ));
            }
        }
        Cow::Owned(out)
    };
    let value = if nowdoc {
        stripped
    } else {
        match stripped {
            Cow::Borrowed(s) => decode_dq(text, pos, s, false)?,
            Cow::Owned(s) => Cow::Owned(decode_dq(text, pos, &s, false)?.into_owned()),
        }
    };
    Ok(Tok {
        kind: Kind::Str,
        start: pos,
        end: cend,
        raw: &text[pos..cend],
        value,
        quote: if nowdoc { Quote::Nowdoc } else { Quote::Heredoc },
    })
}

fn num_end(text: &str, pos: usize) -> Option<usize> {
    let b = text.as_bytes();
    if !b[pos].is_ascii_digit() {
        return None;
    }
    if b[pos] == b'0'
        && matches!(b.get(pos + 1), Some(b'x' | b'X'))
        && b.get(pos + 2)
            .is_some_and(|c| c.is_ascii_hexdigit() || *c == b'_')
    {
        let mut i = pos + 2;
        while i < b.len() && (b[i].is_ascii_hexdigit() || b[i] == b'_') {
            i += 1;
        }
        return Some(i);
    }
    let mut i = pos + 1;
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
        i += 1;
    }
    if b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if b.get(j).is_some_and(u8::is_ascii_digit) {
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    Some(i)
}

fn word_end(text: &str, mut i: usize) -> usize {
    for c in text[i..].chars() {
        if !is_word(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// Lexes `text` from `pos` (just after `<?php`). Stops at `?>`.
pub fn lex<'a>(text: &'a str, mut pos: usize, limits: &Limits) -> Result<Vec<Tok<'a>>, Error> {
    let b = text.as_bytes();
    let n = b.len();
    let cap = limits.max_units.saturating_mul(60);
    let mut toks: Vec<Tok<'a>> = Vec::new();
    let tok = |kind, start, end, quote| Tok {
        kind,
        start,
        end,
        raw: &text[start..end],
        value: Cow::Borrowed(""),
        quote,
    };
    let mut steps = 0usize;
    while pos < n {
        steps += 1;
        if steps > cap {
            return Err(too_many_tokens(limits));
        }
        let c = text[pos..].chars().next().unwrap_or('\0');
        if is_space(c) {
            pos += c.len_utf8();
            while let Some(d) = text[pos..].chars().next() {
                if !is_space(d) {
                    break;
                }
                pos += d.len_utf8();
            }
            continue;
        }
        if b[pos] == b'<' && text[pos..].starts_with("<<<") {
            let t = heredoc(text, pos)?;
            pos = t.end;
            toks.push(t);
            continue;
        }
        // comments
        if b[pos] == b'/' && b.get(pos + 1) == Some(&b'*') {
            if let Some(k) = text[pos + 2..].find("*/") {
                pos = pos + 2 + k + 2;
                continue;
            }
        } else if (b[pos] == b'/' && b.get(pos + 1) == Some(&b'/'))
            || (b[pos] == b'#' && b.get(pos + 1) != Some(&b'['))
        {
            let mut i = pos + if b[pos] == b'#' { 1 } else { 2 };
            while i < n {
                // PHP ends a line comment at LF, at a CR (a bare CR too) or before `?>`
                match b[i] {
                    b'\n' | b'\r' => break,
                    b'?' if b.get(i + 1) == Some(&b'>') => break,
                    _ => i += 1,
                }
            }
            pos = i;
            continue;
        }
        match b[pos] {
            b'\'' => {
                if let Some(e) = quoted_end(b, pos + 1, b'\'') {
                    let mut t = tok(Kind::Str, pos, e, Quote::Single);
                    t.value = decode_sq(&text[pos + 1..e - 1]);
                    pos = e;
                    toks.push(t);
                    continue;
                }
            }
            b'"' => {
                if let Some(e) = quoted_end(b, pos + 1, b'"') {
                    let mut t = tok(Kind::Str, pos, e, Quote::Double);
                    t.value = decode_dq(text, pos, &text[pos + 1..e - 1], true)?;
                    pos = e;
                    toks.push(t);
                    continue;
                }
            }
            b'0'..=b'9' => {
                if let Some(e) = num_end(text, pos) {
                    toks.push(tok(Kind::Num, pos, e, Quote::Single));
                    pos = e;
                    continue;
                }
            }
            b'$' => {
                if text[pos + 1..].chars().next().is_some_and(is_ident_start) {
                    let e = word_end(text, pos + 1);
                    toks.push(tok(Kind::Var, pos, e, Quote::Single));
                    pos = e;
                    continue;
                }
            }
            b'?' if b.get(pos + 1) == Some(&b'>') => {
                let e = pos + 2;
                if !text[e..].trim_matches(is_space).is_empty() {
                    return Err(refuse(text, e, "content after the closing ?> tag"));
                }
                toks.push(tok(Kind::Close, pos, e, Quote::Single));
                break;
            }
            b'=' if b.get(pos + 1) == Some(&b'>') => {
                toks.push(tok(Kind::Punct, pos, pos + 2, Quote::Single));
                pos += 2;
                continue;
            }
            b'|' if b.get(pos + 1) == Some(&b'|') => {
                toks.push(tok(Kind::Punct, pos, pos + 2, Quote::Single));
                pos += 2;
                continue;
            }
            b'[' | b']' | b'(' | b')' | b',' | b';' | b'=' | b'.' | b'-' | b'+' => {
                toks.push(tok(Kind::Punct, pos, pos + 1, Quote::Single));
                pos += 1;
                continue;
            }
            _ => {}
        }
        if is_ident_start(c) {
            let e = word_end(text, pos);
            toks.push(tok(Kind::Ident, pos, e, Quote::Single));
            pos = e;
            continue;
        }
        let snippet: String = text[pos..].chars().take(12).collect();
        return Err(refuse(text, pos, &format!("unexpected {snippet:?}")));
    }
    Ok(toks)
}

fn too_many_tokens(limits: &Limits) -> Error {
    LimitError::new(
        LimitKind::Units,
        format!(
            "This PHP file has far more tokens than {} segments allow. Split it into smaller files.",
            limits.max_units
        ),
    )
    .into()
}
