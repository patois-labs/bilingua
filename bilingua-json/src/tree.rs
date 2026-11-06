//! A strict JSON scanner that builds a tree with byte positions, for the resource dialects (Chrome, FormatJS,
//! Structured JSON, go-i18n, ARB-shaped files) and the i18next plural fill. It reads what Python's `json.loads` reads
//! (`NaN`, `Infinity` and `-Infinity` included; comments, trailing commas and raw control characters are errors),
//! keeps every member of every object in file order (duplicate keys included, as LocOS's scanner does), and refuses
//! nesting deeper than 200 levels, as LocOS does.

/// A JSON value with its byte range in the text.
#[derive(Debug, Clone)]
pub enum Node {
    /// A string token, quotes included. `val` is `None` for a lone surrogate escape, which Rust cannot hold.
    Str { val: Option<String>, start: usize, end: usize },
    Obj(Obj),
    Arr { items: Vec<Node>, start: usize, end: usize },
    /// A number or a literal (`true`, `false`, `null`, `NaN`, ...).
    Other { start: usize, end: usize },
}

#[derive(Debug, Clone)]
pub struct Obj {
    pub pairs: Vec<Pair>,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct Pair {
    pub key: String,
    pub key_start: usize,
    pub value: Node,
}

impl Obj {
    /// The value of the last member with this key (what `dict(pairs)` keeps).
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.pairs.iter().rev().find(|p| p.key == key).map(|p| &p.value)
    }

    /// True when a member has this key.
    pub fn has(&self, key: &str) -> bool {
        self.pairs.iter().any(|p| p.key == key)
    }
}

impl Node {
    pub fn start(&self) -> usize {
        match self {
            Node::Str { start, .. } | Node::Arr { start, .. } | Node::Other { start, .. } => *start,
            Node::Obj(o) => o.start,
        }
    }

    pub fn end(&self) -> usize {
        match self {
            Node::Str { end, .. } | Node::Arr { end, .. } | Node::Other { end, .. } => *end,
            Node::Obj(o) => o.end,
        }
    }

    pub fn is_str(&self) -> bool {
        matches!(self, Node::Str { .. })
    }

    pub fn as_obj(&self) -> Option<&Obj> {
        match self {
            Node::Obj(o) => Some(o),
            _ => None,
        }
    }
}

const MAX_DEPTH: usize = 200;

/// Scans `text`. `Err(())` for anything that is not JSON or is nested deeper than 200 levels (the caller has already
/// validated the text with the flat scanner, so for it that means "too deep").
pub fn scan(text: &str) -> Result<Node, ()> {
    let s = Scanner { t: text, b: text.as_bytes() };
    let i = s.ws(0);
    let (node, i) = s.value(i, 0)?;
    if s.ws(i) < s.b.len() {
        return Err(());
    }
    Ok(node)
}

struct Scanner<'a> {
    t: &'a str,
    b: &'a [u8],
}

impl Scanner<'_> {
    fn ws(&self, mut i: usize) -> usize {
        while i < self.b.len() && matches!(self.b[i], b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
        }
        i
    }

    fn value(&self, i: usize, depth: usize) -> Result<(Node, usize), ()> {
        if depth > MAX_DEPTH {
            return Err(());
        }
        match *self.b.get(i).ok_or(())? {
            b'"' => {
                let end = self.string_end(i)?;
                Ok((Node::Str { val: crate::scan::unescape(&self.t[i + 1..end - 1]), start: i, end }, end))
            }
            b'{' => self.object(i, depth),
            b'[' => self.array(i, depth),
            _ => {
                let end = self.literal(i)?;
                Ok((Node::Other { start: i, end }, end))
            }
        }
    }

    /// Index just past the closing quote of the string starting at `i`.
    fn string_end(&self, i: usize) -> Result<usize, ()> {
        let b = self.b;
        let mut p = i + 1;
        loop {
            match *b.get(p).ok_or(())? {
                b'"' => return Ok(p + 1),
                b'\\' => match b.get(p + 1) {
                    Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => p += 2,
                    Some(b'u') if p + 6 <= b.len() && b[p + 2..p + 6].iter().all(u8::is_ascii_hexdigit) => p += 6,
                    _ => return Err(()),
                },
                0..=0x1f => return Err(()),
                _ => p += 1,
            }
        }
    }

    fn literal(&self, i: usize) -> Result<usize, ()> {
        let b = self.b;
        for lit in [&b"true"[..], b"false", b"null", b"NaN", b"Infinity", b"-Infinity"] {
            if b[i..].starts_with(lit) {
                return Ok(i + lit.len());
            }
        }
        let mut p = i;
        if b.get(p) == Some(&b'-') {
            p += 1;
        }
        match b.get(p) {
            Some(b'0') => p += 1,
            Some(b'1'..=b'9') => {
                while b.get(p).is_some_and(u8::is_ascii_digit) {
                    p += 1;
                }
            }
            _ => return Err(()),
        }
        if b.get(p) == Some(&b'.') && b.get(p + 1).is_some_and(u8::is_ascii_digit) {
            p += 1;
            while b.get(p).is_some_and(u8::is_ascii_digit) {
                p += 1;
            }
        }
        if matches!(b.get(p), Some(b'e' | b'E')) {
            let mut q = p + 1;
            if matches!(b.get(q), Some(b'+' | b'-')) {
                q += 1;
            }
            if b.get(q).is_some_and(u8::is_ascii_digit) {
                while b.get(q).is_some_and(u8::is_ascii_digit) {
                    q += 1;
                }
                p = q;
            }
        }
        Ok(p)
    }

    fn object(&self, start: usize, depth: usize) -> Result<(Node, usize), ()> {
        let mut pairs = Vec::new();
        let mut i = self.ws(start + 1);
        if self.b.get(i) == Some(&b'}') {
            return Ok((Node::Obj(Obj { pairs, start, end: i + 1 }), i + 1));
        }
        loop {
            if self.b.get(i) != Some(&b'"') {
                return Err(());
            }
            let key_end = self.string_end(i)?;
            let key = crate::scan::unescape_lossy(&self.t[i + 1..key_end - 1]);
            let key_start = i;
            i = self.ws(key_end);
            if self.b.get(i) != Some(&b':') {
                return Err(());
            }
            i = self.ws(i + 1);
            let (value, next) = self.value(i, depth + 1)?;
            pairs.push(Pair { key, key_start, value });
            i = self.ws(next);
            match self.b.get(i) {
                Some(b',') => i = self.ws(i + 1),
                Some(b'}') => return Ok((Node::Obj(Obj { pairs, start, end: i + 1 }), i + 1)),
                _ => return Err(()),
            }
        }
    }

    fn array(&self, start: usize, depth: usize) -> Result<(Node, usize), ()> {
        let mut items = Vec::new();
        let mut i = self.ws(start + 1);
        if self.b.get(i) == Some(&b']') {
            return Ok((Node::Arr { items, start, end: i + 1 }, i + 1));
        }
        loop {
            let (v, next) = self.value(i, depth + 1)?;
            items.push(v);
            i = self.ws(next);
            match self.b.get(i) {
                Some(b',') => i = self.ws(i + 1),
                Some(b']') => return Ok((Node::Arr { items, start, end: i + 1 }, i + 1)),
                _ => return Err(()),
            }
        }
    }
}
