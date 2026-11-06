//! A small order-keeping JSON value for the rich text filters.
//!
//! The filters need three things `serde_json::Value` (without its `preserve_order` feature) does not give: object keys
//! in file order, number spellings kept as written (`1.0`, `1e5`, `12345678901234567890`), and the file's layout
//! (indent, separators, trailing newline, `\u` escapes) so a rebuilt tree is written the way the file was. Duplicate
//! keys follow Python's `json.loads`, which Localization OS uses: the last value wins at the first key's place.

use bilingua_model::Error;
use std::collections::BTreeMap;

/// Deepest nesting read; deeper input is refused with a typed error (never a stack overflow).
pub(crate) const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    /// The number as written.
    Num(String),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Value::Obj(m) => m.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_arr_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn is_obj(&self) -> bool {
        matches!(self, Value::Obj(_))
    }

    /// `d.get(key) == Str(s)` for an object.
    pub fn has_str(&self, key: &str, s: &str) -> bool {
        self.get(key).and_then(Value::as_str) == Some(s)
    }

    /// Sets `key`: replaces the value in place, else appends (Python's dict assignment).
    pub fn set(&mut self, key: &str, v: Value) {
        if let Value::Obj(m) = self {
            if let Some(slot) = m.iter_mut().find(|(k, _)| k == key) {
                slot.1 = v;
            } else {
                m.push((key.to_string(), v));
            }
        }
    }

    /// Python truthiness: `None`, `False`, `0`, `""`, `[]` and `{}` are false.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Num(n) => !is_zero(n),
            Value::Str(s) => !s.is_empty(),
            Value::Arr(a) => !a.is_empty(),
            Value::Obj(m) => !m.is_empty(),
        }
    }

    /// A copy without one key (Python's `{k: v for k, v in d.items() if k != key}`).
    pub fn without(&self, key: &str) -> Value {
        match self {
            Value::Obj(m) => Value::Obj(m.iter().filter(|(k, _)| k != key).cloned().collect()),
            other => other.clone(),
        }
    }
}

fn is_zero(n: &str) -> bool {
    let mant = n.split(['e', 'E']).next().unwrap_or("");
    mant.bytes().all(|b| matches!(b, b'0' | b'.' | b'-'))
}

/// How the file was laid out, so a rebuilt tree is written the same way.
#[derive(Debug, Clone, Default)]
pub(crate) struct Style {
    /// Whitespace before the root value (after the BOM).
    pub lead: String,
    /// Whitespace after the root value (the trailing newline).
    pub tail: String,
    /// Inside the root: a newline follows the opening bracket.
    pub pretty: bool,
    /// One indent step (`"  "`, `"    "`, `"\t"`).
    pub indent: String,
    pub crlf: bool,
    /// Whitespace after a colon (`" "` or `""`).
    pub colon: String,
    /// Whitespace after a comma in a one line file (`" "` or `""`).
    pub comma: String,
    /// Non-ASCII characters are written as `\uXXXX` (Python's `ensure_ascii`).
    pub ascii: bool,
}

pub(crate) struct Parsed {
    pub value: Value,
    pub style: Style,
}

struct P<'a> {
    b: &'a [u8],
    s: &'a str,
    i: usize,
    st: Style,
    got_layout: bool,
    got_colon: bool,
    got_comma: bool,
    esc_ascii: bool,
    literal_non_ascii: bool,
}

fn err(msg: impl Into<String>) -> Error {
    Error::Parse { format: "json".into(), msg: msg.into() }
}

pub(crate) fn parse(text: &str) -> Result<Parsed, Error> {
    let mut p = P { b: text.as_bytes(), s: text, i: 0, st: Style::default(), got_layout: false, got_colon: false, got_comma: false, esc_ascii: false, literal_non_ascii: false };
    let lead = p.ws();
    let value = p.value(0)?;
    let tail = p.ws();
    if p.i != p.b.len() {
        return Err(err(format!("unexpected data after the JSON value at byte {}", p.i)));
    }
    p.st.lead = lead.to_string();
    p.st.tail = tail.to_string();
    p.st.ascii = p.esc_ascii && !p.literal_non_ascii;
    if !p.got_colon {
        p.st.colon = " ".into();
    }
    if !p.got_comma {
        p.st.comma = " ".into();
    }
    Ok(Parsed { value, style: p.st })
}

impl<'a> P<'a> {
    fn ws(&mut self) -> &'a str {
        let a = self.i;
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
        &self.s[a..self.i]
    }

    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        if depth > MAX_DEPTH {
            return Err(err(format!("nesting deeper than {MAX_DEPTH}")));
        }
        match self.b.get(self.i) {
            None => Err(err("the file ends before the JSON value does")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') => self.lit("true", Value::Bool(true)),
            Some(b'f') => self.lit("false", Value::Bool(false)),
            Some(b'n') => self.lit("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(c) => Err(err(format!("unexpected byte 0x{c:02x} at {}", self.i))),
        }
    }

    fn lit(&mut self, word: &str, v: Value) -> Result<Value, Error> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(err(format!("bad literal at byte {}", self.i)))
        }
    }

    fn number(&mut self) -> Result<Value, Error> {
        let a = self.i;
        let b = self.b;
        let mut i = self.i;
        if b.get(i) == Some(&b'-') {
            i += 1;
        }
        match b.get(i) {
            Some(b'0') => i += 1,
            Some(b'1'..=b'9') => {
                while matches!(b.get(i), Some(b'0'..=b'9')) {
                    i += 1;
                }
            }
            _ => return Err(err(format!("bad number at byte {a}"))),
        }
        if b.get(i) == Some(&b'.') {
            i += 1;
            if !matches!(b.get(i), Some(b'0'..=b'9')) {
                return Err(err(format!("bad number at byte {a}")));
            }
            while matches!(b.get(i), Some(b'0'..=b'9')) {
                i += 1;
            }
        }
        if matches!(b.get(i), Some(b'e' | b'E')) {
            i += 1;
            if matches!(b.get(i), Some(b'+' | b'-')) {
                i += 1;
            }
            if !matches!(b.get(i), Some(b'0'..=b'9')) {
                return Err(err(format!("bad number at byte {a}")));
            }
            while matches!(b.get(i), Some(b'0'..=b'9')) {
                i += 1;
            }
        }
        self.i = i;
        Ok(Value::Num(self.s[a..i].to_string()))
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let h = self.s.get(self.i..self.i + 4).ok_or_else(|| err("a \\u escape is cut short"))?;
        if !h.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(err("bad \\u escape"));
        }
        self.i += 4;
        u32::from_str_radix(h, 16).map_err(|_| err("bad \\u escape"))
    }

    fn string(&mut self) -> Result<String, Error> {
        self.i += 1; // opening quote
        let mut out = String::new();
        loop {
            let a = self.i;
            while self.i < self.b.len() && !matches!(self.b[self.i], b'"' | b'\\' | 0..=0x1f) {
                self.i += 1;
            }
            let chunk = &self.s[a..self.i];
            if !chunk.is_ascii() {
                self.literal_non_ascii = true;
            }
            out.push_str(chunk);
            match self.b.get(self.i) {
                None => return Err(err("a string is not closed")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(0..=0x1f) => return Err(err(format!("a raw control character in a string at byte {}", self.i))),
                Some(_) => {
                    self.i += 1;
                    let c = *self.b.get(self.i).ok_or_else(|| err("a string is not closed"))?;
                    self.i += 1;
                    match c {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                if self.b.get(self.i) == Some(&b'\\') && self.b.get(self.i + 1) == Some(&b'u') {
                                    self.i += 2;
                                    let lo = self.hex4()?;
                                    if !(0xDC00..0xE000).contains(&lo) {
                                        return Err(err("a lone surrogate in a \\u escape"));
                                    }
                                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                } else {
                                    return Err(err("a lone surrogate in a \\u escape"));
                                }
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return Err(err("a lone surrogate in a \\u escape"));
                            } else {
                                hi
                            };
                            if cp >= 0x80 {
                                self.esc_ascii = true;
                            }
                            out.push(char::from_u32(cp).ok_or_else(|| err("bad \\u escape"))?);
                        }
                        _ => return Err(err(format!("bad escape at byte {}", self.i - 1))),
                    }
                }
            }
        }
    }

    fn layout_after_open(&mut self, ws: &str, empty: bool) {
        if self.got_layout || empty {
            return;
        }
        self.got_layout = true;
        if let Some(nl) = ws.rfind('\n') {
            self.st.pretty = true;
            self.st.indent = ws[nl + 1..].to_string();
            self.st.crlf = ws[..nl].ends_with('\r');
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, Error> {
        self.i += 1;
        let ws = self.ws();
        let mut items = Vec::new();
        if self.b.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Value::Arr(items));
        }
        if depth == 0 {
            self.layout_after_open(ws, false);
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => {
                    self.i += 1;
                    let w = self.ws();
                    if !self.got_comma && !w.contains('\n') {
                        self.got_comma = true;
                        self.st.comma = w.to_string();
                    }
                }
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(err(format!("expected , or ] at byte {}", self.i))),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, Error> {
        self.i += 1;
        let ws = self.ws();
        let mut items: Vec<(String, Value)> = Vec::new();
        let mut index: BTreeMap<String, usize> = BTreeMap::new();
        if self.b.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Value::Obj(items));
        }
        if depth == 0 {
            self.layout_after_open(ws, false);
        }
        loop {
            if self.b.get(self.i) != Some(&b'"') {
                return Err(err(format!("expected a key at byte {}", self.i)));
            }
            let key = self.string()?;
            self.ws();
            if self.b.get(self.i) != Some(&b':') {
                return Err(err(format!("expected : at byte {}", self.i)));
            }
            self.i += 1;
            let w = self.ws();
            if !self.got_colon {
                self.got_colon = true;
                self.st.colon = w.to_string();
            }
            let v = self.value(depth + 1)?;
            match index.get(&key) {
                Some(&at) => items[at].1 = v,
                None => {
                    index.insert(key.clone(), items.len());
                    items.push((key, v));
                }
            }
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => {
                    self.i += 1;
                    let w = self.ws();
                    if !self.got_comma && !w.contains('\n') {
                        self.got_comma = true;
                        self.st.comma = w.to_string();
                    }
                }
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Obj(items));
                }
                _ => return Err(err(format!("expected , or }} at byte {}", self.i))),
            }
        }
    }
}

/// The value as text in the file's layout (without the leading and trailing whitespace of the file).
pub(crate) fn write(v: &Value, st: &Style) -> String {
    let mut out = String::new();
    put(v, 0, st, &mut out);
    out
}

fn nl(st: &Style, depth: usize, out: &mut String) {
    out.push_str(if st.crlf { "\r\n" } else { "\n" });
    for _ in 0..depth {
        out.push_str(&st.indent);
    }
}

fn put(v: &Value, depth: usize, st: &Style, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => out.push_str(n),
        Value::Str(s) => quote(s, st.ascii, out),
        Value::Arr(a) if a.is_empty() => out.push_str("[]"),
        Value::Obj(m) if m.is_empty() => out.push_str("{}"),
        Value::Arr(a) => {
            out.push('[');
            for (k, x) in a.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                    if !st.pretty {
                        out.push_str(&st.comma);
                    }
                }
                if st.pretty {
                    nl(st, depth + 1, out);
                }
                put(x, depth + 1, st, out);
            }
            if st.pretty {
                nl(st, depth, out);
            }
            out.push(']');
        }
        Value::Obj(m) => {
            out.push('{');
            for (k, (key, x)) in m.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                    if !st.pretty {
                        out.push_str(&st.comma);
                    }
                }
                if st.pretty {
                    nl(st, depth + 1, out);
                }
                quote(key, st.ascii, out);
                out.push(':');
                out.push_str(&st.colon);
                put(x, depth + 1, st, out);
            }
            if st.pretty {
                nl(st, depth, out);
            }
            out.push('}');
        }
    }
}

fn quote(s: &str, ascii: bool, out: &mut String) {
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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if ascii && (c as u32) >= 0x80 => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
