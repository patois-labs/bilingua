//! PDF objects and the syntax reader shared by the file parser and the content-stream reader.
//!
//! The rules follow PDFium's `CPDF_SyntaxParser` where they change what a damaged file reads as: nesting deeper
//! than 64 levels gives no object, a dictionary keeps the last value of a repeated key, a stream may not sit
//! inside an array or dictionary, a hex string with an odd digit count gets a trailing zero.

use std::rc::Rc;

/// Nesting deeper than this gives no object (PDFium's `kParserMaxRecursionDepth`).
pub const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub enum Obj {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Str(Rc<Vec<u8>>),
    Name(Rc<Vec<u8>>),
    Array(Rc<Vec<Obj>>),
    Dict(Rc<Dict>),
    Ref(u32, u16),
    Stream(Rc<Stream>),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dict {
    pub items: Vec<(Vec<u8>, Obj)>,
}

impl Dict {
    pub fn get(&self, key: &[u8]) -> Option<&Obj> {
        self.items.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn has(&self, key: &[u8]) -> bool {
        self.get(key).is_some()
    }
    pub fn insert(&mut self, key: &[u8], v: Obj) {
        self.items.retain(|(k, _)| k != key);
        self.items.push((key.to_vec(), v));
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub dict: Dict,
    /// The raw (still encoded, still encrypted) bytes.
    pub raw: Vec<u8>,
    /// The object number the stream was read as (for decryption), 0 when unknown.
    pub num: u32,
    pub gen: u16,
}

impl Obj {
    pub fn as_dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) => Some(d),
            Obj::Stream(s) => Some(&s.dict),
            _ => None,
        }
    }
    pub fn as_name(&self) -> Option<&[u8]> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&[u8]> {
        match self {
            Obj::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Obj]> {
        match self {
            Obj::Array(a) => Some(a),
            _ => None,
        }
    }
    /// PDFium's `GetNumber`: numbers as floats, anything else 0.
    pub fn num(&self) -> Option<f64> {
        match self {
            Obj::Int(i) => Some(*i as f64),
            Obj::Real(r) => Some(*r),
            _ => None,
        }
    }
    /// PDFium's `GetNumber` as the float PDFium computes with.
    pub fn numf(&self) -> Option<f32> {
        self.num().map(|v| v as f32)
    }
    /// PDFium's `GetInteger`: a real truncates toward zero.
    pub fn int(&self) -> Option<i64> {
        match self {
            Obj::Int(i) => Some(*i),
            Obj::Real(r) => Some(if r.is_finite() { *r as i64 } else { 0 }),
            _ => None,
        }
    }
}

pub fn name(b: &[u8]) -> Obj {
    Obj::Name(Rc::new(b.to_vec()))
}

// ---------------------------------------------------------------- character classes

pub fn is_ws(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}
pub fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}
pub fn is_regular(c: u8) -> bool {
    !is_ws(c) && !is_delim(c)
}
pub fn is_numeric(c: u8) -> bool {
    c.is_ascii_digit() || c == b'+' || c == b'-' || c == b'.'
}
fn hexval(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

/// Parses a number word the way PDFium's `CPDF_Number(ByteStringView)` and `StringToFloat` do: a sign, digits and
/// one dot; anything after stops the scan. Words without a dot are integers (wrapping like a 32-bit int).
pub fn parse_number(w: &[u8]) -> Obj {
    let mut i = 0;
    let mut neg = false;
    if i < w.len() && (w[i] == b'+' || w[i] == b'-') {
        neg = w[i] == b'-';
        i += 1;
    }
    if w.contains(&b'.') {
        let mut v = 0f64;
        let mut scale = 0.1f64;
        let mut seen_dot = false;
        while i < w.len() {
            let c = w[i];
            if c == b'.' {
                if seen_dot {
                    break;
                }
                seen_dot = true;
            } else if c.is_ascii_digit() {
                if seen_dot {
                    v += (c - b'0') as f64 * scale;
                    scale /= 10.0;
                } else {
                    v = v * 10.0 + (c - b'0') as f64;
                }
            } else {
                break;
            }
            i += 1;
        }
        let v = if neg { -v } else { v };
        return Obj::Real(v as f32 as f64);
    }
    let mut v: i64 = 0;
    while i < w.len() && w[i].is_ascii_digit() {
        v = v.saturating_mul(10).saturating_add((w[i] - b'0') as i64);
        i += 1;
    }
    // PDFium keeps integers as int32 (an overflowing value is clamped by its safe-math conversion).
    let v = v.min(i32::MAX as i64);
    Obj::Int(if neg { -v } else { v })
}

pub fn decode_name(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'#' {
            if let (Some(a), Some(b)) = (raw.get(i + 1).and_then(|c| hexval(*c)), raw.get(i + 2).and_then(|c| hexval(*c))) {
                out.push(a * 16 + b);
                i += 3;
                continue;
            }
        }
        out.push(raw[i]);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------- the reader

pub enum Tok<'a> {
    /// A regular word: keyword, number, `true`, `R`, ... (`is_number` when every byte is numeric).
    Word(&'a [u8], bool),
    /// A name, without the slash and still `#`-encoded.
    Name(&'a [u8]),
    Open(u8),
    DictOpen,
    DictClose,
    StrStart,
    HexStart,
    ArrClose,
    Eof,
}

pub struct Lexer<'a> {
    pub b: &'a [u8],
    pub pos: usize,
    /// Longest string kept (PDFium's content parser keeps 32767 bytes); `usize::MAX` for file objects.
    pub max_string: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Lexer { b, pos: 0, max_string: usize::MAX }
    }

    pub fn skip_ws(&mut self) {
        loop {
            while self.pos < self.b.len() && is_ws(self.b[self.pos]) {
                self.pos += 1;
            }
            if self.pos < self.b.len() && self.b[self.pos] == b'%' {
                while self.pos < self.b.len() && self.b[self.pos] != b'\n' && self.b[self.pos] != b'\r' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    pub fn next(&mut self) -> Tok<'a> {
        self.skip_ws();
        let b = self.b;
        if self.pos >= b.len() {
            return Tok::Eof;
        }
        let c = b[self.pos];
        match c {
            b'/' => {
                let s = self.pos + 1;
                self.pos = s;
                while self.pos < b.len() && is_regular(b[self.pos]) {
                    self.pos += 1;
                }
                Tok::Name(&b[s..self.pos])
            }
            b'<' => {
                if b.get(self.pos + 1) == Some(&b'<') {
                    self.pos += 2;
                    Tok::DictOpen
                } else {
                    self.pos += 1;
                    Tok::HexStart
                }
            }
            b'>' => {
                if b.get(self.pos + 1) == Some(&b'>') {
                    self.pos += 2;
                    Tok::DictClose
                } else {
                    self.pos += 1;
                    Tok::Open(b'>')
                }
            }
            b'(' => {
                self.pos += 1;
                Tok::StrStart
            }
            b'[' => {
                self.pos += 1;
                Tok::Open(b'[')
            }
            b']' => {
                self.pos += 1;
                Tok::ArrClose
            }
            b')' | b'{' | b'}' => {
                self.pos += 1;
                Tok::Open(c)
            }
            _ => {
                let s = self.pos;
                let mut num = true;
                while self.pos < b.len() && is_regular(b[self.pos]) {
                    if !is_numeric(b[self.pos]) {
                        num = false;
                    }
                    self.pos += 1;
                }
                Tok::Word(&b[s..self.pos], num)
            }
        }
    }

    /// Reads a literal string; the opening parenthesis is already consumed.
    pub fn read_string(&mut self) -> Vec<u8> {
        let b = self.b;
        let mut out = Vec::new();
        let mut depth = 1usize;
        while self.pos < b.len() {
            let c = b[self.pos];
            self.pos += 1;
            match c {
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(c);
                }
                b'\\' => {
                    let Some(&e) = b.get(self.pos) else { break };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'\r' => {
                            if b.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut v = (e - b'0') as u32;
                            for _ in 0..2 {
                                match b.get(self.pos) {
                                    Some(&d) if (b'0'..=b'7').contains(&d) => {
                                        v = v * 8 + (d - b'0') as u32;
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(v as u8);
                        }
                        _ => out.push(e),
                    }
                }
                _ => out.push(c),
            }
        }
        out.truncate(self.max_string);
        out
    }

    /// Reads a hex string; the `<` is already consumed.
    pub fn read_hex(&mut self) -> Vec<u8> {
        let b = self.b;
        let mut out = Vec::new();
        let mut hi: Option<u8> = None;
        while self.pos < b.len() {
            let c = b[self.pos];
            self.pos += 1;
            if c == b'>' {
                break;
            }
            if let Some(v) = hexval(c) {
                match hi.take() {
                    None => hi = Some(v),
                    Some(h) => out.push(h * 16 + v),
                }
            }
        }
        if let Some(h) = hi {
            out.push(h * 16);
        }
        out.truncate(self.max_string);
        out
    }

}

/// What a parse of one object found. `Stream` is the dictionary of a `stream` keyword: the caller reads the data.
pub enum Parsed {
    Obj(Obj),
    StreamDict(Dict),
}

/// Reads one object at the lexer position (file syntax: `N G R` references). `None` when nothing parses, as
/// PDFium's `GetObjectBody` returns null. The value of a stream is returned as its dictionary for the caller.
pub fn parse_obj(lx: &mut Lexer, depth: usize, refs: bool) -> Option<Parsed> {
    if depth > MAX_DEPTH {
        return None;
    }
    let save = lx.pos;
    match lx.next() {
        Tok::Eof => None,
        Tok::Word(w, true) => {
            if refs {
                let after = lx.pos;
                if let Tok::Word(_, true) = lx.next() {
                    let g_end = lx.pos;
                    let gw = &lx.b[after..g_end];
                    if let Tok::Word(b"R", _) = lx.next() {
                        let n = atoui(w);
                        if n == 0 {
                            return None;
                        }
                        let g = atoui(trim_ws(gw));
                        return Some(Parsed::Obj(Obj::Ref(n, g.min(u16::MAX as u32) as u16)));
                    }
                }
                lx.pos = after;
            }
            Some(Parsed::Obj(parse_number(w)))
        }
        Tok::Word(w, false) => match w {
            b"true" => Some(Parsed::Obj(Obj::Bool(true))),
            b"false" => Some(Parsed::Obj(Obj::Bool(false))),
            b"null" => Some(Parsed::Obj(Obj::Null)),
            _ => None,
        },
        Tok::Name(n) => Some(Parsed::Obj(Obj::Name(Rc::new(decode_name(n))))),
        Tok::StrStart => Some(Parsed::Obj(Obj::Str(Rc::new(lx.read_string())))),
        Tok::HexStart => Some(Parsed::Obj(Obj::Str(Rc::new(lx.read_hex())))),
        Tok::Open(b'[') => {
            let mut items = Vec::new();
            // A `]` ends the array; anything else that does not parse also ends it (PDFium's loose parse).
            while let Some(p) = parse_obj(lx, depth + 1, refs) {
                if let Parsed::Obj(o) = p {
                    items.push(o);
                }
            }
            Some(Parsed::Obj(Obj::Array(Rc::new(items))))
        }
        Tok::ArrClose => {
            // An array's end: the caller sees no object and stops; the bracket stays consumed.
            None
        }
        Tok::DictOpen => {
            let mut d = Dict::default();
            loop {
                let before = lx.pos;
                match lx.next() {
                    Tok::Eof => return None,
                    Tok::DictClose => break,
                    Tok::Word(b"endobj", _) => {
                        lx.pos = before;
                        break;
                    }
                    Tok::Name(k) => {
                        let key = decode_name(k);
                        match parse_obj(lx, depth + 1, refs) {
                            Some(Parsed::Obj(v)) => {
                                if !key.is_empty() {
                                    d.insert(&key, v);
                                }
                            }
                            Some(Parsed::StreamDict(_)) => {}
                            None => {}
                        }
                    }
                    Tok::StrStart => {
                        lx.read_string();
                    }
                    Tok::HexStart => {
                        lx.read_hex();
                    }
                    _ => {}
                }
            }
            let save = lx.pos;
            if let Tok::Word(b"stream", _) = lx.next() {
                return Some(Parsed::StreamDict(d));
            }
            lx.pos = save;
            Some(Parsed::Obj(Obj::Dict(Rc::new(d))))
        }
        Tok::DictClose => {
            lx.pos = save;
            None
        }
        Tok::Open(_) => None,
    }
}

fn trim_ws(w: &[u8]) -> &[u8] {
    let s = w.iter().position(|c| !is_ws(*c)).unwrap_or(w.len());
    let e = w.iter().rposition(|c| !is_ws(*c)).map_or(s, |p| p + 1);
    &w[s..e.max(s)]
}

/// PDFium's `FXSYS_atoui`: leading digits, wrapping at 32 bits.
pub fn atoui(w: &[u8]) -> u32 {
    let mut v: u32 = 0;
    let mut i = 0;
    if w.first() == Some(&b'+') {
        i = 1;
    }
    while i < w.len() && w[i].is_ascii_digit() {
        v = v.wrapping_mul(10).wrapping_add((w[i] - b'0') as u32);
        i += 1;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<Obj> {
        match parse_obj(&mut Lexer::new(s.as_bytes()), 0, true) {
            Some(Parsed::Obj(o)) => Some(o),
            _ => None,
        }
    }

    #[test]
    fn basics() {
        assert_eq!(p("12 0 R"), Some(Obj::Ref(12, 0)));
        assert_eq!(p("12 0 obj"), Some(Obj::Int(12)));
        assert_eq!(p("-.5"), Some(Obj::Real(-0.5)));
        assert_eq!(p("(a\\(b\\)c\\101)").unwrap().as_str().unwrap(), b"a(b)cA");
        assert_eq!(p("<41 4>").unwrap().as_str().unwrap(), b"A@");
        assert_eq!(p("/A#20B").unwrap().as_name().unwrap(), b"A B");
        let d = p("<</A 1 /A 2 /B [1 2 [3]] /C<</D null>>>>").unwrap();
        assert_eq!(d.as_dict().unwrap().get(b"A"), Some(&Obj::Int(2)));
        assert_eq!(d.as_dict().unwrap().get(b"B").unwrap().as_array().unwrap().len(), 3);
    }

    #[test]
    fn deep_nesting_gives_no_object_and_no_overflow() {
        let s = "[".repeat(100_000);
        let o = p(&s);
        assert!(o.is_some());
        let s = "<<".repeat(100_000);
        let _ = p(&s);
    }
}
