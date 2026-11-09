//! The Markdown line rules of LocOS (`markdown_filter.py`, walk 3) that the MDX walk is built on: fences, the
//! markup prefix, skip lines, link reference definitions, footnote labels, HTML comment blocks.

use crate::pyx::{is_space, trim};
use regex::Regex;
use std::sync::OnceLock;

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).expect("static pattern"))
}

/// `[\s]` as Python reads `\s`.
const S: &str = r"[\s\x1c-\x1f]";

/// `^\s*(```|~~~)`.
pub(crate) fn is_fence(body: &str) -> bool {
    let r = body.trim_start_matches(is_space);
    r.starts_with("```") || r.starts_with("~~~")
}

fn prefix_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, &format!(r"^({S}*(?:#{{1,6}}{S}+|[-*+]{S}+|\d+[.)]{S}+|>{S}+)*)"))
}

/// Length of the leading markup prefix (`_PREFIX`): heading marks, bullets, numbers, quote marks.
pub(crate) fn prefix_len(body: &str) -> usize {
    prefix_re().captures(body).and_then(|c| c.get(1)).map_or(0, |m| m.end())
}

/// True when the whole text is a markup prefix (`_PREFIX.fullmatch`).
pub(crate) fn prefix_full(lead: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, &format!(r"^{S}*(?:#{{1,6}}{S}+|[-*+]{S}+|\d+[.)]{S}+|>{S}+)*$")).is_match(lead)
}

/// A horizontal rule, a table separator row, or a setext underline.
pub(crate) fn is_skip_line(line: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^(?:[-*_=]{3,}|[\s\x1c-\x1f:|.-]+)$").is_match(trim(line))
}

/// `![alt](target)` and `![alt][ref]`: (match start, alt, target) with offsets relative to `s`.
pub(crate) fn images(s: &str) -> Vec<(usize, String, String)> {
    static R: OnceLock<Regex> = OnceLock::new();
    let r = re(&R, &format!(r"!\[([^\[\]]*)\]{S}*[(\[]([^)\]]*)[)\]]"));
    r.captures_iter(s).map(|c| (c.get(0).unwrap().start(), c[1].to_string(), c[2].to_string())).collect()
}

/// A GFM footnote label at `pos` (`\[\^[^\]\s]+\]:[ \t]*`): the offset after it.
pub(crate) fn footnote_def(body: &str, pos: usize) -> Option<usize> {
    let r = body.get(pos..)?.strip_prefix("[^")?;
    for (n, (k, c)) in r.char_indices().enumerate() {
        if c == ']' {
            if n == 0 {
                return None;
            }
            let after = &r[k + 1..];
            let after = after.strip_prefix(':')?;
            let tail = after.trim_start_matches([' ', '\t']);
            return Some(body.len() - tail.len());
        }
        if is_space(c) {
            return None;
        }
    }
    None
}

/// `FOOTNOTE_DEF + "$"` removed from the end of `lead`.
pub(crate) fn strip_footnote_lead(lead: &str) -> String {
    let mut best = lead.len();
    // a footnote label that ends exactly at the end of `lead`
    for (i, _) in lead.match_indices("[^") {
        if footnote_def(lead, i) == Some(lead.len()) {
            best = i;
            break;
        }
    }
    lead[..best].to_string()
}

/// (column, byte offset) of the first character after the leading blanks; tabs stop every 4 columns.
pub(crate) fn indent(line: &str) -> (usize, usize) {
    let mut i = if line.starts_with('\u{feff}') { 3 } else { 0 };
    let mut col = 0;
    let b = line.as_bytes();
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        col = if b[i] == b'\t' { col + 4 - col % 4 } else { col + 1 };
        i += 1;
    }
    (col, i)
}

#[derive(Debug)]
pub(crate) struct RefDef {
    pub end: usize,
    /// (line, start, end, delimiter): the title's inner span on one line.
    pub title: Option<(usize, usize, usize, char)>,
}

fn ws_once(s: &[u8], mut k: usize) -> usize {
    let n = s.len();
    while k < n && (s[k] == b' ' || s[k] == b'\t') {
        k += 1;
    }
    if k < n && s[k] == b'\n' {
        k += 1;
        while k < n && (s[k] == b' ' || s[k] == b'\t') {
            k += 1;
        }
    }
    k
}

fn title_end(s: &[u8], k: usize) -> Option<usize> {
    let close = match s.get(k)? {
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };
    let mut e = k + 1;
    while e < s.len() {
        let c = s[e];
        if c == b'\\' {
            e += 2;
            continue;
        }
        if c == close {
            return Some(e + 1);
        }
        if close == b')' && c == b'(' {
            return None;
        }
        e += 1;
    }
    None
}

const REFDEF_LINES: usize = 3;

/// The link reference definition that starts at line `i` (CommonMark 4.7).
pub(crate) fn link_ref_def(lines: &[&str], i: usize) -> Option<RefDef> {
    let (_c, p) = indent(lines[i]);
    if lines[i].as_bytes().get(p) != Some(&b'[') {
        return None;
    }
    let mut chunk: Vec<&str> = Vec::new();
    for (j, line) in lines.iter().enumerate().skip(i).take(REFDEF_LINES) {
        if j > i && trim(line).is_empty() {
            break;
        }
        chunk.push(*line);
    }
    let s_str = chunk.join("\n");
    let s = s_str.as_bytes();
    let mut starts = vec![0usize];
    for ln in &chunk[..chunk.len() - 1] {
        starts.push(starts.last().unwrap() + ln.len() + 1);
    }
    let n = s.len();
    let mut k = p + 1;
    while k < n {
        match s[k] {
            b'\\' => {
                k += 2;
                continue;
            }
            b'[' => return None,
            b']' => break,
            _ => k += 1,
        }
    }
    if k >= n {
        return None;
    }
    let label = s_str.get(p + 1..k)?;
    if trim(label).is_empty() || label.chars().count() > 999 || label.starts_with('^') {
        return None;
    }
    if s.get(k + 1) != Some(&b':') {
        return None;
    }
    k = ws_once(s, k + 2);
    if k >= n || s[k] == b'\n' {
        return None;
    }
    let e = if s[k] == b'<' {
        let mut x = k + 1;
        while x < n && !matches!(s[x], b'<' | b'>' | b'\n') {
            x += if s[x] == b'\\' { 2 } else { 1 };
        }
        if x >= n || s[x] != b'>' {
            return None;
        }
        x + 1
    } else {
        let (mut x, mut depth) = (k, 0usize);
        while x < n {
            let c = s[x];
            if c == b'\\' && x + 1 < n && !matches!(s[x + 1], b' ' | b'\t' | b'\n') {
                x += 2;
                continue;
            }
            if matches!(c, b' ' | b'\t' | b'\n') || c < 0x20 || c == 0x7f {
                break;
            }
            if c == b'(' {
                depth += 1;
            } else if c == b')' {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            x += 1;
        }
        if x == k || depth > 0 {
            return None;
        }
        x
    };
    let line_of = |off: usize| starts.partition_point(|&st| st <= off) - 1;
    let done = |t: Option<(usize, usize)>| -> RefDef {
        let t1 = t.map_or(0, |t| t.1);
        let last = line_of(e.max(t1).saturating_sub(1));
        let mut title = None;
        if let Some((t0, t1)) = t {
            if line_of(t0) == line_of(t1 - 1) {
                let li = line_of(t0);
                title = Some((i + li, t0 + 1 - starts[li], t1 - 1 - starts[li], s[t0] as char));
            }
        }
        RefDef { end: i + last, title }
    };
    let eol = s[e.min(n)..].iter().position(|&c| c == b'\n').map_or(n, |q| e + q);
    let after = &s[e..eol];
    let blank = |x: &[u8]| x.iter().all(|&c| c == b' ' || c == b'\t');
    if blank(after) {
        if eol < n {
            let mut t0 = eol + 1;
            while t0 < n && (s[t0] == b' ' || s[t0] == b'\t') {
                t0 += 1;
            }
            if let Some(t1) = title_end(s, t0) {
                let rest_end = s[t1..].iter().position(|&c| c == b'\n').map_or(n, |q| t1 + q);
                if blank(&s[t1..rest_end]) {
                    return Some(done(Some((t0, t1))));
                }
            }
        }
        return Some(done(None));
    }
    if after[0] != b' ' && after[0] != b'\t' {
        return None;
    }
    let lead = after.iter().take_while(|&&c| c == b' ' || c == b'\t').count();
    let t0 = e + lead;
    let t1 = title_end(s, t0)?;
    let rest_end = s[t1..].iter().position(|&c| c == b'\n').map_or(n, |q| t1 + q);
    if !blank(&s[t1..rest_end]) {
        return None;
    }
    Some(done(Some((t0, t1))))
}

/// A link title's text: backslash escapes of ASCII punctuation removed.
pub(crate) fn decode_link_title(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut it = raw.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' && it.peek().is_some_and(|n| n.is_ascii_punctuation()) {
            // the escaped character is kept as it is, so `\\` is one backslash and `\\\"` is `\"`
            if let Some(n) = it.next() {
                out.push(n);
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod title_tests {
    use super::*;

    #[test]
    fn a_title_decodes_each_escape_once() {
        assert_eq!(decode_link_title(r#"Back\\slash and \\\" quote"#), r#"Back\slash and \" quote"#);
        assert_eq!(decode_link_title(r"a \\ b"), r"a \ b");
        assert_eq!(decode_link_title(r"a \s b"), r"a \s b");
        for t in [r#"Back\slash and \" quote"#, r"ends with a backslash \", r#"say "hi" \\"#] {
            for d in ['"', '\'', '('] {
                assert_eq!(decode_link_title(&encode_link_title(t, d)), t, "{t:?} {d}");
            }
        }
    }
}

/// The inside of a link title between `delim`: one line, backslash and the delimiter(s) escaped.
pub(crate) fn encode_link_title(new: &str, delim: char) -> String {
    let new = line_breaks_to_space(new);
    let special: &[char] = if delim == '(' { &['\\', '(', ')'] } else { &['\\', delim] };
    let mut out = String::new();
    for c in new.chars() {
        if special.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `_LINE_BREAKS.sub(" ", s)`.
pub(crate) fn line_breaks_to_space(s: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"[ \t]*(?:\r\n|[\r\n\x0b\x0c\x1c-\x1e\x{85}\x{2028}\x{2029}])[ \t]*").replace_all(s, " ").into_owned()
}

pub(crate) fn has_line_break(s: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"[\r\n\x0b\x0c\x1c-\x1e\x{85}\x{2028}\x{2029}]").is_match(s)
}

/// HTML comment blocks (CommonMark 4.6 type 2) that hold nothing but comments.
pub(crate) struct CommentBlocks<'a> {
    lines: &'a [&'a str],
    next: Option<Vec<Option<usize>>>,
    tail_ok: std::collections::HashMap<usize, bool>,
}

impl<'a> CommentBlocks<'a> {
    pub fn new(lines: &'a [&'a str]) -> Self {
        CommentBlocks { lines, next: None, tail_ok: Default::default() }
    }

    fn next_close(&mut self, j: usize) -> Option<usize> {
        if self.next.is_none() {
            let mut nxt = vec![None; self.lines.len() + 1];
            let mut acc = None;
            for k in (0..self.lines.len()).rev() {
                if self.lines[k].contains("-->") {
                    acc = Some(k);
                }
                nxt[k] = acc;
            }
            self.next = Some(nxt);
        }
        self.next.as_ref().unwrap().get(j).copied().flatten()
    }

    /// Last line of the comment block that starts at line `i`.
    pub fn end(&mut self, i: usize) -> Option<usize> {
        let line = self.lines[i];
        let (col, p) = indent(line);
        if col > 3 || !line[p..].starts_with("<!--") {
            return None;
        }
        if line[p + 2..].contains("-->") {
            return only_comments(line, p).then_some(i);
        }
        let j = self.next_close(i + 1)?;
        let lines = self.lines;
        let ok = *self.tail_ok.entry(j).or_insert_with(|| {
            let tail = lines[j];
            only_comments(tail, tail.find("-->").unwrap_or(0) + 3)
        });
        ok.then_some(j)
    }
}

fn only_comments(s: &str, mut pos: usize) -> bool {
    loop {
        while pos < s.len() && matches!(s.as_bytes()[pos], b' ' | b'\t' | b'\n') {
            pos += 1;
        }
        if pos == s.len() {
            return true;
        }
        if !s[pos..].starts_with("<!--") {
            return false;
        }
        match s[pos + 2..].find("-->") {
            Some(c) => pos = pos + 2 + c + 3,
            None => return false,
        }
    }
}
