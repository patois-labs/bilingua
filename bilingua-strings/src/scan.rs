//! Byte decoding, the tokenizer and the entry scanner for `.strings` text.

use bilingua_model::Error;

/// How the file's bytes were encoded; the writer produces exactly this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enc {
    Utf8,
    Utf8Bom,
    Utf16LeBom,
    Utf16BeBom,
    Utf16Le,
    Utf16Be,
}

impl Enc {
    pub fn name(self) -> &'static str {
        match self {
            Enc::Utf8 => "utf-8",
            Enc::Utf8Bom => "utf-8-bom",
            Enc::Utf16LeBom => "utf-16le-bom",
            Enc::Utf16BeBom => "utf-16be-bom",
            Enc::Utf16Le => "utf-16le",
            Enc::Utf16Be => "utf-16be",
        }
    }

    pub fn from_name(s: &str) -> Option<Enc> {
        [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16LeBom, Enc::Utf16BeBom, Enc::Utf16Le, Enc::Utf16Be].into_iter().find(|e| e.name() == s)
    }

    pub fn encode(self, text: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(text.len() + 4);
        match self {
            Enc::Utf8 => out.extend_from_slice(text.as_bytes()),
            Enc::Utf8Bom => {
                out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
                out.extend_from_slice(text.as_bytes());
            }
            Enc::Utf16LeBom | Enc::Utf16Le => {
                if self == Enc::Utf16LeBom {
                    out.extend_from_slice(&[0xFF, 0xFE]);
                }
                for u in text.encode_utf16() {
                    out.extend_from_slice(&u.to_le_bytes());
                }
            }
            Enc::Utf16BeBom | Enc::Utf16Be => {
                if self == Enc::Utf16BeBom {
                    out.extend_from_slice(&[0xFE, 0xFF]);
                }
                for u in text.encode_utf16() {
                    out.extend_from_slice(&u.to_be_bytes());
                }
            }
        }
        out
    }
}

fn unsupported(msg: &str) -> Error {
    Error::Unsupported(msg.into())
}

fn utf16(body: &[u8], big: bool) -> Result<String, Error> {
    if !body.len().is_multiple_of(2) {
        return Err(unsupported("this .strings file is not valid UTF-16 (odd number of bytes); re-save the file as UTF-8"));
    }
    let units = body.chunks_exact(2).map(|c| if big { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) });
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .map_err(|_| unsupported("this .strings file is not valid UTF-16 (unpaired surrogate); re-save the file as UTF-8"))
}

/// BOM-less UTF-16: an ASCII-heavy text has a NUL in every other byte. Some(true) = big endian.
fn sniff_utf16(data: &[u8]) -> Option<bool> {
    if data.len() < 4 || !data.len().is_multiple_of(2) {
        return None;
    }
    let (mut even, mut odd) = (0usize, 0usize);
    for c in data.chunks_exact(2) {
        even += usize::from(c[0] == 0);
        odd += usize::from(c[1] == 0);
    }
    let half = data.len() / 2;
    if odd * 10 > half * 6 && even == 0 {
        return Some(false);
    }
    if even * 10 > half * 6 && odd == 0 {
        return Some(true);
    }
    None
}

/// Bytes to text and the encoding to write back. Refuses what Xcode would not read.
pub fn decode(data: &[u8]) -> Result<(String, Enc), Error> {
    if data.starts_with(b"bplist00") {
        return Err(unsupported(
            "this .strings file is a binary property list; convert it to text first (plutil -convert xml1, or re-save it in Xcode)",
        ));
    }
    if data.starts_with(&[0xFF, 0xFE, 0, 0]) || data.starts_with(&[0, 0, 0xFE, 0xFF]) {
        return Err(unsupported("UTF-32 files are not supported; re-save the file as UTF-8"));
    }
    if let Some(rest) = data.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let t = std::str::from_utf8(rest).map_err(|_| unsupported("this .strings file is not valid UTF-8; re-save the file as UTF-8"))?;
        return Ok((t.to_string(), Enc::Utf8Bom));
    }
    if let Some(rest) = data.strip_prefix(&[0xFF, 0xFE]) {
        return Ok((utf16(rest, false)?, Enc::Utf16LeBom));
    }
    if let Some(rest) = data.strip_prefix(&[0xFE, 0xFF]) {
        return Ok((utf16(rest, true)?, Enc::Utf16BeBom));
    }
    if data.contains(&0) {
        if let Some(big) = sniff_utf16(data) {
            return Ok((utf16(data, big)?, if big { Enc::Utf16Be } else { Enc::Utf16Le }));
        }
    } else if let Ok(t) = std::str::from_utf8(data) {
        return Ok((t.to_string(), Enc::Utf8));
    }
    Err(unsupported("this .strings file is neither UTF-8 nor UTF-16, the encodings Xcode reads; re-save the file as UTF-8"))
}

/// Python `str.strip` white space (what LocOS trims around a value).
pub fn is_space(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

/// Splits a value into (leading white space, core, trailing white space).
pub fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = s.trim_matches(is_space);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead = s.len() - s.trim_start_matches(is_space).len();
    let trail_start = s.trim_end_matches(is_space).len();
    (&s[..lead], core, &s[trail_start..])
}

/// Resolves the escapes of a quoted string: `\n \r \t \a \b \f \v \" \' \\`, `\Uxxxx` (one to four hex
/// digits; a surrogate pair spans two escapes) and up to three octal digits. Any other escaped character
/// stands for itself. A lone surrogate becomes U+FFFD (a Rust string cannot hold it).
pub fn unescape(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut units: Vec<u16> = Vec::new();
    let flush = |units: &mut Vec<u16>, out: &mut String| {
        if !units.is_empty() {
            out.extend(char::decode_utf16(units.drain(..)).map(|r| r.unwrap_or('\u{fffd}')));
        }
    };
    let mut i = 0;
    let mut lit = 0; // start of the pending literal run
    while i < b.len() {
        if b[i] != b'\\' || i + 1 >= b.len() {
            i += 1; // a trailing backslash stays as a literal one
            continue;
        }
        if lit < i {
            flush(&mut units, &mut out);
            out.push_str(&s[lit..i]);
        }
        let d = b[i + 1];
        if d == b'U' || d == b'u' {
            let mut j = i + 2;
            while j < b.len() && j < i + 6 && b[j].is_ascii_hexdigit() {
                j += 1;
            }
            if j > i + 2 {
                units.push(u16::from_str_radix(&s[i + 2..j], 16).unwrap_or(0xFFFD));
                i = j;
                lit = i;
                continue;
            }
        }
        flush(&mut units, &mut out);
        if (b'0'..=b'7').contains(&d) {
            let mut j = i + 1;
            while j < b.len() && j < i + 4 && (b'0'..=b'7').contains(&b[j]) {
                j += 1;
            }
            let v = u32::from_str_radix(&s[i + 1..j], 8).unwrap_or(0xFFFD);
            out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
            i = j;
        } else {
            let ch = s[i + 1..].chars().next().unwrap_or('\\');
            out.push(match ch {
                'a' => '\u{7}',
                'b' => '\u{8}',
                'f' => '\u{c}',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'v' => '\u{b}',
                c => c,
            });
            i += 1 + ch.len_utf8();
        }
        lit = i;
    }
    flush(&mut units, &mut out);
    out.push_str(&s[lit..]);
    out
}

/// Writes a value back: only backslash, double quote, newline, CR and tab are escaped.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Comment,
    Str,
    Word,
    Eq,
    Semi,
    Open,
    Close,
}

struct Tok {
    kind: Kind,
    start: usize,
    end: usize,
}

fn unquoted(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'+' | b'/' | b':' | b'.' | b'-')
}

fn line_of(text: &str, pos: usize) -> usize {
    1 + text.as_bytes()[..pos.min(text.len())].iter().filter(|&&c| c == b'\n').count()
}

/// The next token from `*i`, or None at the end of the text.
fn next(text: &str, i: &mut usize) -> Result<Option<Tok>, String> {
    let b = text.as_bytes();
    let n = b.len();
    loop {
        if *i >= n {
            return Ok(None);
        }
        match b[*i] {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x0b => *i += 1,
            0xEF if b[*i..].starts_with(&[0xEF, 0xBB, 0xBF]) => *i += 3,
            _ => break,
        }
    }
    let s = *i;
    let c = b[s];
    if c == b'/' && b.get(s + 1) == Some(&b'*') {
        return match text[s + 2..].find("*/") {
            Some(rel) => {
                let j = s + 2 + rel;
                *i = j + 2;
                Ok(Some(Tok { kind: Kind::Comment, start: s + 2, end: j }))
            }
            None => Err(format!("this .strings file has a /* comment that is never closed (line {})", line_of(text, s))),
        };
    }
    if c == b'/' && b.get(s + 1) == Some(&b'/') {
        let mut j = s;
        while j < n && b[j] != b'\r' && b[j] != b'\n' {
            j += 1;
        }
        *i = j;
        return Ok(Some(Tok { kind: Kind::Comment, start: s + 2, end: j }));
    }
    if c == b'"' {
        let mut j = s + 1;
        while j < n {
            match b[j] {
                b'\\' => j += 2,
                b'"' => break,
                _ => j += 1,
            }
        }
        if j >= n {
            return Err(format!("this .strings file has a quoted string that is never closed (line {})", line_of(text, s)));
        }
        *i = j + 1;
        return Ok(Some(Tok { kind: Kind::Str, start: s + 1, end: j }));
    }
    let single = match c {
        b'=' => Some(Kind::Eq),
        b';' => Some(Kind::Semi),
        b'{' => Some(Kind::Open),
        b'}' => Some(Kind::Close),
        _ => None,
    };
    if let Some(kind) = single {
        *i = s + 1;
        return Ok(Some(Tok { kind, start: s, end: s + 1 }));
    }
    if unquoted(c) {
        let mut j = s;
        while j < n && unquoted(b[j]) {
            j += 1;
        }
        *i = j;
        return Ok(Some(Tok { kind: Kind::Word, start: s, end: j }));
    }
    let ch = text[s..].chars().next().unwrap_or('?');
    Err(format!("this .strings file has an unexpected character {ch:?} on line {}; entries must look like \"key\" = \"value\";", line_of(text, s)))
}

/// One `key = value;` entry. `vstart..vend` is the raw value in the text (without the quotes when quoted).
#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub vstart: usize,
    pub vend: usize,
    pub quoted: bool,
    pub comments: Vec<String>,
    pub line: usize,
}

/// Scans every entry into `out`. On error, `out` holds the entries read before it.
pub fn scan(text: &str, out: &mut Vec<Entry>) -> Result<(), String> {
    let mut comments: Vec<String> = Vec::new();
    let mut state = 0u8; // 0 key, 1 '=', 2 value, 3 ';'
    let mut key: Option<Tok> = None;
    let mut val: Option<Tok> = None;
    let (mut line_pos, mut line_no) = (0usize, 1usize);
    let mut i = 0;
    while let Some(tok) = next(text, &mut i)? {
        let at = || line_of(text, tok.start);
        if tok.kind == Kind::Comment {
            if state == 0 {
                let c = text[tok.start..tok.end].trim_matches(is_space);
                if !c.is_empty() {
                    comments.push(c.to_string());
                }
            }
            continue;
        }
        match state {
            0 => {
                if matches!(tok.kind, Kind::Open | Kind::Close) {
                    continue;
                }
                if !matches!(tok.kind, Kind::Str | Kind::Word) {
                    return Err(format!("this .strings file has a stray {:?} on line {}", &text[tok.start..tok.end], at()));
                }
                key = Some(tok);
                state = 1;
            }
            1 => {
                if tok.kind != Kind::Eq {
                    return Err(format!("this .strings file is missing '=' on line {}", at()));
                }
                state = 2;
            }
            2 => {
                if !matches!(tok.kind, Kind::Str | Kind::Word) {
                    return Err(format!("this .strings file is missing a value on line {}", at()));
                }
                val = Some(tok);
                state = 3;
            }
            _ => {
                if tok.kind != Kind::Semi {
                    return Err(format!("this .strings file is missing ';' on line {}", at()));
                }
                let (Some(k), Some(v)) = (key.take(), val.take()) else { return Err("internal scanner state".into()) };
                line_no += text.as_bytes()[line_pos..k.start].iter().filter(|&&c| c == b'\n').count();
                line_pos = k.start;
                let kraw = &text[k.start..k.end];
                let vraw = &text[v.start..v.end];
                out.push(Entry {
                    key: if k.kind == Kind::Str { unescape(kraw) } else { kraw.to_string() },
                    value: if v.kind == Kind::Str { unescape(vraw) } else { vraw.to_string() },
                    vstart: v.start,
                    vend: v.end,
                    quoted: v.kind == Kind::Str,
                    comments: std::mem::take(&mut comments),
                    line: line_no,
                });
                state = 0;
            }
        }
    }
    if state != 0 {
        return Err("this .strings file ends in the middle of an entry".into());
    }
    Ok(())
}
