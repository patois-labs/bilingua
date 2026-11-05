//! A JSON5 / JSONC scanner: validates the whole document and records the exact byte range, quote style, decoded
//! value, dotted key path and leading comment of every string VALUE, in document order. Iterative (an explicit
//! stack, no recursion) and linear in the size of the input; it never builds a tree and never touches a byte.
//!
//! Grammar: JSON5 1.0.0 (comments, trailing commas, unquoted and single-quoted keys, single-quoted strings, line
//! continuations, hex numbers, `Infinity` / `NaN`, a leading `+`). JSONC is a subset (JSON plus comments and
//! trailing commas), so one scanner reads both.

use bilingua_model::Error;
use std::borrow::Cow;

/// Caps this crate applies to a file (the model crate has none in this tree). Defaults are far above any real
/// resource file; `Json5Filter::with_limits` changes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Deepest nesting of `{` and `[`.
    pub max_depth: usize,
    /// Most translatable strings in one file.
    pub max_units: usize,
    /// Longest translatable string, in characters.
    pub max_unit_chars: usize,
    /// Largest file, in bytes.
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_depth: 256, max_units: 500_000, max_unit_chars: 100_000, max_bytes: 256 * 1024 * 1024 }
    }
}

/// One string value in the document.
#[derive(Debug, Clone)]
pub struct StrVal {
    /// Byte range of the whole token, quotes included.
    pub start: usize,
    pub end: usize,
    /// The decoded value.
    pub value: String,
    /// Dotted path: `menu.items[0].label`.
    pub key: String,
    /// The comment block directly above the key (or array item), as plain text.
    pub comment: String,
    /// LocOS rule: the value holds at least one letter or digit.
    pub translatable: bool,
}

/// The one value-selection rule (LocOS `_is_translatable`): at least one letter or digit.
pub fn is_translatable(s: &str) -> bool {
    bilingua_model::py_has_alnum(s)
}

fn perr(text: &str, pos: usize, msg: &str) -> Error {
    let pos = pos.min(text.len());
    let mut p = pos;
    while !text.is_char_boundary(p) {
        p -= 1;
    }
    let before = &text[..p];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
    Error::Parse { format: "json5".into(), msg: format!("Invalid JSON5/JSONC at line {line} column {col}: {msg}") }
}

fn limit_err(msg: String) -> Error {
    Error::Parse { format: "json5".into(), msg: format!("This JSON5/JSONC file is over a limit: {msg}") }
}

fn is_line_end(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// JSON5 whitespace beyond ASCII: NBSP, LS, PS, BOM and the Unicode Zs category.
fn is_ws_char(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

fn char_at(text: &str, p: usize) -> Option<char> {
    text.get(p..).and_then(|s| s.chars().next())
}

/// Pre-parse budget: refuse far above any real resource file before any per-token work.
fn check_budget(text: &str, lim: &Limits) -> Result<(), Error> {
    if text.len() > lim.max_bytes {
        return Err(limit_err(format!("{} bytes (limit {})", text.len(), lim.max_bytes)));
    }
    let (mut quotes, mut others) = (0usize, 0usize);
    for &b in text.as_bytes() {
        match b {
            b'"' | b'\'' => quotes += 1,
            b',' | b'[' | b'{' | b'/' => others += 1,
            _ => {}
        }
    }
    if quotes > 40 * lim.max_units {
        return Err(limit_err(format!("far more strings than {} allowed", lim.max_units)));
    }
    if others > 20 * lim.max_units {
        return Err(limit_err(format!("far more values than {} allowed", lim.max_units)));
    }
    Ok(())
}

type Span = (usize, usize);

/// Whitespace and comments from `pos`. Comments that belong above the next token are pushed to `out` (a comment
/// on the same line as the previous token is not, unless a comment was already collected).
fn skip(text: &str, mut p: usize, out: Option<&mut Vec<Span>>) -> Result<usize, Error> {
    let b = text.as_bytes();
    let n = b.len();
    let start = p;
    let mut out = out;
    let mut saw_line_end = false;
    loop {
        if p >= n {
            return Ok(p);
        }
        match b[p] {
            b' ' | b'\t' | 0x0b | 0x0c => p += 1,
            b'\n' | b'\r' => {
                saw_line_end = true;
                p += 1;
            }
            b'/' if b.get(p + 1) == Some(&b'/') => {
                let s = p;
                p += 2;
                while p < n {
                    let Some(c) = char_at(text, p) else { break };
                    if is_line_end(c) {
                        break;
                    }
                    p += c.len_utf8();
                }
                if let Some(o) = out.as_deref_mut() {
                    if !o.is_empty() || saw_line_end || start == 0 {
                        o.push((s, p));
                    }
                }
            }
            b'/' if b.get(p + 1) == Some(&b'*') => {
                let s = p;
                let Some(close) = text[p + 2..].find("*/") else {
                    return Err(perr(text, p, "unterminated block comment"));
                };
                p = p + 2 + close + 2;
                if let Some(o) = out.as_deref_mut() {
                    if !o.is_empty() || saw_line_end || start == 0 {
                        o.push((s, p));
                    }
                }
                if text[s..p].chars().any(is_line_end) {
                    saw_line_end = true;
                }
            }
            c if c < 0x80 => return Ok(p),
            _ => {
                let Some(c) = char_at(text, p) else { return Ok(p) };
                if is_ws_char(c) {
                    if is_line_end(c) {
                        saw_line_end = true;
                    }
                    p += c.len_utf8();
                } else {
                    return Ok(p);
                }
            }
        }
    }
}

/// Splits on every line terminator `str.splitlines` knows.
fn split_lines(s: &str) -> impl Iterator<Item = &str> {
    s.split(['\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}'])
}

fn comment_text(text: &str, spans: &[Span]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for &(s, e) in spans {
        let c = &text[s..e];
        if let Some(rest) = c.strip_prefix("//") {
            lines.push(rest.trim().to_string());
        } else {
            let inner = &c[2..c.len() - 2];
            for ln in split_lines(inner) {
                let ln = ln.trim().trim_start_matches('*').trim();
                if !ln.is_empty() {
                    lines.push(ln.to_string());
                }
            }
        }
    }
    lines.join("\n")
}

/// The end of the string literal starting at `p` (a quote), or None when it does not terminate.
fn string_end(b: &[u8], p: usize) -> Option<usize> {
    let q = b[p];
    let n = b.len();
    let mut i = p + 1;
    while i < n {
        match b[i] {
            c if c == q => return Some(i + 1),
            b'\\' => {
                i += 1;
                if i >= n {
                    return None;
                }
                if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            b'\n' | b'\r' => return None,
            _ => i += 1,
        }
    }
    None
}

fn hex(b: &[u8], p: usize, len: usize) -> Option<u32> {
    let s = b.get(p..p + len)?;
    let mut v = 0u32;
    for &c in s {
        v = v * 16 + (c as char).to_digit(16)?;
    }
    Some(v)
}

/// The value of the string literal `text[start..end]` (quotes included), per JSON5 section 5.1.
fn decode_string(text: &str, start: usize, end: usize) -> Result<Cow<'_, str>, Error> {
    let body = &text[start + 1..end - 1];
    if !body.contains('\\') {
        return Ok(Cow::Borrowed(body));
    }
    let bb = body.as_bytes();
    let base = start + 1;
    let mut out = String::with_capacity(body.len());
    let mut i = 0usize;
    let mut high: Option<u32> = None;
    while i < body.len() {
        let c = body[i..].chars().next().expect("in range");
        if c != '\\' {
            if high.is_some() {
                return Err(perr(text, start, "lone UTF-16 surrogate escape"));
            }
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        let at = base + i;
        let Some(e) = body[i + 1..].chars().next() else {
            return Err(perr(text, at, "incomplete escape"));
        };
        // \uXXXX is the only escape that can produce a surrogate half.
        if e == 'u' {
            let Some(v) = hex(bb, i + 2, 4) else {
                return Err(perr(text, at, "incomplete \\u escape"));
            };
            i += 6;
            match (high, v) {
                (Some(h), 0xDC00..=0xDFFF) => {
                    let cp = 0x10000 + ((h - 0xD800) << 10) + (v - 0xDC00);
                    out.push(char::from_u32(cp).expect("valid pair"));
                    high = None;
                }
                (Some(_), _) => return Err(perr(text, start, "lone UTF-16 surrogate escape")),
                (None, 0xD800..=0xDBFF) => high = Some(v),
                (None, 0xDC00..=0xDFFF) => return Err(perr(text, start, "lone UTF-16 surrogate escape")),
                (None, _) => out.push(char::from_u32(v).expect("not a surrogate")),
            }
            continue;
        }
        // A line continuation produces nothing, so it may sit between the two halves of a pair (as in LocOS).
        if high.is_some() && !matches!(e, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
            return Err(perr(text, start, "lone UTF-16 surrogate escape"));
        }
        i += 1 + e.len_utf8();
        match e {
            'x' => {
                let Some(v) = hex(bb, i, 2) else {
                    return Err(perr(text, at, "incomplete \\x escape"));
                };
                i += 2;
                out.push(char::from_u32(v).expect("below 256"));
            }
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'v' => out.push('\u{b}'),
            '\n' | '\u{2028}' | '\u{2029}' => {}
            '\r' => {
                if bb.get(i) == Some(&b'\n') {
                    i += 1;
                }
            }
            '0' => {
                if bb.get(i).is_some_and(|d| d.is_ascii_digit()) {
                    return Err(perr(text, at, "\\0 followed by a digit"));
                }
                out.push('\0');
            }
            '1'..='9' => return Err(perr(text, at, &format!("invalid escape \\{e}"))),
            other => out.push(other),
        }
    }
    if high.is_some() {
        return Err(perr(text, start, "lone UTF-16 surrogate escape"));
    }
    Ok(Cow::Owned(out))
}

fn is_ident_start(c: char) -> bool {
    bilingua_model::py_is_word_start_char(c) || c == '$'
}

fn is_ident_part(c: char) -> bool {
    bilingua_model::py_is_word_char(c) || c == '$' || c == '\u{200c}' || c == '\u{200d}'
}

/// An unquoted key at `p`: (end, decoded name).
fn ident(text: &str, p: usize) -> Option<(usize, String)> {
    let b = text.as_bytes();
    let mut i = p;
    let mut name = String::new();
    let mut first = true;
    loop {
        if b.get(i) == Some(&b'\\') && b.get(i + 1) == Some(&b'u') {
            let v = hex(b, i + 2, 4)?;
            let c = char::from_u32(v)?;
            if (first && !is_ident_start(c)) || (!first && !is_ident_part(c)) {
                return None;
            }
            name.push(c);
            i += 6;
        } else {
            let c = char_at(text, i)?;
            if (first && !is_ident_start(c)) || (!first && !is_ident_part(c)) {
                break;
            }
            name.push(c);
            i += c.len_utf8();
        }
        first = false;
    }
    if first {
        None
    } else {
        Some((i, name))
    }
}

/// The end of a number or keyword literal at `p`: `[+-]?(Infinity|NaN|0x..|decimal)` or true, false, null.
fn literal_end(b: &[u8], p: usize) -> Option<usize> {
    let n = b.len();
    let mut i = p;
    if matches!(b.get(i), Some(b'+' | b'-')) {
        i += 1;
        if b[i..].starts_with(b"Infinity") {
            return Some(i + 8);
        }
        if b[i..].starts_with(b"NaN") {
            return Some(i + 3);
        }
        if matches!(b.get(i), Some(b'0')) && matches!(b.get(i + 1), Some(b'x' | b'X')) {
            let mut j = i + 2;
            while j < n && b[j].is_ascii_hexdigit() {
                j += 1;
            }
            if j > i + 2 {
                return Some(j);
            }
        }
        return decimal_end(b, i);
    }
    for kw in [&b"Infinity"[..], b"NaN", b"true", b"false", b"null"] {
        if b[i..].starts_with(kw) {
            return Some(i + kw.len());
        }
    }
    if matches!(b.get(i), Some(b'0')) && matches!(b.get(i + 1), Some(b'x' | b'X')) {
        let mut j = i + 2;
        while j < n && b[j].is_ascii_hexdigit() {
            j += 1;
        }
        if j > i + 2 {
            return Some(j);
        }
    }
    decimal_end(b, i)
}

fn decimal_end(b: &[u8], p: usize) -> Option<usize> {
    let n = b.len();
    let mut i = p;
    let digits = |mut j: usize| {
        while j < n && b[j].is_ascii_digit() {
            j += 1;
        }
        j
    };
    let j = digits(i);
    if j > i {
        i = j;
        if b.get(i) == Some(&b'.') {
            i += 1;
            i = digits(i);
        }
    } else if b.get(i) == Some(&b'.') {
        let k = digits(i + 1);
        if k == i + 1 {
            return None;
        }
        i = k;
    } else {
        return None;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut k = i + 1;
        if matches!(b.get(k), Some(b'+' | b'-')) {
            k += 1;
        }
        let m = digits(k);
        if m > k {
            i = m;
        }
    }
    Some(i)
}

enum Expect {
    Value,
    ValueOrClose,
    KeyOrClose,
    After,
}

enum Seg<'a> {
    None,
    Key(Cow<'a, str>),
    Index(usize),
}

struct Frame {
    open: u8,
    /// Dotted path of this container.
    base: String,
    count: usize,
}

fn join_path(base: &str, seg: &Seg) -> String {
    match seg {
        Seg::None => base.to_string(),
        Seg::Index(i) => format!("{base}[{i}]"),
        Seg::Key(k) => {
            if base.is_empty() {
                k.to_string()
            } else {
                format!("{base}.{k}")
            }
        }
    }
}

/// Validates `text` and returns every string value (translatable or not) in document order.
pub fn scan<'a>(text: &'a str, lim: &Limits) -> Result<Vec<StrVal>, Error> {
    check_budget(text, lim)?;
    let b = text.as_bytes();
    let n = b.len();
    let mut out: Vec<StrVal> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut pending: Vec<Span> = Vec::new();
    let mut comment: Vec<Span> = Vec::new();
    let mut seg: Seg<'a> = Seg::None;
    let mut expect = Expect::Value;
    let mut translatable = 0usize;
    let mut pos = skip(text, 0, Some(&mut pending))?;
    loop {
        let c = b.get(pos).copied();
        match expect {
            Expect::Value => {
                match c {
                    Some(o @ (b'{' | b'[')) => {
                        if stack.len() >= lim.max_depth {
                            return Err(limit_err(format!("nested deeper than {}", lim.max_depth)));
                        }
                        let base = match stack.last() {
                            Some(top) => join_path(&top.base, &seg),
                            None => String::new(),
                        };
                        stack.push(Frame { open: o, base, count: 0 });
                        pos += 1;
                        expect = if o == b'{' { Expect::KeyOrClose } else { Expect::ValueOrClose };
                    }
                    Some(b'"' | b'\'') => {
                        let Some(end) = string_end(b, pos) else {
                            return Err(perr(text, pos, "unterminated string"));
                        };
                        let value = decode_string(text, pos, end)?.into_owned();
                        let ok = is_translatable(&value);
                        if ok {
                            translatable += 1;
                            if translatable > lim.max_units {
                                return Err(limit_err(format!("more than {} strings", lim.max_units)));
                            }
                            if value.len() > lim.max_unit_chars && value.chars().count() > lim.max_unit_chars {
                                return Err(limit_err(format!("a string longer than {} characters", lim.max_unit_chars)));
                            }
                        }
                        let key = match stack.last() {
                            Some(top) => join_path(&top.base, &seg),
                            None => String::new(),
                        };
                        out.push(StrVal { start: pos, end, value, key, comment: comment_text(text, &comment), translatable: ok });
                        pos = end;
                        expect = Expect::After;
                    }
                    _ => {
                        let Some(e) = literal_end(b, pos) else {
                            return Err(perr(text, pos, "expected a value"));
                        };
                        if let Some(nx) = char_at(text, e) {
                            if bilingua_model::py_is_word_char(nx) || nx == '$' {
                                return Err(perr(text, pos, "expected a value"));
                            }
                        }
                        pos = e;
                        expect = Expect::After;
                    }
                }
                comment.clear();
                pending.clear();
            }
            Expect::ValueOrClose => {
                let top = stack.last().expect("inside an array");
                if c == Some(b']') {
                    stack.pop();
                    pos += 1;
                    expect = Expect::After;
                    pending.clear();
                } else {
                    seg = Seg::Index(top.count);
                    comment = std::mem::take(&mut pending);
                    expect = Expect::Value;
                    continue;
                }
            }
            Expect::KeyOrClose => {
                if c == Some(b'}') {
                    stack.pop();
                    pos += 1;
                    expect = Expect::After;
                    pending.clear();
                } else {
                    comment = std::mem::take(&mut pending);
                    let key: Cow<'a, str>;
                    match c {
                        Some(q @ (b'"' | b'\'')) => {
                            let _ = q;
                            let Some(end) = string_end(b, pos) else {
                                return Err(perr(text, pos, "unterminated string"));
                            };
                            key = decode_string(text, pos, end)?;
                            pos = end;
                        }
                        _ => {
                            let Some((end, name)) = ident(text, pos) else {
                                return Err(perr(text, pos, "expected a key or '}'"));
                            };
                            key = Cow::Owned(name);
                            pos = end;
                        }
                    }
                    pos = skip(text, pos, None)?;
                    if b.get(pos) != Some(&b':') {
                        return Err(perr(text, pos, "expected ':' after a key"));
                    }
                    pos += 1;
                    seg = Seg::Key(key);
                    expect = Expect::Value;
                }
            }
            Expect::After => {
                let Some(top) = stack.last_mut() else {
                    if pos < n {
                        return Err(perr(text, pos, "unexpected content after the top-level value"));
                    }
                    return Ok(out);
                };
                if c == Some(b',') {
                    top.count += 1;
                    pos += 1;
                    expect = if top.open == b'{' { Expect::KeyOrClose } else { Expect::ValueOrClose };
                    pending.clear();
                } else if (c == Some(b'}') && top.open == b'{') || (c == Some(b']') && top.open == b'[') {
                    stack.pop();
                    pos += 1;
                    pending.clear();
                } else {
                    return Err(perr(text, pos, "expected ',' or a closing bracket"));
                }
            }
        }
        pos = skip(text, pos, Some(&mut pending))?;
    }
}

/// The JSON5 string literal for `value` in the given quote style. Only JSON escapes are used, so a
/// double-quoted result is valid JSON and a single-quoted one valid JSON5. U+2028 and U+2029 are escaped.
pub fn quote(value: &str, q: u8) -> String {
    let qc = q as char;
    let mut out = String::with_capacity(value.len() + 2);
    out.push(qc);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            c if c == qc => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < ' ' || c == '\u{2028}' || c == '\u{2029}' || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(qc);
    out
}
