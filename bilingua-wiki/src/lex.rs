//! Character classes the way Python's `re` and `str` see them, and small scanning helpers.
//!
//! The Localization OS filter is a set of Python regular expressions with look-behind, look-ahead and lazy
//! quantifiers. The `regex` crate has none of those, so every pattern is a small hand-written scanner in this
//! crate (iterative, no recursion over the input); the comments say which pattern each one stands for. All
//! offsets are byte offsets into a `&str`; the structural characters are ASCII, so they always sit on character
//! boundaries.

use regex::Regex;
use std::sync::OnceLock;

/// The mask character: a protected region of the source is replaced by one NUL byte per byte of the region.
pub const MASK: u8 = 0;

/// Python `str.isspace` (and `\s`): Unicode white space plus the four information separators `\x1c`..`\x1f`.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

/// Python `str.strip()`.
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

fn matches_class(cell: &'static OnceLock<Regex>, pat: &str, c: char) -> bool {
    let re = cell.get_or_init(|| Regex::new(pat).expect("category pattern"));
    let mut b = [0u8; 4];
    re.is_match(c.encode_utf8(&mut b))
}

/// Python `str.isalpha` for one character (Unicode category L*).
pub fn is_letter(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic();
    }
    // common scripts need no table look-up
    match c as u32 {
        0xC0..=0x24F => return c != '\u{d7}' && c != '\u{f7}',
        0x3041..=0x3096 | 0x30A1..=0x30FA | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7A3 => return true,
        0x3A2 => return false, // unassigned, inside the Greek capitals
        0x391..=0x3A9 | 0x3B1..=0x3C9 | 0x410..=0x44F => return true,
        0x621..=0x64A => return true,
        _ => {}
    }
    static R: OnceLock<Regex> = OnceLock::new();
    c.is_alphabetic() && matches_class(&R, r"^\p{L}$", c)
}

/// Python `str.isalnum` for one character (letters and numbers).
pub fn is_alnum(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric();
    }
    if is_letter(c) {
        return true;
    }
    static R: OnceLock<Regex> = OnceLock::new();
    c.is_alphanumeric() && matches_class(&R, r"^[\p{L}\p{N}]$", c)
}

/// Python `\w` (alphanumeric or underscore).
pub fn is_word(c: char) -> bool {
    c == '_' || is_alnum(c)
}

/// Start of each run of ten decimal digits (Unicode category Nd), for Python's `\d`.
const ND_STARTS: &[u32] = &[
    0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66, 0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090,
    0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50,
    0xABF0, 0xFF10, 0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650, 0x116C0, 0x11730,
    0x118E0, 0x11950, 0x11C50, 0x11D50, 0x11DA0, 0x11F50, 0x16A60, 0x16AC0, 0x16B50, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6,
    0x1E140, 0x1E2F0, 0x1E4F0, 0x1E950, 0x1FBF0,
];

/// Python `\d` for one character (Unicode category Nd).
pub fn is_decimal(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    let u = c as u32;
    ND_STARTS.iter().any(|s| (*s..*s + 10).contains(&u))
}

/// Whether `s` holds a letter (the mask character is not one).
pub fn has_alpha(s: &str) -> bool {
    s.chars().any(is_letter)
}

pub fn any_letter_in(inl: &[bilingua_model::Inline]) -> bool {
    inl.iter().any(|i| matches!(i, bilingua_model::Inline::Text(t) if has_alpha(t)))
}

/// The char starting at byte `i`.
pub fn char_at(s: &str, i: usize) -> Option<char> {
    s.get(i..)?.chars().next()
}

/// The char ending at byte `i`.
pub fn char_before(s: &str, i: usize) -> Option<char> {
    s.get(..i)?.chars().next_back()
}

/// `s[a..b]`, empty when the offsets are not on character boundaries (never a panic).
pub fn sl(s: &str, a: usize, b: usize) -> &str {
    s.get(a..b).unwrap_or("")
}

/// Whether `bytes[i..limit]` starts with `lit`.
pub fn at(bytes: &[u8], i: usize, limit: usize, lit: &[u8]) -> bool {
    i + lit.len() <= limit && bytes.get(i..i + lit.len()) == Some(lit)
}

/// The lazy pattern `[^\r\n]{min,max}?` followed by `closer`, starting at `from` and bounded by `limit`: the
/// smallest count of characters for which the closer follows. Returns (end of the inner text, end of the closer).
pub fn lazy_close(m: &str, from: usize, limit: usize, min: usize, max: usize, closer: &[u8]) -> Option<(usize, usize)> {
    let bytes = m.as_bytes();
    let mut i = from;
    let mut k = 0usize;
    loop {
        if k >= min && at(bytes, i, limit, closer) {
            return Some((i, i + closer.len()));
        }
        if i >= limit || k >= max {
            return None;
        }
        let c = *bytes.get(i)?;
        if c == b'\r' || c == b'\n' {
            return None;
        }
        i += utf8_len(c);
        k += 1;
    }
}

/// The next position at or after a query where a byte pattern starts, remembered between queries: while queries move
/// forward and stay before the answer, the answer stands, so a walk that asks at every position costs one pass in
/// all (a pattern scan per query would cost its bound, up to 2000 characters, at every position).
#[derive(Debug, Default, Clone, Copy)]
pub struct Seek {
    used: bool,
    from: usize,
    at: Option<usize>,
}

impl Seek {
    pub fn find(&mut self, b: &[u8], q: usize, hit: impl Fn(&[u8], usize) -> bool) -> Option<usize> {
        if self.used && q >= self.from && self.at.is_none_or(|a| q <= a) {
            return self.at;
        }
        self.used = true;
        self.from = q;
        self.at = (q..b.len()).find(|&i| hit(b, i));
        self.at
    }
}

/// Characters between two positions, counted with two forward cursors from one origin (for the character bounds of
/// the lazy patterns, without a count per query).
#[derive(Debug, Default, Clone, Copy)]
pub struct Chars {
    used: bool,
    origin: usize,
    cur: [(usize, usize); 2],
}

impl Chars {
    fn upto(&mut self, b: &[u8], k: usize, x: usize) -> usize {
        let (pos, n) = self.cur[k];
        let (pos, n) = if x >= pos { (pos, n) } else { (self.origin, 0) };
        let add = b[pos..x].iter().filter(|c| (**c & 0xC0) != 0x80).count();
        self.cur[k] = (x, n + add);
        n + add
    }

    /// Characters in `b[from..to]`.
    pub fn between(&mut self, b: &[u8], from: usize, to: usize) -> usize {
        if !self.used || from < self.origin {
            *self = Chars { used: true, origin: from, cur: [(from, 0), (from, 0)] };
        }
        let a = self.upto(b, 0, from);
        self.upto(b, 1, to) - a
    }
}

/// The search state of one tokenizer: where the next closer, `]` and line end are.
#[derive(Debug, Default, Clone, Copy)]
pub struct Seekers {
    pub link_close: Seek,
    pub foot_close: Seek,
    pub bracket: Seek,
    pub line_end: Seek,
    pub chars: Chars,
}

fn is_line_end(b: &[u8], i: usize) -> bool {
    matches!(b[i], b'\r' | b'\n')
}

impl Seekers {
    /// [`lazy_close`] for a closer that is not also the opener (`]]`, or `))` for a footnote): the first closer after `min` (0 or 1)
    /// characters, on the same line, within `max` characters. Same answer, one pass for a whole walk.
    pub fn lazy_far(&mut self, m: &str, from: usize, limit: usize, min: usize, max: usize, foot: bool) -> Option<(usize, usize)> {
        let closer: &[u8] = if foot { b"))" } else { b"]]" };
        let b = m.as_bytes();
        let mut start = from;
        if min == 1 {
            let c = *b.get(from)?;
            if from >= limit || c == b'\r' || c == b'\n' {
                return None;
            }
            start = from + utf8_len(c);
        }
        let seek = if foot { &mut self.foot_close } else { &mut self.link_close };
        let q = seek.find(b, start, |b, i| b[i..].starts_with(closer))?;
        if q + closer.len() > limit {
            return None;
        }
        if self.line_end.find(b, from, is_line_end).is_some_and(|n| n < q) {
            return None;
        }
        self.within(b, from, q, max).then_some((q, q + closer.len()))
    }

    /// The `]` that ends an external link label starting at `es`: on the same line, before `limit`, within 2000
    /// characters (`[^\]\r\n]{0,2000}\]`).
    pub fn label_end(&mut self, m: &str, es: usize, limit: usize) -> Option<usize> {
        let b = m.as_bytes();
        let q = self.bracket.find(b, es, |b, i| b[i] == b']')?;
        if q >= limit || self.line_end.find(b, es, is_line_end).is_some_and(|n| n < q) {
            return None;
        }
        self.within(b, es, q, 2000).then_some(q)
    }

    fn within(&mut self, b: &[u8], from: usize, to: usize, max: usize) -> bool {
        let bytes = to - from;
        if bytes <= max {
            return true;
        }
        if bytes > 4 * max {
            return false;
        }
        self.chars.between(b, from, to) <= max
    }
}

pub fn utf8_len(first: u8) -> usize {
    match first {
        0..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Advances over at most `max` characters that satisfy `ok`, from `i` (bounded by `limit`); returns the end.
pub fn run(m: &str, i: usize, limit: usize, max: usize, ok: impl Fn(char) -> bool) -> usize {
    let mut j = i;
    let mut n = 0;
    while j < limit && n < max {
        let Some(c) = char_at(m, j) else { break };
        if !ok(c) {
            break;
        }
        j += c.len_utf8();
        n += 1;
    }
    j
}

/// Number of characters in `m[a..b]` (for caps expressed in characters).
pub fn char_count(m: &str, a: usize, b: usize) -> usize {
    sl(m, a, b).chars().count()
}
