//! A strict JSON scanner that keeps byte positions. It validates the whole text (like `json.loads`: NaN and
//! Infinity are accepted, comments, trailing commas and control characters in strings are not) and keeps a tree
//! only as deep as an ARB file needs: the top-level object, its `@key` metadata objects and their placeholders.
//! Anything deeper is validated and recorded as a span.

/// Where a value sits in the text, and what it is.
#[derive(Debug, Clone)]
pub enum Node {
    /// A string token, quotes included.
    Str { start: usize, end: usize },
    /// An object whose members are kept (depth 3 or less).
    Obj { pairs: Vec<Pair>, start: usize, end: usize },
    /// Anything else: a number, literal, array, or an object deeper than the kept levels.
    Other { start: usize, end: usize },
}

#[derive(Debug, Clone)]
pub struct Pair {
    pub key: String,
    pub key_start: usize,
    pub value: Node,
}

impl Node {
    pub fn start(&self) -> usize {
        match self {
            Node::Str { start, .. } | Node::Obj { start, .. } | Node::Other { start, .. } => *start,
        }
    }
    pub fn end(&self) -> usize {
        match self {
            Node::Str { end, .. } | Node::Obj { end, .. } | Node::Other { end, .. } => *end,
        }
    }
}

#[derive(Debug)]
pub struct ScanError {
    pub msg: String,
    pub pos: usize,
}

/// Levels whose members are kept: root (0), `@key` (1), `placeholders` (2), one placeholder (3).
const KEEP_DEPTH: usize = 3;

struct Scanner<'a> {
    t: &'a str,
    b: &'a [u8],
    max_depth: usize,
}

pub fn scan(text: &str, max_depth: usize) -> Result<Node, ScanError> {
    let s = Scanner { t: text, b: text.as_bytes(), max_depth };
    let i = s.ws(0);
    let (node, i) = s.value(i, 0)?;
    let i = s.ws(i);
    if i < s.b.len() {
        return Err(s.err("extra data after the JSON value", i));
    }
    Ok(node)
}

impl Scanner<'_> {
    fn err(&self, msg: &str, pos: usize) -> ScanError {
        ScanError { msg: msg.to_string(), pos }
    }

    fn ws(&self, mut i: usize) -> usize {
        while i < self.b.len() && matches!(self.b[i], b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
        }
        i
    }

    fn value(&self, i: usize, depth: usize) -> Result<(Node, usize), ScanError> {
        if depth > self.max_depth {
            return Err(self.err(&format!("nested deeper than {} levels", self.max_depth), i));
        }
        let Some(&c) = self.b.get(i) else { return Err(self.err("unexpected end of the file", i)) };
        match c {
            b'"' => {
                let end = self.string_end(i)?;
                Ok((Node::Str { start: i, end }, end))
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
    fn string_end(&self, i: usize) -> Result<usize, ScanError> {
        let b = self.b;
        let mut p = i + 1;
        loop {
            let Some(&c) = b.get(p) else { return Err(self.err("unterminated string", i)) };
            match c {
                b'"' => return Ok(p + 1),
                b'\\' => match b.get(p + 1) {
                    Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => p += 2,
                    Some(b'u') => {
                        if p + 6 <= b.len() && b[p + 2..p + 6].iter().all(u8::is_ascii_hexdigit) {
                            p += 6;
                        } else {
                            return Err(self.err("bad \\u escape", p));
                        }
                    }
                    _ => return Err(self.err("bad escape in string", p)),
                },
                0..=0x1f => return Err(self.err("control character in string", p)),
                _ => p += 1,
            }
        }
    }

    fn literal(&self, i: usize) -> Result<usize, ScanError> {
        let b = self.b;
        let rest = &b[i..];
        for lit in [&b"true"[..], b"false", b"null", b"NaN", b"Infinity", b"-Infinity"] {
            if rest.starts_with(lit) {
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
            _ => return Err(self.err("expected a JSON value", i)),
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

    fn object(&self, start: usize, depth: usize) -> Result<(Node, usize), ScanError> {
        let keep = depth <= KEEP_DEPTH;
        let mut pairs = Vec::new();
        let mut i = self.ws(start + 1);
        if self.b.get(i) == Some(&b'}') {
            return Ok((self.obj_node(keep, pairs, start, i + 1), i + 1));
        }
        loop {
            if self.b.get(i) != Some(&b'"') {
                return Err(self.err("expected a property name in double quotes", i));
            }
            let key_end = self.string_end(i)?;
            let key = if keep { lossy_unescape(&self.t[i + 1..key_end - 1]) } else { String::new() };
            let key_start = i;
            i = self.ws(key_end);
            if self.b.get(i) != Some(&b':') {
                return Err(self.err("expected ':' after a property name", i));
            }
            i = self.ws(i + 1);
            let (value, next) = self.value(i, depth + 1)?;
            if keep {
                pairs.push(Pair { key, key_start, value });
            }
            i = self.ws(next);
            match self.b.get(i) {
                Some(b',') => i = self.ws(i + 1),
                Some(b'}') => return Ok((self.obj_node(keep, pairs, start, i + 1), i + 1)),
                _ => return Err(self.err("expected ',' or '}'", i)),
            }
        }
    }

    fn obj_node(&self, keep: bool, pairs: Vec<Pair>, start: usize, end: usize) -> Node {
        if keep {
            Node::Obj { pairs, start, end }
        } else {
            Node::Other { start, end }
        }
    }

    fn array(&self, start: usize, depth: usize) -> Result<(Node, usize), ScanError> {
        let mut i = self.ws(start + 1);
        if self.b.get(i) == Some(&b']') {
            return Ok((Node::Other { start, end: i + 1 }, i + 1));
        }
        loop {
            let (_, next) = self.value(i, depth + 1)?;
            i = self.ws(next);
            match self.b.get(i) {
                Some(b',') => i = self.ws(i + 1),
                Some(b']') => return Ok((Node::Other { start, end: i + 1 }, i + 1)),
                _ => return Err(self.err("expected ',' or ']'", i)),
            }
        }
    }
}

/// 1-based line and column of a byte offset.
pub fn line_col(text: &str, pos: usize) -> (usize, usize) {
    let pos = pos.min(text.len());
    let head = &text.as_bytes()[..pos];
    let line = head.iter().filter(|&&c| c == b'\n').count() + 1;
    let col = pos - head.iter().rposition(|&c| c == b'\n').map_or(0, |p| p + 1) + 1;
    (line, col)
}

/// The text of a string token's inside (between the quotes). `None` when it holds a lone surrogate escape,
/// which Rust strings cannot hold.
pub fn unescape(inner: &str) -> Option<String> {
    decode(inner, false)
}

fn lossy_unescape(inner: &str) -> String {
    decode(inner, true).unwrap_or_default()
}

fn decode(inner: &str, lossy: bool) -> Option<String> {
    if !inner.contains('\\') {
        return Some(inner.to_string());
    }
    let b = inner.as_bytes();
    let mut out = String::with_capacity(inner.len());
    let mut i = 0;
    let mut run = 0; // start of the pending plain run
    while i < b.len() {
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        out.push_str(&inner[run..i]);
        let c = *b.get(i + 1)?;
        i += 2;
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
                let hi = hex4(b, i)?;
                i += 4;
                let ch = if (0xD800..0xDC00).contains(&hi) {
                    // a high surrogate needs its low half right behind it
                    if b.get(i) == Some(&b'\\') && b.get(i + 1) == Some(&b'u') {
                        let lo = hex4(b, i + 2)?;
                        if (0xDC00..0xE000).contains(&lo) {
                            i += 6;
                            char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else if (0xDC00..0xE000).contains(&hi) {
                    None
                } else {
                    char::from_u32(hi)
                };
                match ch {
                    Some(ch) => out.push(ch),
                    None if lossy => out.push('\u{FFFD}'),
                    None => return None,
                }
            }
            _ => return None,
        }
        run = i;
    }
    out.push_str(&inner[run..]);
    Some(out)
}

fn hex4(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 4)?;
    let mut v = 0u32;
    for &c in s {
        v = v * 16 + (c as char).to_digit(16)?;
    }
    Some(v)
}

/// A JSON string token for `s`: quotes, backslash and control characters escaped, everything else as is.
pub fn quote(s: &str) -> String {
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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
