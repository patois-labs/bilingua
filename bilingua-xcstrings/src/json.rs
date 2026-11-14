//! A small JSON reader that remembers where every value sits in the text, and a writer in the style of the file.
//!
//! The String Catalog filter never re-serialises a whole file: it reads the tree, decides what changes, and
//! splices new text into the original at the recorded byte offsets. Everything outside a splice is therefore
//! the original bytes.
//!
//! The reader accepts what Python's `json.loads` accepts (the Localization OS reader): strict strings (no raw
//! control characters), `NaN`, `Infinity` and `-Infinity`, any depth up to the caller's cap.

use std::collections::HashSet;

/// A value and the byte range `start..end` it occupies in the text.
#[derive(Debug, Clone)]
pub struct Node {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub enum Kind {
    Obj(Obj),
    Arr(Vec<Node>),
    Str(String),
    /// A number, `true`, `false` or `null`.
    Other,
}

#[derive(Debug, Clone)]
pub struct Obj {
    pub members: Vec<Member>,
    /// A key appears twice (Python keeps the first position and the last value).
    pub dup: bool,
}

#[derive(Debug, Clone)]
pub struct Member {
    pub key: String,
    pub key_start: usize,
    pub val: Node,
}

#[derive(Debug)]
pub enum JsonError {
    Syntax(String),
    Deep,
}

impl Node {
    pub fn obj(&self) -> Option<&Obj> {
        match &self.kind {
            Kind::Obj(o) => Some(o),
            _ => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match &self.kind {
            Kind::Str(s) => Some(s),
            _ => None,
        }
    }
}

impl Obj {
    /// The value of `key` (the last one when the key repeats).
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.members.iter().rev().find(|m| m.key == key).map(|m| &m.val)
    }

    /// Python `dict.items()`: first position, last value.
    pub fn items(&self) -> Vec<(&str, &Node)> {
        if !self.dup {
            return self.members.iter().map(|m| (m.key.as_str(), &m.val)).collect();
        }
        let mut out: Vec<(&str, &Node)> = Vec::with_capacity(self.members.len());
        for m in &self.members {
            match out.iter_mut().find(|(k, _)| *k == m.key) {
                Some(slot) => slot.1 = &m.val,
                None => out.push((m.key.as_str(), &m.val)),
            }
        }
        out
    }
}

pub fn parse(text: &str, max_depth: usize) -> Result<Node, JsonError> {
    let mut p = P { b: text.as_bytes(), t: text, i: 0, max_depth };
    p.ws();
    let n = p.value(0)?;
    p.ws();
    if p.i != p.b.len() {
        return Err(p.err("extra data after the value"));
    }
    Ok(n)
}

struct P<'a> {
    b: &'a [u8],
    t: &'a str,
    i: usize,
    max_depth: usize,
}

impl P<'_> {
    fn err(&self, msg: &str) -> JsonError {
        let line = self.b[..self.i.min(self.b.len())].iter().filter(|c| **c == b'\n').count() + 1;
        JsonError::Syntax(format!("{msg} at byte {} (line {line})", self.i))
    }

    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, JsonError> {
        let start = self.i;
        let Some(&c) = self.b.get(self.i) else { return Err(self.err("unexpected end of data")) };
        let kind = match c {
            b'{' => {
                if depth + 1 > self.max_depth {
                    return Err(JsonError::Deep);
                }
                self.i += 1;
                let mut members: Vec<Member> = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        if self.b.get(self.i) != Some(&b'"') {
                            return Err(self.err("expected a string key"));
                        }
                        let key_start = self.i;
                        let key = self.string()?;
                        self.ws();
                        if self.b.get(self.i) != Some(&b':') {
                            return Err(self.err("expected ':'"));
                        }
                        self.i += 1;
                        self.ws();
                        let val = self.value(depth + 1)?;
                        members.push(Member { key, key_start, val });
                        self.ws();
                        match self.b.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b'}') => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(self.err("expected ',' or '}'")),
                        }
                    }
                }
                let dup = has_dup(&members);
                Kind::Obj(Obj { members, dup })
            }
            b'[' => {
                if depth + 1 > self.max_depth {
                    return Err(JsonError::Deep);
                }
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        items.push(self.value(depth + 1)?);
                        self.ws();
                        match self.b.get(self.i) {
                            Some(b',') => self.i += 1,
                            Some(b']') => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(self.err("expected ',' or ']'")),
                        }
                    }
                }
                Kind::Arr(items)
            }
            b'"' => Kind::Str(self.string()?),
            b't' => self.lit(b"true")?,
            b'f' => self.lit(b"false")?,
            b'n' => self.lit(b"null")?,
            b'N' => self.lit(b"NaN")?,
            b'I' => self.lit(b"Infinity")?,
            b'-' | b'0'..=b'9' => self.number()?,
            _ => return Err(self.err("unexpected character")),
        };
        Ok(Node { start, end: self.i, kind })
    }

    fn lit(&mut self, word: &[u8]) -> Result<Kind, JsonError> {
        if self.b[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(Kind::Other)
        } else {
            Err(self.err("unexpected literal"))
        }
    }

    fn number(&mut self) -> Result<Kind, JsonError> {
        if self.b[self.i..].starts_with(b"-Infinity") {
            self.i += 9;
            return Ok(Kind::Other);
        }
        if self.b.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        match self.b.get(self.i) {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                    self.i += 1;
                }
            }
            _ => return Err(self.err("bad number")),
        }
        if self.b.get(self.i) == Some(&b'.') && self.b.get(self.i + 1).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
            while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                self.i += 1;
            }
        }
        if matches!(self.b.get(self.i), Some(b'e' | b'E')) {
            let mut j = self.i + 1;
            if matches!(self.b.get(j), Some(b'+' | b'-')) {
                j += 1;
            }
            if self.b.get(j).is_some_and(u8::is_ascii_digit) {
                while self.b.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                self.i = j;
            }
        }
        Ok(Kind::Other)
    }

    /// Reads a string starting at the opening quote and returns its decoded value.
    fn string(&mut self) -> Result<String, JsonError> {
        self.i += 1;
        let start = self.i;
        // Fast path: no escapes.
        let mut j = start;
        while j < self.b.len() {
            match self.b[j] {
                b'"' => {
                    self.i = j + 1;
                    return Ok(self.t[start..j].to_string());
                }
                b'\\' => break,
                c if c < 0x20 => {
                    self.i = j;
                    return Err(self.err("control character inside a string"));
                }
                _ => j += 1,
            }
        }
        let mut out = String::from(&self.t[start..j]);
        while j < self.b.len() {
            match self.b[j] {
                b'"' => {
                    self.i = j + 1;
                    return Ok(out);
                }
                b'\\' => {
                    j += 1;
                    let Some(&e) = self.b.get(j) else { break };
                    j += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4(j)?;
                            j += 4;
                            let mut cp = hi as u32;
                            if (0xD800..0xDC00).contains(&hi) && self.b.get(j) == Some(&b'\\') && self.b.get(j + 1) == Some(&b'u') {
                                let lo = self.hex4(j + 2)?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    cp = 0x10000 + (((hi as u32) - 0xD800) << 10) + ((lo as u32) - 0xDC00);
                                    j += 6;
                                }
                            }
                            // A lone surrogate cannot be text in Rust; it becomes U+FFFD.
                            out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                        }
                        _ => {
                            self.i = j - 1;
                            return Err(self.err("bad escape"));
                        }
                    }
                }
                c if c < 0x20 => {
                    self.i = j;
                    return Err(self.err("control character inside a string"));
                }
                _ => {
                    // copy a run of plain bytes at once
                    let run_start = j;
                    while j < self.b.len() && !matches!(self.b[j], b'"' | b'\\') && self.b[j] >= 0x20 {
                        j += 1;
                    }
                    out.push_str(&self.t[run_start..j]);
                }
            }
        }
        self.i = self.b.len();
        Err(self.err("unterminated string"))
    }

    fn hex4(&self, at: usize) -> Result<u16, JsonError> {
        let h = self.b.get(at..at + 4).ok_or_else(|| JsonError::Syntax("truncated \\u escape".into()))?;
        let s = std::str::from_utf8(h).map_err(|_| JsonError::Syntax("bad \\u escape".into()))?;
        if !h.iter().all(u8::is_ascii_hexdigit) {
            return Err(JsonError::Syntax("bad \\u escape".into()));
        }
        u16::from_str_radix(s, 16).map_err(|_| JsonError::Syntax("bad \\u escape".into()))
    }
}

fn has_dup(members: &[Member]) -> bool {
    if members.len() < 2 {
        return false;
    }
    if members.len() <= 12 {
        return members.iter().enumerate().any(|(i, m)| members[..i].iter().any(|o| o.key == m.key));
    }
    let mut seen: HashSet<&str> = HashSet::with_capacity(members.len());
    members.iter().any(|m| !seen.insert(m.key.as_str()))
}

// ---------------------------------------------------------------------------------------------------------
// A value to write: used for text the filter builds (a new locale) and for old leaves it keeps.

#[derive(Debug, Clone)]
pub enum J {
    Obj(Vec<(String, J)>),
    Arr(Vec<J>),
    Str(String),
    /// A number or literal, as it was written.
    Raw(String),
}

impl J {
    /// Copies a parsed value (Python `dict` semantics for repeated keys).
    pub fn from_node(text: &str, n: &Node) -> J {
        match &n.kind {
            Kind::Obj(o) => J::Obj(o.items().into_iter().map(|(k, v)| (k.to_string(), J::from_node(text, v))).collect()),
            Kind::Arr(a) => J::Arr(a.iter().map(|v| J::from_node(text, v)).collect()),
            Kind::Str(s) => J::Str(s.clone()),
            Kind::Other => J::Raw(text[n.start..n.end].to_string()),
        }
    }

    pub fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Xcode writes every object with sorted keys (dicts only; lists are left alone).
    pub fn sorted_deep(&self) -> J {
        match self {
            J::Obj(m) => {
                let mut v: Vec<(String, J)> = m.iter().map(|(k, x)| (k.clone(), x.sorted_deep())).collect();
                v.sort_by(|a, b| a.0.cmp(&b.0));
                J::Obj(v)
            }
            o => o.clone(),
        }
    }
}

/// Python `==` on parsed JSON: object key order does not matter.
pub fn same(a: &J, b: &J) -> bool {
    match (a, b) {
        (J::Obj(x), J::Obj(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.iter().any(|(k2, v2)| k == k2 && same(v, v2))),
        (J::Arr(x), J::Arr(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
        (J::Str(x), J::Str(y)) => x == y,
        (J::Raw(x), J::Raw(y)) => x == y,
        _ => false,
    }
}

/// How a file lays out its JSON, read from the text (Localization OS `_dump`).
#[derive(Debug, Clone)]
pub struct Style {
    /// `None` = one line; otherwise the indent unit ("  " or a tab).
    pub indent: Option<String>,
    pub item_sep: String,
    pub key_sep: String,
    pub ensure_ascii: bool,
    pub escape_slash: bool,
    /// The line ending the file uses (LF or CRLF).
    pub eol: &'static str,
    /// Write a number or literal the way Python's `json.dumps` prints the value it read (`1.50` as `1.5`, `1e5` as
    /// `100000.0`, `-0` as `0`), not as it was written.
    pub py_numbers: bool,
}

impl Style {
    pub fn detect(text: &str) -> Style {
        Style::detect_with(text, false)
    }

    /// The style as Localization OS reads it: lines are Python's `splitlines` lines, and numbers are written as
    /// Python prints them.
    pub fn detect_locos(text: &str) -> Style {
        let mut st = Style::detect_with(text, true);
        st.py_numbers = true;
        st
    }

    fn detect_with(text: &str, python_lines: bool) -> Style {
        let ensure_ascii = text.is_ascii() && text.contains("\\u");
        let trimmed = text.trim_matches(bilingua_model::py_isspace_char);
        let second = if python_lines { py_splitlines(trimmed).get(1).copied() } else { split_lines(trimmed).get(1).copied() };
        let (indent, item_sep, key_sep) = match second {
            Some(second) => {
                let key_sep = if text.contains("\" : ") { " : " } else { ": " };
                let indent = if second.starts_with('\t') {
                    "\t".to_string()
                } else {
                    let n = second.len() - second.trim_start_matches(' ').len();
                    " ".repeat(if n == 0 { 2 } else { n })
                };
                (Some(indent), ",", key_sep)
            }
            None => {
                if text.contains("\":") && !text.contains("\": ") {
                    (None, ",", ":")
                } else {
                    (None, ", ", ": ")
                }
            }
        };
        let crlf = text.find('\n').is_some_and(|i| i > 0 && text.as_bytes()[i - 1] == b'\r');
        let eol = if crlf { "\r\n" } else { "\n" };
        Style { indent, item_sep: item_sep.into(), key_sep: key_sep.into(), ensure_ascii, escape_slash: text.contains("\\/"), eol, py_numbers: false }
    }

    fn nl(&self, depth: usize, out: &mut String) {
        if let Some(i) = &self.indent {
            out.push_str(self.eol);
            for _ in 0..depth {
                out.push_str(i);
            }
        }
    }

    /// A string literal with its quotes.
    pub fn string(&self, s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                '/' if self.escape_slash => out.push_str("\\/"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c if self.ensure_ascii && !(' '..='~').contains(&c) => {
                    let mut buf = [0u16; 2];
                    for u in c.encode_utf16(&mut buf) {
                        out.push_str(&format!("\\u{u:04x}"));
                    }
                }
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    /// `value` written at nesting `depth` (the depth of the object or array it is).
    pub fn emit(&self, v: &J, depth: usize, out: &mut String) {
        match v {
            J::Str(s) => out.push_str(&self.string(s)),
            J::Raw(r) if self.py_numbers => out.push_str(&py_number(r)),
            J::Raw(r) => out.push_str(r),
            J::Obj(m) if m.is_empty() => out.push_str("{}"),
            J::Arr(a) if a.is_empty() => out.push_str("[]"),
            J::Obj(m) => {
                out.push('{');
                for (i, (k, x)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push_str(&self.item_sep);
                    }
                    self.nl(depth + 1, out);
                    out.push_str(&self.string(k));
                    out.push_str(&self.key_sep);
                    self.emit(x, depth + 1, out);
                }
                self.nl(depth, out);
                out.push('}');
            }
            J::Arr(a) => {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push_str(&self.item_sep);
                    }
                    self.nl(depth + 1, out);
                    self.emit(x, depth + 1, out);
                }
                self.nl(depth, out);
                out.push(']');
            }
        }
    }

    /// The text of a member `"key": value` for an object at `depth`.
    pub fn member(&self, key: &str, v: &J, depth: usize) -> String {
        let mut s = self.string(key);
        s.push_str(&self.key_sep);
        self.emit(v, depth + 1, &mut s);
        s
    }

    /// The separator between two members of an object at `depth` (comma and white space).
    pub fn separator(&self, depth: usize) -> String {
        let mut s = self.item_sep.clone();
        self.nl(depth + 1, &mut s);
        s
    }
}

/// Lines the way Python's `splitlines` sees the common cases (`\n`, `\r\n`, `\r`).
fn split_lines(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\n' || b[i] == b'\r' {
            out.push(&s[start..i]);
            if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < b.len() {
        out.push(&s[start..]);
    }
    out
}

/// Python's `str.splitlines`: a line ends at `\n`, `\r`, `\r\n`, and at VT, FF, FS, GS, RS, NEL, LS and PS.
fn py_splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if matches!(c, '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}') {
            out.push(&s[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r' && it.peek().is_some_and(|(_, n)| *n == '\n') {
                it.next();
                end += 1;
            }
            start = end;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// A number or literal as Python's `json.dumps` prints what `json.loads` read: an integer keeps its digits (`-0` is
/// `0`), a number with a fraction or an exponent is a float printed with `repr`, and `NaN` and the infinities keep
/// their JSON-extension names.
fn py_number(raw: &str) -> String {
    if matches!(raw, "NaN" | "Infinity" | "-Infinity" | "true" | "false" | "null") {
        return raw.to_string();
    }
    if !raw.contains(['.', 'e', 'E']) {
        return if raw == "-0" { "0".to_string() } else { raw.to_string() };
    }
    match raw.parse::<f64>() {
        Ok(f) => py_float_repr(f),
        Err(_) => raw.to_string(),
    }
}

/// Python's `float.__repr__`: the shortest digits that read back, in fixed notation when the decimal point falls
/// within 16 places to the right or 4 to the left of the first digit, else as `d.ddde+XX`.
fn py_float_repr(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    let sign = if f.is_sign_negative() { "-" } else { "" };
    if f == 0.0 {
        return format!("{sign}0.0");
    }
    let sci = format!("{:e}", f.abs());
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let n = digits.len() as i32;
    let decpt = exp + 1;
    let body = if decpt > 16 || decpt <= -4 {
        let (first, rest) = digits.split_at(1);
        let m = if rest.is_empty() { first.to_string() } else { format!("{first}.{rest}") };
        let e = decpt - 1;
        format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    } else if decpt <= 0 {
        format!("0.{}{digits}", "0".repeat((-decpt) as usize))
    } else if decpt >= n {
        format!("{digits}{}.0", "0".repeat((decpt - n) as usize))
    } else {
        let (a, b) = digits.split_at(decpt as usize);
        format!("{a}.{b}")
    };
    format!("{sign}{body}")
}
