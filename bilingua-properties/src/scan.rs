//! Reading a `.properties` file: decoding, the logical-line walk and the escapes (`java.util.Properties.load`).

use crate::moz;
use bilingua_model::Error;
use std::borrow::Cow;

const FORMAT: &str = "properties";

/// How translated text is written back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Everything above U+007E becomes `\uXXXX` (the native2ascii convention).
    Ascii,
    /// ISO-8859-1 file: characters above U+00FF become `\uXXXX`.
    Latin1,
    /// Literal UTF-8.
    Utf8,
}

pub(crate) struct Decoded<'a> {
    pub text: Cow<'a, str>,
    pub mode: Mode,
    pub bom: bool,
}

impl Mode {
    /// The skeleton's `encoding` string for this mode.
    pub fn label(self, bom: bool) -> &'static str {
        match (self, bom) {
            (Mode::Utf8, false) => "utf-8",
            (Mode::Utf8, true) => "utf-8-bom",
            (Mode::Ascii, false) => "ascii",
            (Mode::Ascii, true) => "ascii-bom",
            (Mode::Latin1, _) => "iso-8859-1",
        }
    }

    pub fn from_label(s: &str) -> Option<(Mode, bool)> {
        Some(match s {
            "utf-8" => (Mode::Utf8, false),
            "utf-8-bom" => (Mode::Utf8, true),
            "ascii" => (Mode::Ascii, false),
            "ascii-bom" => (Mode::Ascii, true),
            "iso-8859-1" => (Mode::Latin1, false),
            _ => return None,
        })
    }
}

fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse { format: FORMAT.into(), msg: msg.into() }
}

/// True when the bytes are not text: a NUL, or more C0 controls than 1% of the file (plus two). Tab, line feed,
/// vertical tab, form feed, carriage return, SUB and ESC are fine.
fn looks_binary(data: &[u8]) -> bool {
    if data.contains(&0) {
        return true;
    }
    let bad = data.iter().filter(|&&b| b < 0x20 && !matches!(b, 9..=13 | 0x1A | 0x1B)).count();
    bad > 2 + data.len() / 100
}

fn binary_error() -> Error {
    Error::Unsupported("this .properties file looks like a binary file, not text; save it as UTF-8 and try again".into())
}

/// Bytes to text the way Java reads a bundle: UTF-8 first (a byte-order mark is kept), ISO-8859-1 when the bytes
/// are not UTF-8. UTF-16 and UTF-32 are refused.
pub(crate) fn decode(data: &[u8]) -> Result<Decoded<'_>, Error> {
    if data.starts_with(&[0xFF, 0xFE, 0, 0]) || data.starts_with(&[0, 0, 0xFE, 0xFF]) {
        return Err(Error::Unsupported("UTF-32 files; save the file as UTF-8".into()));
    }
    if data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]) {
        return Err(Error::Unsupported(
            "this .properties file is UTF-16; Java reads .properties as UTF-8 or ISO-8859-1 only, so save it as UTF-8".into(),
        ));
    }
    let (body, bom) = match data.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(r) => (r, true),
        None => (data, false),
    };
    match std::str::from_utf8(body) {
        Ok(text) => {
            if body.contains(&0) {
                return Err(binary_error());
            }
            let mode = if !text.is_ascii() || moz::looks_mozilla(text) { Mode::Utf8 } else { Mode::Ascii };
            Ok(Decoded { text: Cow::Borrowed(text), mode, bom })
        }
        Err(_) => {
            if looks_binary(data) {
                return Err(binary_error());
            }
            let text: String = data.iter().map(|&b| b as char).collect();
            Ok(Decoded { text: Cow::Owned(text), mode: Mode::Latin1, bom: false })
        }
    }
}

/// Characters Python (and so Localization OS) treats as white space.
pub(crate) fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

pub(crate) fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// Leading white space, the text, trailing white space.
pub(crate) fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = strip(s);
    if core.is_empty() {
        return (s, "", "");
    }
    let lead = s.len() - s.trim_start_matches(is_space).len();
    let trail = s.len() - s.trim_end_matches(is_space).len();
    (&s[..lead], core, &s[s.len() - trail..])
}

#[derive(Debug)]
pub(crate) struct Entry {
    pub key: String,
    /// Decoded value (escapes resolved).
    pub value: String,
    /// Raw value span in the text.
    pub start: usize,
    pub end: usize,
    /// 1-based line of the key.
    pub line: usize,
    /// Comment lines directly above (a blank line ends the block), without the `#` or `!`.
    pub comment: Vec<String>,
}

fn is_blank(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | 0x0C)
}

/// End of the natural line starting at `pos` and the start of the next one.
fn line_end(b: &[u8], pos: usize) -> (usize, usize) {
    let eol = b[pos..].iter().position(|&c| c == b'\n' || c == b'\r').map_or(b.len(), |p| pos + p);
    let next = if b[eol..].starts_with(b"\r\n") {
        eol + 2
    } else if eol < b.len() {
        eol + 1
    } else {
        eol
    };
    (eol, next)
}

/// Number of line breaks (`\n`, `\r`, `\r\n` count once) in `b[from..to]`.
fn count_breaks(b: &[u8], from: usize, to: usize) -> usize {
    let mut n = 0;
    let mut i = from;
    while i < to {
        match b[i] {
            b'\n' => n += 1,
            b'\r' => {
                n += 1;
                if b.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    n
}

fn char_len(s: &str, at: usize) -> usize {
    s[at..].chars().next().map_or(1, char::len_utf8)
}

/// Every entry in file order. Errors on a malformed `\uXXXX` escape in a key or value.
pub(crate) fn scan(text: &str) -> Result<Vec<Entry>, Error> {
    let b = text.as_bytes();
    let n = b.len();
    let mut entries = Vec::new();
    let mut comment: Vec<String> = Vec::new();
    let mut pos = 0usize;
    let mut line = 1usize;
    let mut logical = String::new();
    while pos < n {
        let (eol, nxt) = line_end(b, pos);
        let mut s = pos;
        while s < eol && is_blank(b[s]) {
            s += 1;
        }
        if s == eol {
            comment.clear();
            pos = nxt;
            line += 1;
            continue;
        }
        if b[s] == b'#' || b[s] == b'!' {
            comment.push(strip(&text[s + 1..eol]).to_string());
            pos = nxt;
            line += 1;
            continue;
        }
        let entry_line = line;
        let mut pieces: Vec<(usize, usize)> = Vec::new();
        let (mut start, mut end, mut cur_next) = (s, eol, nxt);
        loop {
            let mut bs = 0;
            while end > start + bs && b[end - bs - 1] == b'\\' {
                bs += 1;
            }
            if bs % 2 == 1 && cur_next < n {
                pieces.push((start, end - 1));
                let mut p = cur_next;
                let (e2, n2) = line_end(b, p);
                while p < e2 && is_blank(b[p]) {
                    p += 1;
                }
                start = p;
                end = e2;
                cur_next = n2;
                continue;
            }
            pieces.push((start, end));
            break;
        }
        // The logical line: the pieces joined.
        let single = pieces.len() == 1;
        let lg: &str = if single {
            &text[pieces[0].0..pieces[0].1]
        } else {
            logical.clear();
            for &(a, z) in &pieces {
                logical.push_str(&text[a..z]);
            }
            &logical
        };
        let lb = lg.as_bytes();
        let ln = lb.len();
        let mut i = 0usize;
        while i < ln {
            let c = lb[i];
            if c == b'\\' {
                i += 1;
                if i < ln {
                    i += char_len(lg, i);
                }
                continue;
            }
            if c == b'=' || c == b':' || is_blank(c) {
                break;
            }
            i += 1;
        }
        let key_raw = &lg[..i.min(ln)];
        while i < ln && is_blank(lb[i]) {
            i += 1;
        }
        if i < ln && (lb[i] == b'=' || lb[i] == b':') {
            i += 1;
            while i < ln && is_blank(lb[i]) {
                i += 1;
            }
        }
        let value_raw = &lg[i..];
        let vstart = if single {
            pieces[0].0 + i
        } else if i >= ln {
            pieces[pieces.len() - 1].1
        } else {
            let mut offsets = Vec::with_capacity(pieces.len());
            let mut acc = 0;
            for &(a, z) in &pieces {
                offsets.push(acc);
                acc += z - a;
            }
            let k = offsets.partition_point(|&o| o <= i) - 1;
            pieces[k].0 + (i - offsets[k])
        };
        entries.push(Entry {
            key: unescape(key_raw)?.into_owned(),
            value: unescape(value_raw)?.into_owned(),
            start: vstart,
            end: pieces[pieces.len() - 1].1,
            line: entry_line,
            comment: std::mem::take(&mut comment),
        });
        // Lines consumed by this entry (continuations and the final break).
        line += count_breaks(b, pos, cur_next);
        pos = cur_next;
    }
    Ok(entries)
}

fn hex4(s: &str) -> Option<u32> {
    let mut v = 0u32;
    let mut count = 0;
    for c in s.chars().take(4) {
        v = v * 16 + c.to_digit(16)?;
        count += 1;
    }
    (count == 4).then_some(v)
}

/// Resolves `\t \n \r \f \uXXXX` and `\x` for any other `x`. A lone trailing backslash is dropped, and a
/// surrogate that has no partner becomes U+FFFD (the file itself is never changed by that).
pub(crate) fn unescape(s: &str) -> Result<Cow<'_, str>, Error> {
    if !s.contains('\\') {
        return Ok(Cow::Borrowed(s));
    }
    let mut out = String::with_capacity(s.len());
    let mut high: Option<u32> = None;
    let mut it = s.char_indices();
    while let Some((i, c)) = it.next() {
        if c != '\\' {
            if high.take().is_some() {
                out.push('\u{FFFD}');
            }
            out.push(c);
            continue;
        }
        let Some((_, d)) = it.next() else { break };
        if d == 'u' {
            let Some(v) = hex4(&s[i + 2..]) else {
                let shown: String = s[i..].chars().take(6).collect();
                return Err(parse_err(format!(
                    "this .properties file has a malformed \\uXXXX escape ({shown}); Java refuses it too, so fix the escape and try again"
                )));
            };
            for _ in 0..4 {
                it.next();
            }
            match v {
                0xD800..=0xDBFF => {
                    if high.replace(v).is_some() {
                        out.push('\u{FFFD}');
                    }
                }
                0xDC00..=0xDFFF => match high.take() {
                    Some(h) => out.push(char::from_u32(0x10000 + ((h - 0xD800) << 10) + (v - 0xDC00)).unwrap_or('\u{FFFD}')),
                    None => out.push('\u{FFFD}'),
                },
                _ => {
                    if high.take().is_some() {
                        out.push('\u{FFFD}');
                    }
                    out.push(char::from_u32(v).unwrap_or('\u{FFFD}'));
                }
            }
            continue;
        }
        if high.take().is_some() {
            out.push('\u{FFFD}');
        }
        out.push(match d {
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            'f' => '\u{0C}',
            other => other,
        });
    }
    if high.is_some() {
        out.push('\u{FFFD}');
    }
    Ok(Cow::Owned(out))
}
