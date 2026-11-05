//! The .resjson scanner: one JSON object with JavaScript comments, walked without recursion, that reports every
//! string value with the byte range of its text (between the quotes) and the position it sits at.
//!
//! The grammar is the one Localization OS accepts: `//` and `/* */` comments and the U+FEFF character count as
//! white space, a trailing comma before `}` or `]` is accepted, the top level must be one object. Nothing is
//! rewritten; the caller keeps every byte it does not translate.

use crate::Limits;
use bilingua_model::Error;
use std::borrow::Cow;

/// A string value found in the file.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The key of the member that holds the value; empty for an array element.
    pub key: String,
    /// The decoded value. `None` when it holds a lone surrogate escape, which a Rust string cannot hold.
    pub value: Option<String>,
    /// Byte range of the text between the quotes.
    pub start: usize,
    pub end: usize,
    /// Slash-joined names of the objects above (arrays add nothing): `menu`, `a/b`. Empty at the top.
    pub context: String,
    /// JSON pointer of the value, array indices included.
    pub pointer: String,
}

/// A `_id.attr` member whose value is not a string (`"_count.maxLength": 40`).
#[derive(Debug, Clone)]
pub struct MetaLiteral {
    pub key: String,
    pub text: String,
    pub context: String,
}

#[derive(Debug, Default)]
pub struct Scanned {
    pub entries: Vec<Entry>,
    pub literals: Vec<MetaLiteral>,
    /// True when the file has a `//` or `/* */` comment.
    pub saw_comment: bool,
}

/// `_<id>.<attr>`: metadata of the resource `<id>`, never text to translate.
pub fn is_meta(key: &str) -> bool {
    key.strip_prefix('_').is_some_and(|r| r.contains('.'))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tok {
    Str,
    Lit,
    P(u8),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    Key,
    Colon,
    Value,
    Sep,
}

struct Frame {
    obj: bool,
    context: String,
    pointer: String,
    expect: Expect,
    /// Next array index.
    idx: usize,
    /// The member name read last (objects).
    key: String,
}

struct Scan<'a> {
    text: &'a str,
    b: &'a [u8],
    i: usize,
    saw_comment: bool,
}

pub fn scan(text: &str, limits: &Limits) -> Result<Scanned, Error> {
    let mut s = Scan { text, b: text.as_bytes(), i: 0, saw_comment: false };
    let mut out = Scanned::default();
    let mut stack: Vec<Frame> = Vec::new();
    let mut seen_root = false;
    while let Some((tok, st, en)) = s.next()? {
        let Some(top) = stack.last_mut() else {
            if seen_root {
                return Err(s.err(st, "text after the top-level object"));
            }
            if tok != Tok::P(b'{') {
                return Err(s.err(st, "a .resjson file must be one JSON object"));
            }
            stack.push(Frame { obj: true, context: String::new(), pointer: String::new(), expect: Expect::Key, idx: 0, key: String::new() });
            seen_root = true;
            continue;
        };
        match top.expect {
            Expect::Key => {
                // `top` is an object here (an array never expects a key).
                match tok {
                    Tok::P(b'}') => {
                        stack.pop();
                        after_value(&mut stack);
                    }
                    Tok::Str => {
                        top.key = s.key(st, en)?;
                        top.expect = Expect::Colon;
                    }
                    _ => return Err(s.err(st, "expected a key")),
                }
            }
            Expect::Colon => {
                if tok != Tok::P(b':') {
                    return Err(s.err(st, "expected ':' after the key"));
                }
                top.expect = Expect::Value;
            }
            Expect::Sep => match tok {
                Tok::P(b',') => top.expect = if top.obj { Expect::Key } else { Expect::Value },
                Tok::P(b'}') if top.obj => {
                    stack.pop();
                    after_value(&mut stack);
                }
                Tok::P(b']') if !top.obj => {
                    stack.pop();
                    after_value(&mut stack);
                }
                _ => return Err(s.err(st, "expected ',' or the end of the object")),
            },
            Expect::Value => {
                if !top.obj && tok == Tok::P(b']') {
                    stack.pop();
                    after_value(&mut stack);
                    continue;
                }
                let key = if top.obj { std::mem::take(&mut top.key) } else { String::new() };
                let pointer = if top.obj {
                    let mut p = String::with_capacity(top.pointer.len() + key.len() + 1);
                    p.push_str(&top.pointer);
                    p.push('/');
                    push_pointer_escaped(&mut p, &key);
                    p
                } else {
                    format!("{}/{}", top.pointer, top.idx)
                };
                if !top.obj {
                    top.idx += 1;
                }
                match tok {
                    Tok::P(c @ (b'{' | b'[')) => {
                        if stack.len() >= limits.max_depth {
                            return Err(Error::Unsupported(format!(
                                "this .resjson file is nested too deeply (limit {} levels)",
                                limits.max_depth
                            )));
                        }
                        let top = stack.last_mut().expect("checked above");
                        let context = if top.obj {
                            if top.context.is_empty() {
                                key
                            } else {
                                format!("{}/{}", top.context, key)
                            }
                        } else {
                            top.context.clone()
                        };
                        top.expect = Expect::Sep;
                        stack.push(Frame {
                            obj: c == b'{',
                            context,
                            pointer,
                            expect: if c == b'{' { Expect::Key } else { Expect::Value },
                            idx: 0,
                            key: String::new(),
                        });
                    }
                    Tok::Str => {
                        let inner = &s.text[st + 1..en - 1];
                        let value = match decode(inner) {
                            Ok(v) => Some(v.into_owned()),
                            Err(StrErr::LoneSurrogate) => None,
                            Err(e) => return Err(s.str_err(st, e)),
                        };
                        out.entries.push(Entry { key, value, start: st + 1, end: en - 1, context: top.context.clone(), pointer });
                        top.expect = Expect::Sep;
                    }
                    Tok::Lit => {
                        let lit = &s.text[st..en];
                        if !valid_literal(lit) {
                            return Err(s.err(st, "expected a value"));
                        }
                        if top.obj && is_meta(&key) {
                            out.literals.push(MetaLiteral { key, text: lit.to_string(), context: top.context.clone() });
                        }
                        top.expect = Expect::Sep;
                    }
                    Tok::P(_) => return Err(s.err(st, "expected a value")),
                }
            }
        }
    }
    if !stack.is_empty() || !seen_root {
        return Err(Error::Parse { format: crate::ID.into(), msg: "the file ends before its object is closed".into() });
    }
    out.saw_comment = s.saw_comment;
    Ok(out)
}

fn after_value(stack: &mut [Frame]) {
    if let Some(top) = stack.last_mut() {
        top.expect = Expect::Sep;
    }
}

fn push_pointer_escaped(out: &mut String, key: &str) {
    for c in key.chars() {
        match c {
            '~' => out.push_str("~0"),
            '/' => out.push_str("~1"),
            c => out.push(c),
        }
    }
}

fn valid_literal(s: &str) -> bool {
    if s == "true" || s == "false" || s == "null" {
        return true;
    }
    let b = s.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'-') {
        i += 1;
    }
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        let from = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == from {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let from = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == from {
            return false;
        }
    }
    i == b.len()
}

impl Scan<'_> {
    fn err(&self, at: usize, msg: &str) -> Error {
        let before = &self.text[..at.min(self.text.len())];
        let line = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        Error::Parse { format: crate::ID.into(), msg: format!("{msg} (line {line}, column {col})") }
    }

    fn str_err(&self, at: usize, e: StrErr) -> Error {
        match e {
            StrErr::BadEscape => self.err(at, "the string has a bad escape"),
            StrErr::Control => self.err(at, "the string has a raw control character (write it as an escape)"),
            StrErr::LoneSurrogate => self.err(at, "the string has an unpaired surrogate escape"),
        }
    }

    /// The decoded text of a member name. A name with a lone surrogate escape is read with U+FFFD in its place.
    fn key(&self, st: usize, en: usize) -> Result<String, Error> {
        let inner = &self.text[st + 1..en - 1];
        match decode(inner) {
            Ok(v) => Ok(v.into_owned()),
            Err(StrErr::LoneSurrogate) => Ok(decode_lossy(inner)),
            Err(e) => Err(self.str_err(st, e)),
        }
    }

    /// The next token: a string (range includes the quotes), a bare literal, or one punctuation byte.
    fn next(&mut self) -> Result<Option<(Tok, usize, usize)>, Error> {
        let b = self.b;
        let n = b.len();
        loop {
            let Some(&c) = b.get(self.i) else { return Ok(None) };
            match c {
                b' ' | b'\t' | b'\r' | b'\n' => self.i += 1,
                0xEF if b[self.i..].starts_with(&[0xEF, 0xBB, 0xBF]) => self.i += 3,
                b'/' if b.get(self.i + 1) == Some(&b'/') => {
                    self.saw_comment = true;
                    while self.i < n && !matches!(b[self.i], b'\r' | b'\n') {
                        self.i += 1;
                    }
                }
                b'/' if b.get(self.i + 1) == Some(&b'*') => {
                    self.saw_comment = true;
                    match find(&b[self.i + 2..], b"*/") {
                        Some(j) => self.i += 2 + j + 2,
                        None => return Err(self.err(self.i, "a /* comment is never closed")),
                    }
                }
                b'"' => {
                    let st = self.i;
                    let mut j = st + 1;
                    loop {
                        match b.get(j) {
                            None => return Err(self.err(st, "a string is never closed")),
                            Some(b'\\') => j += 2,
                            Some(b'"') => break,
                            Some(_) => j += 1,
                        }
                        if j > n {
                            return Err(self.err(st, "a string is never closed"));
                        }
                    }
                    self.i = j + 1;
                    return Ok(Some((Tok::Str, st, j + 1)));
                }
                b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                    let st = self.i;
                    self.i += 1;
                    return Ok(Some((Tok::P(c), st, st + 1)));
                }
                _ => {
                    let st = self.i;
                    let mut j = st;
                    while j < n && !matches!(b[j], b' ' | b'\t' | b'\r' | b'\n' | b',' | b':' | b'{' | b'}' | b'[' | b']' | b'"' | b'/') {
                        j += 1;
                    }
                    if j == st {
                        return Err(self.err(st, "unexpected '/'"));
                    }
                    self.i = j;
                    return Ok(Some((Tok::Lit, st, j)));
                }
            }
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrErr {
    BadEscape,
    Control,
    LoneSurrogate,
}

/// The text a string token's inside stands for (JSON rules: raw control characters and unknown escapes are
/// errors, a surrogate pair is one character, a lone surrogate is `LoneSurrogate`).
pub fn decode(inner: &str) -> Result<Cow<'_, str>, StrErr> {
    let b = inner.as_bytes();
    if !b.iter().any(|&c| c == b'\\' || c < 0x20) {
        return Ok(Cow::Borrowed(inner));
    }
    decode_slow(inner, false).map(Cow::Owned)
}

/// Like [`decode`] for a name, with U+FFFD for a lone surrogate.
fn decode_lossy(inner: &str) -> String {
    decode_slow(inner, true).unwrap_or_else(|_| inner.to_string())
}

fn decode_slow(inner: &str, lossy: bool) -> Result<String, StrErr> {
    let mut out = String::with_capacity(inner.len());
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        if (c as u32) < 0x20 {
            return Err(StrErr::Control);
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next().ok_or(StrErr::BadEscape)? {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let hi = hex4(&mut it)?;
                if (0xD800..0xDC00).contains(&hi) {
                    // A high surrogate needs `\uDC00..DFFF` right after it.
                    let mut peek = it.clone();
                    if peek.next() == Some('\\') && peek.next() == Some('u') {
                        let lo = hex4(&mut peek)?;
                        if (0xDC00..0xE000).contains(&lo) {
                            it = peek;
                            out.push(char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)).ok_or(StrErr::BadEscape)?);
                            continue;
                        }
                    }
                    if !lossy {
                        return Err(StrErr::LoneSurrogate);
                    }
                    out.push('\u{FFFD}');
                } else if (0xDC00..0xE000).contains(&hi) {
                    if !lossy {
                        return Err(StrErr::LoneSurrogate);
                    }
                    out.push('\u{FFFD}');
                } else {
                    out.push(char::from_u32(hi).ok_or(StrErr::BadEscape)?);
                }
            }
            _ => return Err(StrErr::BadEscape),
        }
    }
    Ok(out)
}

fn hex4(it: &mut std::str::Chars<'_>) -> Result<u32, StrErr> {
    let mut v = 0u32;
    for _ in 0..4 {
        v = v * 16 + it.next().and_then(|c| c.to_digit(16)).ok_or(StrErr::BadEscape)?;
    }
    Ok(v)
}

/// The inside of a JSON string token for `text`, escaping only what JSON requires.
pub fn quote_inner(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str("\\u00");
                out.push(char::from_digit(c as u32 >> 4, 16).unwrap_or('0'));
                out.push(char::from_digit(c as u32 & 15, 16).unwrap_or('0'));
            }
            c => out.push(c),
        }
    }
    out
}
