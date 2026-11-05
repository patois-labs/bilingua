//! A validating scanner for TOML 1.0. It walks the document once, checks the grammar and the table/key rules
//! (duplicate keys, redefined tables, arrays of tables), and records the byte span, quoting kind, decoded value
//! and key path of every string value. Keys, numbers, dates, comments and layout are only checked, never kept.

use crate::render::escape_basic;
use crate::scalar::valid_scalar;
use std::borrow::Cow;
use std::collections::HashMap;

/// Arrays and inline tables nested deeper than this are refused.
pub const MAX_DEPTH: usize = 64;
/// More string values than this are refused.
pub const MAX_STRINGS: usize = 2_000_000;
/// More values (strings, numbers, arrays, tables) than this are refused.
pub const MAX_VALUES: usize = 8_000_000;

#[derive(Debug)]
pub enum ScanError {
    /// Not valid TOML; the message names the line.
    Syntax(String),
    /// A size limit was hit.
    Limit(String),
}

/// How a string was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Basic,
    Literal,
    MlBasic,
    MlLiteral,
}

/// One string value.
#[derive(Debug)]
pub struct Str<'a> {
    /// Byte span of the whole token, quotes included.
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
    /// The line break that directly follows an opening multi-line delimiter ("" for single-line strings).
    pub lead: &'static str,
    pub value: Cow<'a, str>,
    /// Readable key path: `fruits[0].physical.color`.
    pub key: String,
    /// The table or inline table the value is a direct member of; `None` for an array element.
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
enum Seg {
    Key(String),
    Idx(usize),
}

#[derive(Debug)]
enum Val {
    Str(usize),
    Other,
    Table(usize),
    Array,
    Aot(Vec<usize>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TKind {
    Root,
    Implicit,
    Explicit,
    Dotted,
    Inline,
}

#[derive(Debug)]
pub struct Table {
    kind: TKind,
    keys: Vec<String>,
    vals: Vec<Val>,
    index: HashMap<String, usize>,
}

impl Table {
    fn new(kind: TKind) -> Table {
        Table { kind, keys: Vec::new(), vals: Vec::new(), index: HashMap::new() }
    }
    /// The (key, string index or None for a non-string value) of every member.
    pub fn members(&self) -> impl Iterator<Item = (&str, Option<usize>)> {
        self.keys.iter().zip(&self.vals).map(|(k, v)| (k.as_str(), if let Val::Str(i) = v { Some(*i) } else { None }))
    }
    /// The string index of the member `key`, when it is a string.
    pub fn string(&self, key: &str) -> Option<usize> {
        match self.vals.get(*self.index.get(key)?)? {
            Val::Str(i) => Some(*i),
            _ => None,
        }
    }
}

pub struct Scan<'a> {
    pub strings: Vec<Str<'a>>,
    pub tables: Vec<Table>,
    /// True when the document holds at least one key/value pair or table header.
    pub has_content: bool,
}

enum Peek {
    Missing,
    Table(usize),
    Aot(usize, usize),
    Other,
}

struct Tok<'a> {
    kind: Kind,
    value: Cow<'a, str>,
    start: usize,
    end: usize,
    lead: &'static str,
}

type R<T> = Result<T, ScanError>;

struct P<'a> {
    t: &'a str,
    b: &'a [u8],
    i: usize,
    tables: Vec<Table>,
    strings: Vec<Str<'a>>,
    path: Vec<Seg>,
    nvals: usize,
    has_content: bool,
}

pub fn scan(text: &str) -> R<Scan<'_>> {
    let mut p = P {
        t: text,
        b: text.as_bytes(),
        i: 0,
        tables: vec![Table::new(TKind::Root)],
        strings: Vec::new(),
        path: Vec::new(),
        nvals: 0,
        has_content: false,
    };
    p.run()?;
    Ok(Scan { strings: p.strings, tables: p.tables, has_content: p.has_content })
}

fn is_ctrl(c: u8) -> bool {
    c < 0x20 || c == 0x7f
}

impl<'a> P<'a> {
    fn err<T>(&self, what: &str) -> R<T> {
        let line = 1 + self.b[..self.i.min(self.b.len())].iter().filter(|&&c| c == b'\n').count();
        Err(ScanError::Syntax(format!("{what} on line {line}")))
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.i += 1;
        }
    }

    fn comment(&mut self) -> R<()> {
        // at '#'
        while let Some(c) = self.peek() {
            if c == b'\n' || c == b'\r' {
                break;
            }
            if is_ctrl(c) && c != b'\t' {
                return self.err("a control character in a comment");
            }
            self.i += 1;
        }
        Ok(())
    }

    /// Whitespace, line breaks and comments.
    fn ws_nl(&mut self) -> R<()> {
        while let Some(c) = self.peek() {
            match c {
                b' ' | b'\t' | b'\n' => self.i += 1,
                b'\r' => {
                    if self.b.get(self.i + 1) == Some(&b'\n') {
                        self.i += 2;
                    } else {
                        return self.err("a bare carriage return");
                    }
                }
                b'#' => self.comment()?,
                _ => break,
            }
        }
        Ok(())
    }

    fn eol(&mut self) -> R<()> {
        self.ws();
        if self.peek() == Some(b'#') {
            self.comment()?;
        }
        match self.peek() {
            None | Some(b'\n') => Ok(()),
            Some(b'\r') if self.b.get(self.i + 1) == Some(&b'\n') => Ok(()),
            _ => self.err("unexpected text after a value"),
        }
    }

    fn expect(&mut self, s: &str) -> R<()> {
        if self.t.get(self.i..).is_some_and(|r| r.starts_with(s)) {
            self.i += s.len();
            Ok(())
        } else {
            self.err(&format!("expected '{s}'"))
        }
    }

    fn starts(&self, s: &str) -> bool {
        self.t.get(self.i..).is_some_and(|r| r.starts_with(s))
    }

    // ---- keys

    fn key_parts(&mut self) -> R<Vec<String>> {
        let mut parts = Vec::new();
        loop {
            self.ws();
            match self.peek() {
                Some(b'"' | b'\'') => {
                    let tok = self.string()?;
                    if matches!(tok.kind, Kind::MlBasic | Kind::MlLiteral) {
                        return self.err("a multi-line string used as a key");
                    }
                    parts.push(tok.value.into_owned());
                }
                _ => {
                    let s = self.i;
                    while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
                        self.i += 1;
                    }
                    if self.i == s {
                        return self.err("expected a key");
                    }
                    parts.push(self.t[s..self.i].to_string());
                }
            }
            self.ws();
            if self.peek() == Some(b'.') {
                self.i += 1;
                continue;
            }
            return Ok(parts);
        }
    }

    // ---- strings

    fn string(&mut self) -> R<Tok<'a>> {
        let start = self.i;
        let t = self.t;
        if self.starts("\"\"\"") {
            return self.ml(start, b'"');
        }
        if self.starts("'''") {
            return self.ml(start, b'\'');
        }
        if self.b[start] == b'\'' {
            let mut i = start + 1;
            loop {
                match self.b.get(i) {
                    None | Some(b'\n' | b'\r') => {
                        self.i = i;
                        return self.err("an unterminated literal string");
                    }
                    Some(b'\'') => break,
                    Some(&c) if is_ctrl(c) && c != b'\t' => {
                        self.i = i;
                        return self.err("a control character in a string");
                    }
                    _ => i += 1,
                }
            }
            self.i = i + 1;
            return Ok(Tok { kind: Kind::Literal, value: Cow::Borrowed(&t[start + 1..i]), start, end: i + 1, lead: "" });
        }
        // basic
        let mut i = start + 1;
        let mut seg = i;
        let mut out: Option<String> = None;
        loop {
            let Some(&c) = self.b.get(i) else {
                self.i = i;
                return self.err("an unterminated string");
            };
            match c {
                b'"' => break,
                b'\\' => {
                    let (ni, ch) = self.escape(i)?;
                    let o = out.get_or_insert_with(String::new);
                    o.push_str(&t[seg..i]);
                    o.push(ch);
                    i = ni;
                    seg = i;
                }
                b'\n' | b'\r' => {
                    self.i = i;
                    return self.err("an unterminated string");
                }
                c if is_ctrl(c) && c != b'\t' => {
                    self.i = i;
                    return self.err("a control character in a string");
                }
                _ => i += 1,
            }
        }
        let value = match out {
            None => Cow::Borrowed(&t[seg..i]),
            Some(mut o) => {
                o.push_str(&t[seg..i]);
                Cow::Owned(o)
            }
        };
        self.i = i + 1;
        Ok(Tok { kind: Kind::Basic, value, start, end: i + 1, lead: "" })
    }

    fn escape(&mut self, i: usize) -> R<(usize, char)> {
        let e = self.b.get(i + 1).copied();
        let named = match e {
            Some(b'b') => Some('\u{8}'),
            Some(b't') => Some('\t'),
            Some(b'n') => Some('\n'),
            Some(b'f') => Some('\u{c}'),
            Some(b'r') => Some('\r'),
            Some(b'"') => Some('"'),
            Some(b'\\') => Some('\\'),
            _ => None,
        };
        if let Some(c) = named {
            return Ok((i + 2, c));
        }
        let width = match e {
            Some(b'u') => 4,
            Some(b'U') => 8,
            _ => 0,
        };
        if width > 0 {
            if let Some(h) = self.t.get(i + 2..i + 2 + width) {
                if h.bytes().all(|c| c.is_ascii_hexdigit()) {
                    if let Some(c) = u32::from_str_radix(h, 16).ok().and_then(char::from_u32) {
                        return Ok((i + 2 + width, c));
                    }
                }
            }
        }
        self.i = i;
        self.err("an invalid escape sequence")
    }

    fn ml(&mut self, start: usize, q: u8) -> R<Tok<'a>> {
        let t = self.t;
        let n = self.b.len();
        let basic = q == b'"';
        let delim = if basic { "\"\"\"" } else { "'''" };
        let mut i = start + 3;
        let lead: &'static str = if t[i..].starts_with("\r\n") {
            "\r\n"
        } else if t[i..].starts_with('\n') {
            "\n"
        } else {
            ""
        };
        i += lead.len();
        let vstart = i;
        let mut seg = i;
        let mut out: Option<String> = None;
        let end;
        loop {
            let Some(&c) = self.b.get(i) else {
                self.i = i;
                return self.err("an unterminated multi-line string");
            };
            if c == q && t[i..].starts_with(delim) {
                let mut run = 3;
                while run < 5 && i + run < n && self.b[i + run] == q {
                    run += 1;
                }
                end = i + run;
                break;
            }
            if basic && c == b'\\' {
                let mut j = i + 1;
                while matches!(self.b.get(j), Some(b' ' | b'\t')) {
                    j += 1;
                }
                let line_end = match self.b.get(j) {
                    Some(b'\n') => true,
                    Some(b'\r') => self.b.get(j + 1) == Some(&b'\n'),
                    _ => false,
                };
                if line_end {
                    let o = out.get_or_insert_with(String::new);
                    o.push_str(&t[seg..i]);
                    while matches!(self.b.get(j), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                        if self.b[j] == b'\r' && self.b.get(j + 1) != Some(&b'\n') {
                            self.i = j;
                            return self.err("a bare carriage return");
                        }
                        j += 1;
                    }
                    i = j;
                } else {
                    let (ni, ch) = self.escape(i)?;
                    let o = out.get_or_insert_with(String::new);
                    o.push_str(&t[seg..i]);
                    o.push(ch);
                    i = ni;
                }
                seg = i;
                continue;
            }
            if c == b'\r' {
                if self.b.get(i + 1) == Some(&b'\n') {
                    let o = out.get_or_insert_with(String::new);
                    o.push_str(&t[seg..i]);
                    o.push('\n');
                    i += 2;
                    seg = i;
                    continue;
                }
                self.i = i;
                return self.err("a bare carriage return");
            }
            if is_ctrl(c) && c != b'\t' && c != b'\n' {
                self.i = i;
                return self.err("a control character in a string");
            }
            i += 1;
        }
        // Quotes beyond the closing delimiter (at most two) are part of the text.
        let value = match out {
            None => Cow::Borrowed(&t[vstart..end - 3]),
            Some(mut o) => {
                o.push_str(&t[seg..end - 3]);
                Cow::Owned(o)
            }
        };
        self.i = end;
        Ok(Tok { kind: if basic { Kind::MlBasic } else { Kind::MlLiteral }, value, start, end, lead })
    }

    // ---- values

    fn key_text(&self) -> String {
        let mut s = String::new();
        for seg in &self.path {
            match seg {
                Seg::Idx(i) => {
                    s.push('[');
                    s.push_str(&i.to_string());
                    s.push(']');
                }
                Seg::Key(k) => {
                    if !s.is_empty() {
                        s.push('.');
                    }
                    if !k.is_empty() && k.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
                        s.push_str(k);
                    } else {
                        s.push('"');
                        s.push_str(&escape_basic(k));
                        s.push('"');
                    }
                }
            }
        }
        s
    }

    fn new_table(&mut self, kind: TKind) -> usize {
        self.tables.push(Table::new(kind));
        self.tables.len() - 1
    }

    fn insert(&mut self, table: usize, key: String, v: Val) {
        let t = &mut self.tables[table];
        t.index.insert(key.clone(), t.keys.len());
        t.keys.push(key);
        t.vals.push(v);
    }

    fn peek_key(&self, table: usize, key: &str) -> Peek {
        let t = &self.tables[table];
        match t.index.get(key) {
            None => Peek::Missing,
            Some(&ix) => match &t.vals[ix] {
                Val::Table(id) => Peek::Table(*id),
                Val::Aot(v) => Peek::Aot(*v.last().unwrap_or(&0), v.len()),
                _ => Peek::Other,
            },
        }
    }

    fn value(&mut self, parent: Option<usize>, depth: usize) -> R<Val> {
        if depth > MAX_DEPTH {
            return Err(ScanError::Limit("the TOML file is nested too deeply".into()));
        }
        self.nvals += 1;
        if self.nvals > MAX_VALUES {
            return Err(ScanError::Limit("the TOML file has too many values".into()));
        }
        match self.peek() {
            Some(b'"' | b'\'') => {
                let tok = self.string()?;
                if self.strings.len() >= MAX_STRINGS {
                    return Err(ScanError::Limit("the TOML file has too many strings".into()));
                }
                let key = self.key_text();
                self.strings.push(Str { start: tok.start, end: tok.end, kind: tok.kind, lead: tok.lead, value: tok.value, key, parent });
                Ok(Val::Str(self.strings.len() - 1))
            }
            Some(b'[') => {
                self.i += 1;
                let mut k = 0;
                loop {
                    self.ws_nl()?;
                    if self.peek() == Some(b']') {
                        self.i += 1;
                        break;
                    }
                    self.path.push(Seg::Idx(k));
                    self.value(None, depth + 1)?;
                    self.path.pop();
                    k += 1;
                    self.ws_nl()?;
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b']') => {}
                        _ => return self.err("expected ',' or ']' in an array"),
                    }
                }
                Ok(Val::Array)
            }
            Some(b'{') => {
                self.i += 1;
                let id = self.new_table(TKind::Inline);
                self.ws();
                if self.peek() == Some(b'}') {
                    self.i += 1;
                    return Ok(Val::Table(id));
                }
                loop {
                    self.keyval(id, depth + 1)?;
                    self.ws();
                    match self.peek() {
                        Some(b',') => {
                            self.i += 1;
                            self.ws();
                        }
                        Some(b'}') => {
                            self.i += 1;
                            break;
                        }
                        _ => return self.err("expected ',' or '}' in an inline table"),
                    }
                }
                Ok(Val::Table(id))
            }
            _ => {
                let s = self.i;
                while let Some(c) = self.peek() {
                    if matches!(c, b',' | b']' | b'}' | b'#' | b'\r' | b'\n') {
                        break;
                    }
                    self.i += 1;
                }
                let raw = self.t.get(s..self.i).unwrap_or("");
                let trimmed = raw.trim_end_matches([' ', '\t']);
                self.i = s + trimmed.len();
                if trimmed.is_empty() {
                    return self.err("expected a value");
                }
                if !valid_scalar(trimmed) {
                    return self.err("an invalid value");
                }
                Ok(Val::Other)
            }
        }
    }

    /// `key = value` (keys may be dotted) as a member of `table`.
    fn keyval(&mut self, table: usize, depth: usize) -> R<()> {
        let mut keys = self.key_parts()?;
        self.expect("=")?;
        self.ws();
        let last = keys.pop().unwrap_or_default();
        let base = self.path.len();
        let mut cur = table;
        for k in keys {
            self.path.push(Seg::Key(k.clone()));
            match self.peek_key(cur, &k) {
                Peek::Missing => {
                    let id = self.new_table(TKind::Dotted);
                    self.insert(cur, k, Val::Table(id));
                    cur = id;
                }
                Peek::Table(id) if self.tables[id].kind == TKind::Dotted => cur = id,
                _ => return self.err("a key that is already defined"),
            }
        }
        if !matches!(self.peek_key(cur, &last), Peek::Missing) {
            return self.err("a duplicate key");
        }
        self.path.push(Seg::Key(last.clone()));
        let v = self.value(Some(cur), depth)?;
        self.insert(cur, last, v);
        self.path.truncate(base);
        Ok(())
    }

    /// `[a.b]` or `[[a.b]]`; returns the table the following pairs belong to and sets `self.path`.
    fn header(&mut self, aot: bool) -> R<usize> {
        let keys = self.key_parts()?;
        self.expect(if aot { "]]" } else { "]" })?;
        self.path.clear();
        let n = keys.len();
        let mut cur = 0usize;
        for (ix, k) in keys.into_iter().enumerate() {
            let last = ix + 1 == n;
            self.path.push(Seg::Key(k.clone()));
            match self.peek_key(cur, &k) {
                Peek::Missing => {
                    let id = self.new_table(if last && !aot { TKind::Explicit } else { TKind::Implicit });
                    if last && aot {
                        self.tables[id].kind = TKind::Explicit;
                        self.insert(cur, k, Val::Aot(vec![id]));
                        self.path.push(Seg::Idx(0));
                    } else {
                        self.insert(cur, k, Val::Table(id));
                    }
                    cur = id;
                }
                Peek::Table(id) => {
                    if last {
                        if aot || self.tables[id].kind != TKind::Implicit {
                            return self.err("a table that is already defined");
                        }
                        self.tables[id].kind = TKind::Explicit;
                    } else if self.tables[id].kind == TKind::Inline {
                        return self.err("a table that is already defined");
                    }
                    cur = id;
                }
                Peek::Aot(lastid, count) => {
                    if last {
                        if !aot {
                            return self.err("a table that is already defined");
                        }
                        let id = self.new_table(TKind::Explicit);
                        let ix = self.tables[cur].index[&k];
                        if let Val::Aot(v) = &mut self.tables[cur].vals[ix] {
                            v.push(id);
                        }
                        self.path.push(Seg::Idx(count));
                        cur = id;
                    } else {
                        self.path.push(Seg::Idx(count - 1));
                        cur = lastid;
                    }
                }
                Peek::Other => return self.err("a key that is already defined"),
            }
        }
        Ok(cur)
    }

    fn run(&mut self) -> R<()> {
        let mut current = 0usize;
        loop {
            self.ws_nl()?;
            if self.i >= self.b.len() {
                return Ok(());
            }
            self.has_content = true;
            if self.starts("[[") {
                self.i += 2;
                current = self.header(true)?;
            } else if self.peek() == Some(b'[') {
                self.i += 1;
                current = self.header(false)?;
            } else {
                let base = self.path.len();
                self.keyval(current, 0)?;
                self.path.truncate(base);
            }
            self.eol()?;
        }
    }
}
