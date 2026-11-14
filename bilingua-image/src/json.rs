//! A small JSON reader that keeps the byte span of every value, so a string can be replaced without touching the
//! rest of the document. It accepts what Python's `json.loads` accepts that matters here: `NaN`, `Infinity`,
//! `-Infinity`, duplicate keys (the last wins) and arbitrarily long integers.

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Node>),
    Obj(Vec<(String, Node)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub v: Val,
    /// Byte span of the value in the source text.
    pub span: (usize, usize),
}

impl Node {
    /// The last value stored under `key` of an object.
    pub fn get(&self, key: &str) -> Option<&Node> {
        match &self.v {
            Val::Obj(m) => m.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

const MAX_DEPTH: usize = 64;

struct P<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
}

pub fn parse(s: &str) -> Option<Node> {
    let mut p = P { s, b: s.as_bytes(), i: 0 };
    p.ws();
    let n = p.value(0)?;
    p.ws();
    (p.i == p.b.len()).then_some(n)
}

impl P<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn lit(&mut self, w: &str) -> bool {
        if self.s[self.i..].starts_with(w) {
            self.i += w.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Option<Node> {
        if depth > MAX_DEPTH {
            return None;
        }
        let start = self.i;
        let v = match *self.b.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut m = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        let k = self.string()?;
                        self.ws();
                        if self.b.get(self.i) != Some(&b':') {
                            return None;
                        }
                        self.i += 1;
                        self.ws();
                        let v = self.value(depth + 1)?;
                        m.push((k, v));
                        self.ws();
                        match self.b.get(self.i)? {
                            b',' => self.i += 1,
                            b'}' => {
                                self.i += 1;
                                break;
                            }
                            _ => return None,
                        }
                    }
                }
                Val::Obj(m)
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        a.push(self.value(depth + 1)?);
                        self.ws();
                        match self.b.get(self.i)? {
                            b',' => self.i += 1,
                            b']' => {
                                self.i += 1;
                                break;
                            }
                            _ => return None,
                        }
                    }
                }
                Val::Arr(a)
            }
            b'"' => Val::Str(self.string()?),
            b't' if self.lit("true") => Val::Bool(true),
            b'f' if self.lit("false") => Val::Bool(false),
            b'n' if self.lit("null") => Val::Null,
            b'N' if self.lit("NaN") => Val::Num(f64::NAN),
            b'I' if self.lit("Infinity") => Val::Num(f64::INFINITY),
            b'-' if self.lit("-Infinity") => Val::Num(f64::NEG_INFINITY),
            b'-' | b'0'..=b'9' => Val::Num(self.number()?),
            _ => return None,
        };
        Some(Node { v, span: (start, self.i) })
    }

    fn number(&mut self) -> Option<f64> {
        let st = self.i;
        if self.b.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        match self.b.get(self.i)? {
            b'0' => self.i += 1,
            b'1'..=b'9' => {
                while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                    self.i += 1;
                }
            }
            _ => return None,
        }
        if self.b.get(self.i) == Some(&b'.') && self.b.get(self.i + 1).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
            while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                self.i += 1;
            }
        }
        if matches!(self.b.get(self.i), Some(b'e' | b'E')) {
            let save = self.i;
            self.i += 1;
            if matches!(self.b.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                    self.i += 1;
                }
            } else {
                self.i = save;
            }
        }
        self.s[st..self.i].parse().ok()
    }

    fn hex4(&mut self) -> Option<u32> {
        let h = self.s.get(self.i..self.i + 4)?;
        if !h.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        self.i += 4;
        u32::from_str_radix(h, 16).ok()
    }

    fn string(&mut self) -> Option<String> {
        if self.b.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = *self.b.get(self.i)?;
            match c {
                b'"' => {
                    self.i += 1;
                    return Some(out);
                }
                b'\\' => {
                    self.i += 1;
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
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
                            let u = self.hex4()?;
                            if (0xD800..0xDC00).contains(&u) && self.s[self.i..].starts_with("\\u") {
                                let save = self.i;
                                self.i += 2;
                                match self.hex4() {
                                    Some(lo) if (0xDC00..0xE000).contains(&lo) => {
                                        out.push(char::from_u32(0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00))?);
                                        continue;
                                    }
                                    _ => self.i = save,
                                }
                            }
                            // a lone surrogate cannot be a Rust char: U+FFFD
                            out.push(char::from_u32(u).unwrap_or('\u{FFFD}'));
                        }
                        _ => return None,
                    }
                }
                0..=0x1F => return None,
                _ => {
                    let ch = self.s[self.i..].chars().next()?;
                    out.push(ch);
                    self.i += ch.len_utf8();
                }
            }
        }
    }
}

/// The text as a JSON string literal with only ASCII characters (what `json.dumps` writes), so it survives a
/// Latin-1 PNG `tEXt` chunk.
pub fn ascii_literal(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            '\u{8}' => o.push_str("\\b"),
            '\u{c}' => o.push_str("\\f"),
            c if (' '..='~').contains(&c) => o.push(c),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    o.push_str(&format!("\\u{u:04x}"));
                }
            }
        }
    }
    o.push('"');
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_spans() {
        let s = r#" [{"text": "Hi é😀", "bbox": [1, 2.5, 3, 4e1], "x": NaN}] "#;
        let n = parse(s).unwrap();
        let Val::Arr(a) = &n.v else { panic!() };
        let t = a[0].get("text").unwrap();
        assert_eq!(t.v, Val::Str("Hi \u{e9}\u{1F600}".into()));
        assert_eq!(&s[t.span.0..t.span.1], r#""Hi é😀""#);
        assert!(parse("[1,]").is_none() && parse("{} x").is_none());
        assert_eq!(parse(&ascii_literal("a\"\u{e9}\u{1F600}")).unwrap().v, Val::Str("a\"\u{e9}\u{1F600}".into()));
    }
}
