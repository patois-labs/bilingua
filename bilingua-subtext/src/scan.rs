//! The small hand-written matchers of the LocOS `subtitle_text_filter` patterns. Python's `\d` is any Unicode
//! decimal digit, `isspace`/`strip` are Python's, and a line ends at `\r\n`, `\r` or `\n` only; all of it is
//! reproduced here so the same files are read the same way.

use bilingua_model::{py_isdecimal_char, py_isspace_char};

/// One line of the text: byte offsets (the end excludes the terminator) and its 1-based number.
#[derive(Clone, Copy)]
pub struct Line<'a> {
    pub start: usize,
    pub end: usize,
    pub text: &'a str,
    pub no: usize,
}

pub struct Lines<'a> {
    text: &'a str,
    pos: usize,
    no: usize,
}

pub fn lines(text: &str) -> Lines<'_> {
    Lines { text, pos: 0, no: 0 }
}

impl<'a> Iterator for Lines<'a> {
    type Item = Line<'a>;
    fn next(&mut self) -> Option<Line<'a>> {
        let b = self.text.as_bytes();
        if self.pos >= b.len() {
            return None;
        }
        let start = self.pos;
        let end = b[start..].iter().position(|&c| c == b'\r' || c == b'\n').map_or(b.len(), |p| start + p);
        self.pos = if end >= b.len() {
            b.len()
        } else if b[end] == b'\r' && b.get(end + 1) == Some(&b'\n') {
            end + 2
        } else {
            end + 1
        };
        self.no += 1;
        Some(Line { start, end, text: &self.text[start..end], no: self.no })
    }
}

/// The first line terminator of the text (`\n` when it has none).
pub fn first_newline(text: &str) -> &'static str {
    let b = text.as_bytes();
    match b.iter().position(|&c| c == b'\r' || c == b'\n') {
        Some(p) if b[p] == b'\r' && b.get(p + 1) == Some(&b'\n') => "\r\n",
        Some(p) if b[p] == b'\r' => "\r",
        _ => "\n",
    }
}

/// `[start, end)` without leading and trailing Python whitespace.
pub fn core(text: &str, start: usize, end: usize) -> (usize, usize) {
    let s = &text[start..end];
    let lead = s.len() - s.trim_start_matches(py_isspace_char).len();
    let s2 = &s[lead..];
    let trail = s2.len() - s2.trim_end_matches(py_isspace_char).len();
    (start + lead, end - trail)
}

/// The pieces of `s` between line terminators (`\r\n`, `\r`, `\n`), like `re.split(r"\r\n|\r|\n", s)`.
pub fn split_newlines(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let (mut from, mut i) = (0, 0);
    while i < b.len() {
        match b[i] {
            b'\r' => {
                out.push(&s[from..i]);
                i += if b.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                from = i;
            }
            b'\n' => {
                out.push(&s[from..i]);
                i += 1;
                from = i;
            }
            _ => i += 1,
        }
    }
    out.push(&s[from..]);
    out
}

fn ws(s: &mut &str) {
    *s = s.trim_start_matches([' ', '\t']);
}

fn eat(s: &mut &str, c: char) -> bool {
    match s.strip_prefix(c) {
        Some(r) => {
            *s = r;
            true
        }
        None => false,
    }
}

/// `\d{min,max}` followed by a non-digit; returns the matched digits.
fn digits<'a>(s: &mut &'a str, min: usize, max: usize) -> Option<&'a str> {
    let (mut n, mut end) = (0usize, 0usize);
    for (i, c) in s.char_indices() {
        if !py_isdecimal_char(c) {
            break;
        }
        n += 1;
        if n > max {
            return None;
        }
        end = i + c.len_utf8();
    }
    if n < min {
        return None;
    }
    let (d, rest) = s.split_at(end);
    *s = rest;
    Some(d)
}

fn clock(s: &mut &str) -> Option<()> {
    digits(s, 1, 2)?;
    eat(s, ':').then_some(())?;
    digits(s, 2, 2)?;
    eat(s, ':').then_some(())?;
    digits(s, 2, 2)?;
    eat(s, '.').then_some(())?;
    digits(s, 1, 3)?;
    Some(())
}

/// `^[ \t]*H:MM:SS.fff[ \t]*,[ \t]*H:MM:SS.fff[ \t]*$` (1 to 3 fraction digits). Returns the two stamps.
pub fn time_pair(line: &str) -> Option<(&str, &str)> {
    let mut s = line;
    ws(&mut s);
    let a0 = s;
    clock(&mut s)?;
    let a = &a0[..a0.len() - s.len()];
    ws(&mut s);
    eat(&mut s, ',').then_some(())?;
    ws(&mut s);
    let b0 = s;
    clock(&mut s)?;
    let b = &b0[..b0.len() - s.len()];
    ws(&mut s);
    s.is_empty().then_some((a, b))
}

/// `^[ \t]*\[\d{2}:\d{2}:\d{2}\][ \t]*$` (SubViewer 1.0).
pub fn sv1_time(line: &str) -> bool {
    let mut s = line;
    ws(&mut s);
    let ok = (|| {
        eat(&mut s, '[').then_some(())?;
        digits(&mut s, 2, 2)?;
        eat(&mut s, ':').then_some(())?;
        digits(&mut s, 2, 2)?;
        eat(&mut s, ':').then_some(())?;
        digits(&mut s, 2, 2)?;
        eat(&mut s, ']').then_some(())
    })();
    ok.is_some() && {
        ws(&mut s);
        s.is_empty()
    }
}

/// `^([ \t]*)\{(\d{1,10})\}\{(\d{0,10})\}`: the byte length of the prefix and the two frame numbers.
pub fn mdvd_cue(line: &str) -> Option<(usize, &str, &str)> {
    let mut s = line;
    ws(&mut s);
    eat(&mut s, '{').then_some(())?;
    let first = digits(&mut s, 1, 10)?;
    eat(&mut s, '}').then_some(())?;
    eat(&mut s, '{').then_some(())?;
    let last = digits(&mut s, 0, 10)?;
    eat(&mut s, '}').then_some(())?;
    Some((line.len() - s.len(), first, last))
}

/// `^[ \t]*\d{1,3}(?:\.\d{1,6})?[ \t]*$`: a frame-rate declaration.
pub fn fps_text(body: &str) -> bool {
    let mut s = body;
    ws(&mut s);
    if digits(&mut s, 1, 3).is_none() {
        return false;
    }
    if eat(&mut s, '.') && digits(&mut s, 1, 6).is_none() {
        return false;
    }
    ws(&mut s);
    s.is_empty()
}

/// The byte length of the MicroDVD control code at the start of `s` (`\{[A-Za-z]:[^{}\r\n]{0,200}\}`), if any.
pub fn code_at(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if b.len() < 4 || b[0] != b'{' || !b[1].is_ascii_alphabetic() || b[2] != b':' {
        return None;
    }
    let mut count = 0;
    for (i, c) in s[3..].char_indices() {
        match c {
            '}' => return Some(3 + i + 1),
            '{' | '\r' | '\n' => return None,
            _ => {
                count += 1;
                if count > 200 {
                    return None;
                }
            }
        }
    }
    None
}

/// Byte ranges of every control code in `s`, left to right, not overlapping (`finditer`).
pub fn find_codes(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            if let Some(n) = code_at(&s[i..]) {
                out.push((i, i + n));
                i += n;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// How many control codes `s` holds.
pub fn count_codes(s: &str) -> usize {
    find_codes(s).len()
}

/// The length of `(?:[ \t]*CODE)*[ \t]*` at the start of `body`.
pub fn lead_end(body: &str) -> usize {
    let skip = |from: usize| from + body[from..].bytes().take_while(|&c| c == b' ' || c == b'\t').count();
    let mut pos = 0;
    loop {
        let p = skip(pos);
        match code_at(&body[p..]) {
            Some(n) => pos = p + n,
            None => break,
        }
    }
    skip(pos)
}

/// `_PLACEHOLDER_RE.sub("", s)`: removes LocOS inline tokens (`⟦/?[a-z]?\d+⟧`).
pub fn strip_placeholders(s: &str) -> String {
    const OPEN: char = '\u{27E6}';
    if !s.contains(OPEN) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find(OPEN) {
        out.push_str(&rest[..p]);
        let tok = &rest[p + OPEN.len_utf8()..];
        match token_len(tok) {
            Some(n) => rest = &tok[n..],
            None => {
                out.push(OPEN);
                rest = tok;
            }
        }
    }
    out.push_str(rest);
    out
}

/// After the opening bracket: `/?[a-z]?\d+` and the closing bracket; the length through it.
fn token_len(s: &str) -> Option<usize> {
    let mut it = s.char_indices().peekable();
    if it.peek().is_some_and(|(_, c)| *c == '/') {
        it.next();
    }
    if it.peek().is_some_and(|(_, c)| c.is_ascii_lowercase()) {
        it.next();
    }
    let mut n = 0;
    while let Some(&(_, c)) = it.peek() {
        if !py_isdecimal_char(c) {
            break;
        }
        n += 1;
        it.next();
    }
    match it.next() {
        Some((i, '\u{27E7}')) if n > 0 => Some(i + '\u{27E7}'.len_utf8()),
        _ => None,
    }
}

/// `re.sub(r"[ \t]*(?:\r\n|\r|\n)+[ \t]*", joiner, s)`.
pub fn collapse_newlines(s: &str, joiner: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let (mut from, mut i) = (0, 0);
    while i < b.len() {
        if matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            let mut j = i;
            while j < b.len() && matches!(b[j], b' ' | b'\t') {
                j += 1;
            }
            if j < b.len() && matches!(b[j], b'\r' | b'\n') {
                while j < b.len() && matches!(b[j], b'\r' | b'\n') {
                    j += 1;
                }
                while j < b.len() && matches!(b[j], b' ' | b'\t') {
                    j += 1;
                }
                out.push_str(&s[from..i]);
                out.push_str(joiner);
                from = j;
                i = j;
                continue;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    out.push_str(&s[from..]);
    out
}
