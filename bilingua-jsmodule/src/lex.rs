//! The lexer: strings, template literals (without `${}`), numbers, identifiers, comments and punctuation.
//! Nothing is executed; anything the lexer cannot read is refused with a line number.

use crate::{Limits, ModuleError};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Str,
    Num,
    Ident,
    Punct,
}

#[derive(Debug)]
pub struct Tok {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    /// A line terminator came between the previous token and this one (automatic semicolon insertion).
    pub nl: bool,
    /// For a string: the quote byte (`'`, `"` or a backtick).
    pub quote: u8,
    /// For a string: the cooked value.
    pub cooked: String,
}

/// First `max` characters of `s`, quoted the way the LocOS messages quote them.
pub fn short(s: &str) -> String {
    let mut out = String::from("'");
    for (i, c) in s.chars().enumerate() {
        if i >= 20 {
            break;
        }
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\'' => out.push_str("\\'"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// A file that is not a static locale module, with the line the problem is on.
pub fn refuse<T>(text: &str, pos: usize, why: &str) -> Result<T, ModuleError> {
    let pos = pos.min(text.len());
    let line = text.as_bytes()[..pos]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1;
    Err(ModuleError::Refused(format!(
        "This file is not a plain JavaScript/TypeScript locale module (line {line}: {why}). Only `export default {{...}}`, \
         `module.exports = {{...}}` or `const x = {{...}}` files whose values are string literals can be translated; \
         the file is never executed."
    )))
}

/// Python's `str.isspace` (what the LocOS regex `\s` matches) plus the byte order mark.
fn is_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}' | '\u{feff}')
}

fn is_lt(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$' || c == '\u{200c}' || c == '\u{200d}'
}

/// Cooks the body of a string or template: ECMAScript escapes (modules are strict, so legacy octal escapes are
/// refused). A template's raw CR and CRLF read as LF. Returns the reason on failure.
pub fn decode(body: &str, tmpl: bool) -> Result<String, &'static str> {
    if !(body.contains('\\') || tmpl && body.contains('\r')) {
        return Ok(body.to_string());
    }
    let normalised;
    let body = if tmpl && body.contains('\r') {
        normalised = body.replace("\r\n", "\n").replace('\r', "\n");
        normalised.as_str()
    } else {
        body
    };
    let mut out = String::with_capacity(body.len());
    let mut high: Option<u32> = None;
    let mut it = body.char_indices().peekable();
    // Pushes a code point from an escape, pairing surrogates.
    fn put(out: &mut String, high: &mut Option<u32>, cp: u32) -> Result<(), &'static str> {
        if (0xD800..0xDC00).contains(&cp) {
            if high.is_some() {
                return Err("a lone surrogate \\u escape");
            }
            *high = Some(cp);
            return Ok(());
        }
        if (0xDC00..0xE000).contains(&cp) {
            let Some(h) = high.take() else {
                return Err("a lone surrogate \\u escape");
            };
            let c = 0x10000 + ((h - 0xD800) << 10) + (cp - 0xDC00);
            out.push(char::from_u32(c).ok_or("a lone surrogate \\u escape")?);
            return Ok(());
        }
        if high.is_some() {
            return Err("a lone surrogate \\u escape");
        }
        out.push(char::from_u32(cp).ok_or("an invalid \\u{...} escape")?);
        Ok(())
    }
    while let Some((i, c)) = it.next() {
        if c != '\\' {
            if high.is_some() {
                return Err("a lone surrogate \\u escape");
            }
            out.push(c);
            continue;
        }
        let rest = &body[i + 1..];
        let Some(e) = rest.chars().next() else { break };
        match e {
            'u' => {
                if let Some(inner) = rest[1..].strip_prefix('{') {
                    if let Some(close) = inner.find('}') {
                        let digits = &inner[..close];
                        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit()) {
                            let trimmed = digits.trim_start_matches('0');
                            if trimmed.len() > 6 {
                                return Err("an invalid \\u{...} escape");
                            }
                            let cp = u32::from_str_radix(
                                if trimmed.is_empty() { "0" } else { trimmed },
                                16,
                            )
                            .unwrap_or(u32::MAX);
                            if cp > 0x10FFFF {
                                return Err("an invalid \\u{...} escape");
                            }
                            put(&mut out, &mut high, cp)?;
                            // consume `u{digits}`
                            for _ in 0..(close + 3) {
                                it.next();
                            }
                            continue;
                        }
                    }
                    return Err("a malformed \\u or \\x escape");
                }
                let h = rest
                    .get(1..5)
                    .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
                match h {
                    Some(h) => {
                        put(&mut out, &mut high, u32::from_str_radix(h, 16).unwrap_or(0))?;
                        for _ in 0..5 {
                            it.next();
                        }
                    }
                    None => return Err("a malformed \\u or \\x escape"),
                }
            }
            'x' => {
                let h = rest
                    .get(1..3)
                    .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
                match h {
                    Some(h) => {
                        put(&mut out, &mut high, u32::from_str_radix(h, 16).unwrap_or(0))?;
                        it.next();
                        it.next();
                        it.next();
                    }
                    None => return Err("a malformed \\u or \\x escape"),
                }
            }
            _ => {
                it.next();
                if high.is_some() {
                    return Err("a lone surrogate \\u escape");
                }
                match e {
                    '0' => out.push('\0'),
                    '1'..='9' => return Err("a legacy octal escape (not allowed in a module)"),
                    '\n' | '\r' | '\u{2028}' | '\u{2029}' => {
                        // line continuation; CRLF is one
                        if e == '\r' && rest[1..].starts_with('\n') {
                            it.next();
                        }
                    }
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'v' => out.push('\u{b}'),
                    other => out.push(other),
                }
            }
        }
    }
    if high.is_some() {
        return Err("a lone surrogate \\u escape");
    }
    Ok(out)
}

/// End of a quoted string starting at `pos`, or `None` when it is not terminated on its line.
fn quoted_end(b: &[u8], pos: usize) -> Option<usize> {
    let q = b[pos];
    let mut i = pos + 1;
    while i < b.len() {
        match b[i] {
            c if c == q => return Some(i + 1),
            b'\\' => {
                if i + 1 >= b.len() {
                    return None;
                }
                i += if b[i + 1] == b'\r' && b.get(i + 2) == Some(&b'\n') {
                    3
                } else {
                    2
                };
            }
            b'\n' | b'\r' => return None,
            _ => i += 1,
        }
    }
    None
}

fn digits(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
        i += 1;
    }
    i
}

fn number_end(b: &[u8], pos: usize) -> usize {
    let n = b.len();
    let radix = |i: usize, ok: fn(u8) -> bool| -> Option<usize> {
        let mut j = i;
        while j < n && (ok(b[j]) || b[j] == b'_') {
            j += 1;
        }
        (j > i).then_some(j)
    };
    let mut end = None;
    if b[pos] == b'0' && pos + 1 < n {
        end = match b[pos + 1] {
            b'x' | b'X' => radix(pos + 2, |c| c.is_ascii_hexdigit()),
            b'o' | b'O' => radix(pos + 2, |c| (b'0'..=b'7').contains(&c)),
            b'b' | b'B' => radix(pos + 2, |c| c == b'0' || c == b'1'),
            _ => None,
        };
    }
    let mut i = match end {
        Some(e) => e,
        None => {
            let mut i = pos;
            if b[i] == b'.' {
                i = digits(b, i + 1);
            } else {
                i = digits(b, i);
                if i < n && b[i] == b'.' {
                    i = digits(b, i + 1);
                }
            }
            if i < n && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < n && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                if j < n && b[j].is_ascii_digit() {
                    i = digits(b, j);
                }
            }
            i
        }
    };
    if i < n && b[i] == b'n' {
        i += 1;
    }
    i
}

fn punct_len(b: &[u8], pos: usize) -> Option<usize> {
    let rest = &b[pos..];
    if rest.starts_with(b"...") {
        return Some(3);
    }
    if rest.starts_with(b"=>") || rest.starts_with(b"?.") || rest.starts_with(b"??") {
        return Some(2);
    }
    if b"{}[]();,:=.<>|&?!+-*/%~^@#".contains(&rest[0]) {
        return Some(1);
    }
    None
}

pub fn lex(text: &str, limits: &Limits) -> Result<Vec<Tok>, ModuleError> {
    let b = text.as_bytes();
    let n = b.len();
    let cap = 60 * limits.max_units;
    let mut toks: Vec<Tok> = Vec::with_capacity((n / 6).min(cap).min(1 << 20));
    let mut pos = 0;
    if text.starts_with("#!") || text.starts_with("\u{feff}#!") {
        pos = text.find('\n').unwrap_or(n);
    }
    let mut nl = false;
    while pos < n {
        let c = b[pos];
        // Strings and templates.
        if c == b'`' {
            let mut i = pos + 1;
            let end = loop {
                if i >= n {
                    return refuse(text, pos, "an unterminated template literal");
                }
                match b[i] {
                    b'\\' => i += 2,
                    b'`' => break i + 1,
                    b'$' if b.get(i + 1) == Some(&b'{') => {
                        return refuse(text, i, "a template literal with ${...} interpolation")
                    }
                    _ => i += 1,
                }
            };
            let cooked =
                decode(&text[pos + 1..end - 1], true).or_else(|why| refuse(text, pos, why))?;
            toks.push(Tok {
                kind: Kind::Str,
                start: pos,
                end,
                nl,
                quote: b'`',
                cooked,
            });
            nl = false;
            pos = end;
        } else if c == b'\'' || c == b'"' {
            let Some(end) = quoted_end(b, pos) else {
                return refuse(
                    text,
                    pos,
                    &format!(
                        "unexpected {}",
                        short(&text[pos..].chars().take(12).collect::<String>())
                    ),
                );
            };
            let cooked =
                decode(&text[pos + 1..end - 1], false).or_else(|why| refuse(text, pos, why))?;
            toks.push(Tok {
                kind: Kind::Str,
                start: pos,
                end,
                nl,
                quote: c,
                cooked,
            });
            nl = false;
            pos = end;
        } else if c == b'/' && b.get(pos + 1) == Some(&b'/') {
            let mut i = pos + 2;
            while i < n
                && !(b[i] == b'\n'
                    || b[i] == b'\r'
                    || (b[i] == 0xE2
                        && b.get(i + 1) == Some(&0x80)
                        && matches!(b.get(i + 2), Some(0xA8 | 0xA9))))
            {
                i += 1;
            }
            pos = i;
        } else if c == b'/' && b.get(pos + 1) == Some(&b'*') {
            let Some(rel) = text[pos + 2..].find("*/") else {
                return refuse(text, pos, "an unterminated /* comment");
            };
            let end = pos + 2 + rel + 2;
            if text[pos..end].chars().any(is_lt) {
                nl = true;
            }
            pos = end;
        } else if c.is_ascii_digit()
            || (c == b'.' && b.get(pos + 1).is_some_and(u8::is_ascii_digit))
        {
            let end = number_end(b, pos);
            toks.push(Tok {
                kind: Kind::Num,
                start: pos,
                end,
                nl,
                quote: 0,
                cooked: String::new(),
            });
            nl = false;
            pos = end;
        } else if c < 0x80 && !c.is_ascii_alphabetic() && c != b'_' && c != b'$' {
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c | 0x1c..=0x1f) {
                let mut i = pos;
                while i < n
                    && matches!(
                        b[i],
                        b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c | 0x1c..=0x1f
                    )
                {
                    if b[i] == b'\n' || b[i] == b'\r' {
                        nl = true;
                    }
                    i += 1;
                }
                pos = i;
            } else if let Some(len) = punct_len(b, pos) {
                toks.push(Tok {
                    kind: Kind::Punct,
                    start: pos,
                    end: pos + len,
                    nl,
                    quote: 0,
                    cooked: String::new(),
                });
                nl = false;
                pos += len;
            } else {
                return refuse(
                    text,
                    pos,
                    &format!(
                        "unexpected {}",
                        short(&text[pos..].chars().take(12).collect::<String>())
                    ),
                );
            }
        } else {
            let ch = text[pos..].chars().next().unwrap_or('\0');
            if is_space(ch) {
                let mut i = pos;
                for c in text[pos..].chars() {
                    if !is_space(c) {
                        break;
                    }
                    if is_lt(c) {
                        nl = true;
                    }
                    i += c.len_utf8();
                }
                pos = i;
            } else if is_ident_start(ch) {
                let mut i = pos;
                for c in text[pos..].chars() {
                    if !is_ident_part(c) {
                        break;
                    }
                    i += c.len_utf8();
                }
                toks.push(Tok {
                    kind: Kind::Ident,
                    start: pos,
                    end: i,
                    nl,
                    quote: 0,
                    cooked: String::new(),
                });
                nl = false;
                pos = i;
            } else {
                return refuse(
                    text,
                    pos,
                    &format!(
                        "unexpected {}",
                        short(&text[pos..].chars().take(12).collect::<String>())
                    ),
                );
            }
        }
        if toks.len() > cap {
            return Err(ModuleError::Limit(format!(
                "This file has far more tokens than the limit allows ({} units). Split it into smaller files.",
                limits.max_units
            )));
        }
    }
    Ok(toks)
}
