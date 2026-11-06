//! A strict JSON scanner that finds every string value with its byte range and JSON pointer, without building
//! a tree and without touching a byte of the input.

use bilingua_model::Error;

/// A string value: the byte range of the whole token (quotes included) and its JSON pointer.
#[derive(Debug, Clone)]
pub struct StrVal {
    pub start: usize,
    pub end: usize,
    pub pointer: String,
    /// Python's walk order of the value: per level, the position of its key among the object's distinct keys
    /// (first occurrence) or its array index. What `json.loads` iterates (the LocOS profile).
    pub ord: Vec<usize>,
    /// Under a key that the same object repeats later: `json.loads` keeps the last value only.
    pub dead: bool,
}

const MAX_DEPTH: usize = 200;

pub fn scan(text: &str) -> Result<Vec<StrVal>, Error> {
    scan_with(text, false)
}

/// [`scan`]; with `python` also the `NaN`, `Infinity` and `-Infinity` that Python's `json.loads` reads (LocOS).
pub fn scan_with(text: &str, python: bool) -> Result<Vec<StrVal>, Error> {
    let mut s = Scan { text, b: text.as_bytes(), i: 0, depth: 0, vals: Vec::new(), ord: Vec::new(), python };
    s.ws();
    let mut path = Vec::new();
    s.value(&mut path)?;
    s.ws();
    if s.i != s.b.len() {
        return Err(s.err("extra content after the JSON value"));
    }
    Ok(s.vals)
}

struct Scan<'a> {
    text: &'a str,
    b: &'a [u8],
    i: usize,
    depth: usize,
    vals: Vec<StrVal>,
    ord: Vec<usize>,
    python: bool,
}

impl Scan<'_> {
    fn err(&self, msg: &str) -> Error {
        let before = &self.text[..self.i.min(self.text.len())];
        let line = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
        Error::Parse { format: "json".into(), msg: format!("{msg} (line {line}, column {col})") }
    }

    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn value(&mut self, path: &mut Vec<String>) -> Result<(), Error> {
        match self.peek() {
            None => Err(self.err("unexpected end of the file")),
            Some(b'{') => self.object(path),
            Some(b'[') => self.array(path),
            Some(b'"') => {
                let (start, end) = self.string()?;
                self.vals.push(StrVal { start, end, pointer: pointer(path), ord: self.ord.clone(), dead: false });
                Ok(())
            }
            Some(b't') => self.literal("true"),
            Some(b'f') => self.literal("false"),
            Some(b'n') => self.literal("null"),
            Some(b'N') if self.python => self.literal("NaN"),
            Some(b'I') if self.python => self.literal("Infinity"),
            Some(b'-') if self.python && self.b[self.i..].starts_with(b"-Infinity") => self.literal("-Infinity"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), Error> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(())
        } else {
            Err(self.err("unexpected word"))
        }
    }

    fn number(&mut self) -> Result<(), Error> {
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek() {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.err("bad number")),
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("bad number"));
            }
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("bad number"));
            }
            self.digits();
        }
        Ok(())
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
    }

    fn string(&mut self) -> Result<(usize, usize), Error> {
        let start = self.i;
        self.i += 1; // opening quote
        loop {
            match self.peek() {
                None => return Err(self.err("string is not closed")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok((start, self.i));
                }
                Some(b'\\') => {
                    self.i += 1;
                    match self.peek() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => self.i += 1,
                        Some(b'u') => {
                            self.i += 1;
                            for _ in 0..4 {
                                if !matches!(self.peek(), Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')) {
                                    return Err(self.err("bad \\u escape"));
                                }
                                self.i += 1;
                            }
                        }
                        _ => return Err(self.err("bad escape in string")),
                    }
                }
                Some(c) if c < 0x20 => return Err(self.err("control character in string")),
                Some(_) => self.i += 1,
            }
        }
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err("nesting is too deep"));
        }
        Ok(())
    }

    fn object(&mut self, path: &mut Vec<String>) -> Result<(), Error> {
        self.enter()?;
        self.i += 1;
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            self.depth -= 1;
            return Ok(());
        }
        // key -> (its position among the distinct keys, the range of `vals` its latest value produced)
        let mut keys: Vec<(usize, usize, usize)> = Vec::new();
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("expected a key in quotes"));
            }
            let (s, e) = self.string()?;
            let inner = &self.text[s + 1..e - 1];
            let key = unescape(inner).unwrap_or_else(|| inner.to_string());
            self.ws();
            if self.peek() != Some(b':') {
                return Err(self.err("expected ':' after a key"));
            }
            self.i += 1;
            self.ws();
            path.push(key.replace('~', "~0").replace('/', "~1"));
            let at = seen.get(&key).copied();
            let o = at.map(|k| keys[k].0).unwrap_or(keys.len());
            self.ord.push(o);
            let first = self.vals.len();
            self.value(path)?;
            self.ord.pop();
            path.pop();
            let last = self.vals.len();
            match at {
                Some(k) => {
                    let (_, a, b) = keys[k];
                    for v in &mut self.vals[a..b] {
                        v.dead = true;
                    }
                    keys[k].1 = first;
                    keys[k].2 = last;
                }
                None => {
                    seen.insert(key, keys.len());
                    keys.push((o, first, last));
                }
            }
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self, path: &mut Vec<String>) -> Result<(), Error> {
        self.enter()?;
        self.i += 1;
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            self.depth -= 1;
            return Ok(());
        }
        let mut n = 0usize;
        loop {
            self.ws();
            path.push(n.to_string());
            self.ord.push(n);
            self.value(path)?;
            self.ord.pop();
            path.pop();
            n += 1;
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
    }
}

fn pointer(path: &[String]) -> String {
    path.iter().map(|p| format!("/{p}")).collect()
}

/// The text of a string token's inside (between the quotes). `None` for a lone surrogate escape.
pub fn unescape(inner: &str) -> Option<String> {
    if !inner.contains('\\') {
        return Some(inner.to_string());
    }
    let mut out = String::with_capacity(inner.len());
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next()? {
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
                    if it.next()? != '\\' || it.next()? != 'u' {
                        return None;
                    }
                    let lo = hex4(&mut it)?;
                    if !(0xDC00..0xE000).contains(&lo) {
                        return None;
                    }
                    out.push(char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00))?);
                } else {
                    out.push(char::from_u32(hi)?); // a lone low surrogate gives None
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

fn hex4(it: &mut std::str::Chars<'_>) -> Option<u32> {
    let mut v = 0u32;
    for _ in 0..4 {
        v = v * 16 + it.next()?.to_digit(16)?;
    }
    Some(v)
}

/// A JSON string token (with quotes) for `text`, escaping only what JSON requires.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
