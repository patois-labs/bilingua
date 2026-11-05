//! Inline markup: the scanner that finds roles, literals, references and emphasis (LocOS `_INLINE_RE`, written by
//! hand because the pattern uses look-behind), and the writer that puts a translated unit back as reStructuredText
//! (`_render`, with the #695 boundary guard).

use crate::pyu::*;
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const MAX_TOKENS: usize = 1000;

/// What a token stands for in the source.
#[derive(Debug, Clone)]
pub enum Code {
    Point(String),
    Pair(String, String),
}

#[derive(Debug, Clone)]
pub enum Piece {
    Text(String),
    Open(usize),
    Close(usize),
    Point(usize),
}

#[derive(Default)]
pub struct Tok {
    pub n: usize,
    pub codes: Vec<Code>,
    pub pieces: Vec<Piece>,
}

#[derive(Clone, Copy)]
enum Kind {
    Esc,
    Strong { a: usize, b: usize },
    Em { a: usize, b: usize },
    Eref { text_end: usize },
    Other,
}

struct M {
    start: usize,
    end: usize,
    kind: Kind,
}

fn at(s: &str, i: usize) -> Option<char> {
    s.get(i..).and_then(|r| r.chars().next())
}

fn not_word_at(s: &str, i: usize) -> bool {
    at(s, i).is_none_or(|c| !is_word(c))
}

fn prev_not(s: &str, i: usize, bad: impl Fn(char) -> bool) -> bool {
    prev_char(s, i).is_none_or(|c| !bad(c))
}

/// `(?:__?)` followed by `(?!\w)`, tried greedy first; `optional` allows the empty choice. Returns the end.
fn underscores(s: &str, p: usize, optional: bool) -> Option<usize> {
    if at(s, p) == Some('_') {
        if at(s, p + 1) == Some('_') && not_word_at(s, p + 2) {
            return Some(p + 2);
        }
        if not_word_at(s, p + 1) {
            return Some(p + 1);
        }
    }
    if optional && not_word_at(s, p) {
        return Some(p);
    }
    None
}

fn role_class(c: char) -> bool {
    is_word(c) || matches!(c, '.' | '+' | ':' | '-')
}

/// Byte positions after 0..=max characters of `s[from..]` that satisfy `pred` (position 0 is `from`).
fn run_ends(s: &str, from: usize, max: usize, pred: impl Fn(char) -> bool) -> Vec<usize> {
    let mut v = vec![from];
    let mut p = from;
    for c in s[from..].chars() {
        if v.len() > max || !pred(c) {
            break;
        }
        p += c.len_utf8();
        v.push(p);
    }
    v
}

/// Body of `[^`]{1,max}?` closed by the first backtick at or after `q`: the position of that backtick.
fn tick_body(s: &str, q: usize, max: usize) -> Option<usize> {
    let off = s[q..].find('`')?;
    if off == 0 || s[q..q + off].chars().take(max + 1).count() > max {
        return None;
    }
    Some(q + off)
}

fn m_lit(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '\\') || !s[i..].starts_with("``") {
        return None;
    }
    let a = i + 2;
    if at(s, a).is_none_or(is_space) {
        return None;
    }
    let mut j = a;
    for (k, ch) in s[a..].chars().enumerate() {
        j += ch.len_utf8();
        if k >= 2000 {
            return None;
        }
        if is_space(ch) {
            continue;
        }
        if s[j..].starts_with("``") && not_word_at(s, j + 2) {
            return Some(M { start: 0, end: j + 2, kind: Kind::Other });
        }
    }
    None
}

fn m_role(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '\\' || c == '`') || !at(s, i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let ends = run_ends(s, i + 2, 100, role_class);
    for &p in ends.iter().rev() {
        if s[p..].starts_with(":`") {
            if let Some(close) = tick_body(s, p + 2, 2000) {
                if not_word_at(s, close + 1) {
                    return Some(M { start: 0, end: close + 1, kind: Kind::Other });
                }
            }
        }
    }
    None
}

fn m_eref(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '\\' || c == '`') {
        return None;
    }
    let from = i + 1;
    let q = from + s[from..].find(['`', '<'])?;
    if at(s, q) != Some('<') {
        return None;
    }
    let mut ws = q;
    while ws > from && prev_char(s, ws).is_some_and(is_space) {
        ws -= prev_char(s, ws).map_or(1, char::len_utf8);
    }
    if s[from..ws].chars().take(2001).count() > 2000 {
        return None;
    }
    let mut p = q + 1;
    let mut r = 0usize;
    for c in s[p..].chars() {
        if matches!(c, '`' | '<' | '>') {
            break;
        }
        r += 1;
        p += c.len_utf8();
        if r > 2000 {
            return None;
        }
    }
    if r == 0 || at(s, p) != Some('>') || at(s, p + 1) != Some('`') || at(s, p + 2) != Some('_') {
        return None;
    }
    let end = underscores(s, p + 2, false)?;
    Some(M { start: 0, end, kind: Kind::Eref { text_end: ws } })
}

fn m_ref(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '\\' || c == '`') {
        return None;
    }
    let close = tick_body(s, i + 1, 2000)?;
    let c = close + 1;
    if at(s, c) == Some(':') && at(s, c + 1).is_some_and(|x| x.is_ascii_alphabetic()) {
        let ends = run_ends(s, c + 2, 100, role_class);
        for &p in ends.iter().rev() {
            if at(s, p) == Some(':') && not_word_at(s, p + 1) {
                return Some(M { start: 0, end: p + 1, kind: Kind::Other });
            }
        }
    }
    let end = underscores(s, c, true)?;
    Some(M { start: 0, end, kind: Kind::Other })
}

fn m_subst(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '\\' || c == '|') || at(s, i + 1) == Some(' ') {
        return None;
    }
    let mut p = i + 1;
    let mut k = 0usize;
    let mut last = None;
    for c in s[p..].chars() {
        if c == '|' {
            break;
        }
        if c == '\r' || c == '\n' {
            return None;
        }
        k += 1;
        if k > 200 {
            return None;
        }
        p += c.len_utf8();
        last = Some(c);
    }
    if k == 0 || at(s, p) != Some('|') || last == Some(' ') {
        return None;
    }
    let end = underscores(s, p + 1, true)?;
    Some(M { start: 0, end, kind: Kind::Other })
}

fn fn_class(c: char) -> bool {
    is_word(c) || c == '.' || c == '-'
}

fn m_fnref(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '\\') {
        return None;
    }
    let mut p = i + 1;
    let first = at(s, p)?;
    let (mut all_digit, mut all_class) = (true, true);
    let mut n = 0usize;
    let hash = first == '#';
    let star = first == '*';
    loop {
        let c = at(s, p)?;
        if c == ']' {
            break;
        }
        n += 1;
        all_digit &= is_digit(c);
        all_class &= fn_class(c);
        if !(fn_class(c) || (n == 1 && (hash || star))) {
            return None;
        }
        p += c.len_utf8();
    }
    let ok = n > 0 && (all_digit || (hash && s[i + 2..p].chars().all(fn_class)) || (star && n == 1) || all_class);
    if !ok || at(s, p + 1) != Some('_') || !not_word_at(s, p + 2) {
        return None;
    }
    Some(M { start: 0, end: p + 2, kind: Kind::Other })
}

fn m_target(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '\\') || at(s, i + 1) != Some('`') {
        return None;
    }
    let close = tick_body(s, i + 2, 2000)?;
    if !not_word_at(s, close + 1) {
        return None;
    }
    Some(M { start: 0, end: close + 1, kind: Kind::Other })
}

fn m_strong(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '\\' || c == '*') || !s[i..].starts_with("**") {
        return None;
    }
    let a = i + 2;
    let c0 = at(s, a)?;
    if is_space(c0) || c0 == '*' {
        return None;
    }
    let mut j = a;
    for (k, ch) in s[a..].chars().enumerate() {
        j += ch.len_utf8();
        if k >= 2000 {
            return None;
        }
        if is_space(ch) || ch == '\\' {
            continue;
        }
        if s[j..].starts_with("**") && at(s, j + 2).is_none_or(|c| !is_word(c) && c != '*') {
            return Some(M { start: 0, end: j + 2, kind: Kind::Strong { a, b: j } });
        }
    }
    None
}

fn m_em(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '\\' || c == '*') {
        return None;
    }
    let a = i + 1;
    let c0 = at(s, a)?;
    if is_space(c0) || c0 == '*' {
        return None;
    }
    let star = a + s[a..].find('*')?;
    if s[a..star].chars().take(2001).count() > 2000 {
        return None;
    }
    let last = prev_char(s, star)?;
    if is_space(last) || last == '\\' || at(s, star + 1).is_some_and(|c| is_word(c) || c == '*') {
        return None;
    }
    Some(M { start: 0, end: star + 1, kind: Kind::Em { a, b: star } })
}

fn m_url(s: &str, i: usize) -> Option<M> {
    if !prev_not(s, i, |c| is_word(c) || c == '/' || c == '\\' || c == '<') {
        return None;
    }
    let r = &s[i..];
    let p = if r.starts_with("https:") {
        i + 6
    } else if r.starts_with("http:") {
        i + 5
    } else if r.starts_with("ftp:") {
        i + 4
    } else if r.starts_with("mailto:") {
        i + 7
    } else {
        return None;
    };
    let run_len = s[p..].find(|c: char| is_space(c) || matches!(c, '<' | '>' | '`')).unwrap_or(s.len() - p);
    let run = s[p..p + run_len].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '\'', '"']);
    if run.is_empty() {
        return None;
    }
    Some(M { start: 0, end: p + run.len(), kind: Kind::Other })
}

fn m_wref(s: &str, i: usize) -> Option<M> {
    let first = at(s, i)?;
    if !first.is_ascii_alphanumeric() || !prev_not(s, i, |c| is_word(c) || c == '\\' || c == '.' || c == '-') {
        return None;
    }
    let ends = run_ends(s, i + 1, 200, |c| is_word(c) || matches!(c, '.' | '+' | '-'));
    for &p in ends.iter().rev() {
        if at(s, p) == Some('_') && prev_char(s, p).is_some_and(|c| !matches!(c, '.' | '_' | '+' | '-')) {
            if let Some(end) = underscores(s, p, false) {
                return Some(M { start: 0, end, kind: Kind::Other });
            }
        }
    }
    None
}

fn match_at(s: &str, i: usize, c: char) -> Option<M> {
    let mut m = match_raw(s, i, c)?;
    m.start = i;
    Some(m)
}

fn match_raw(s: &str, i: usize, c: char) -> Option<M> {
    match c {
        '\\' => {
            let n = at(s, i + 1)?;
            (n != '\n').then(|| M { start: 0, end: i + 1 + n.len_utf8(), kind: Kind::Esc })
        }
        '`' => m_lit(s, i).or_else(|| m_eref(s, i)).or_else(|| m_ref(s, i)),
        ':' => m_role(s, i),
        '|' => m_subst(s, i),
        '[' => m_fnref(s, i),
        '_' => m_target(s, i),
        '*' => m_strong(s, i).or_else(|| m_em(s, i)),
        'h' | 'f' | 'm' => m_url(s, i).or_else(|| m_wref(s, i)),
        c if c.is_ascii_alphanumeric() => m_wref(s, i),
        _ => None,
    }
}

/// `_SOFT.sub(" ", s)`: a line break with the blanks around it becomes one space.
pub fn plain(s: &str) -> String {
    if !s.contains(['\n', '\r']) {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < b.len() {
        let mut q = i;
        while q < b.len() && (b[q] == b' ' || b[q] == b'\t') {
            q += 1;
        }
        if q < b.len() && (b[q] == b'\n' || b[q] == b'\r') {
            q += if b[q] == b'\r' && b.get(q + 1) == Some(&b'\n') { 2 } else { 1 };
            while q < b.len() && (b[q] == b' ' || b[q] == b'\t') {
                q += 1;
            }
            out.push(' ');
            i = q;
        } else if q > i {
            out.push_str(&s[i..q]);
            i = q;
        } else {
            let c = s[i..].chars().next().unwrap_or(' ');
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

impl Tok {
    fn text(&mut self, s: &str) {
        if !s.is_empty() {
            self.pieces.push(Piece::Text(plain(s)));
        }
    }

    fn point(&mut self, raw: &str) {
        self.n += 1;
        self.codes.push(Code::Point(raw.to_string()));
        self.pieces.push(Piece::Point(self.n));
    }

    fn pair(&mut self, open: &str, inner: &str, close: &str, depth: usize) {
        self.n += 1;
        let n = self.n;
        self.codes.push(Code::Pair(open.to_string(), close.to_string()));
        self.pieces.push(Piece::Open(n));
        self.scan(inner, depth + 1);
        self.pieces.push(Piece::Close(n));
    }

    fn scan(&mut self, s: &str, depth: usize) {
        let mut pos = 0usize;
        let mut i = 0usize;
        while i < s.len() {
            let c = s[i..].chars().next().unwrap_or(' ');
            let Some(m) = match_at(s, i, c) else {
                i += c.len_utf8();
                continue;
            };
            if self.n >= MAX_TOKENS {
                break;
            }
            self.text(&s[pos..i]);
            pos = m.end;
            i = m.end;
            self.emit(s, &m, depth);
        }
        self.text(&s[pos..]);
    }

    fn emit(&mut self, s: &str, m: &M, depth: usize) {
        let start = m.start;
        let raw = &s[start..m.end];
        match m.kind {
            Kind::Strong { a, b } if depth < 4 => self.pair("**", &s[a..b], "**", depth),
            Kind::Em { a, b } if depth < 4 => self.pair("*", &s[a..b], "*", depth),
            Kind::Eref { text_end } if s[start + 1..text_end].chars().any(is_alpha) => {
                self.pair("`", &s[start + 1..text_end], &s[text_end..m.end], 4)
            }
            _ => self.point(raw),
        }
    }
}

/// Tokenises raw source text of one unit. The result is stripped like `str.strip()`.
pub fn tokenize(raw: &str) -> Tok {
    let mut t = Tok::default();
    t.scan(raw, 0);
    t.strip();
    t
}

impl Tok {
    fn strip(&mut self) {
        if let Some(Piece::Text(first)) = self.pieces.first_mut() {
            *first = lstrip(first).to_string();
        }
        if let Some(Piece::Text(last)) = self.pieces.last_mut() {
            *last = rstrip(last).to_string();
        }
        self.pieces.retain(|p| !matches!(p, Piece::Text(t) if t.is_empty()));
        // adjacent text pieces join
        let mut out: Vec<Piece> = Vec::with_capacity(self.pieces.len());
        for p in self.pieces.drain(..) {
            match (out.last_mut(), p) {
                (Some(Piece::Text(a)), Piece::Text(b)) => a.push_str(&b),
                (_, p) => out.push(p),
            }
        }
        self.pieces = out;
    }

    /// True when the unit has a letter outside its tokens (`_translatable`).
    pub fn translatable(&self) -> bool {
        self.pieces.iter().any(|p| matches!(p, Piece::Text(t) if t.chars().any(is_alpha)))
    }

    /// The bilingua inline codes and the code table for the unit.
    pub fn inlines(&self, label_for: impl Fn(&Code, &str) -> &'static str) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
        let mut v = Vec::with_capacity(self.pieces.len());
        let mut codes = BTreeMap::new();
        for (idx, c) in self.codes.iter().enumerate() {
            let id = (idx + 1).to_string();
            let cd = match c {
                Code::Point(raw) => CodeData { standalone: Some(raw.clone()), label: Some(label_for(c, raw).into()), ..Default::default() },
                Code::Pair(o, cl) => CodeData { open: Some(o.clone()), close: Some(cl.clone()), label: Some(label_for(c, o).into()), ..Default::default() },
            };
            codes.insert(id, cd);
        }
        for p in &self.pieces {
            match p {
                Piece::Text(t) => v.push(Inline::Text(t.clone())),
                Piece::Open(n) | Piece::Close(n) | Piece::Point(n) => {
                    let id = n.to_string();
                    let cd = &codes[&id];
                    let (kind, display) = match p {
                        Piece::Open(_) => (TagKind::Open, cd.open.clone().unwrap_or_default()),
                        Piece::Close(_) => (TagKind::Close, cd.close.clone().unwrap_or_default()),
                        _ => (TagKind::Standalone, cd.standalone.clone().unwrap_or_default()),
                    };
                    v.push(Inline::tag(id, kind, display));
                }
            }
        }
        (v, codes)
    }
}

pub fn label_for(c: &Code, first: &str) -> &'static str {
    match c {
        Code::Pair(o, _) => match o.as_str() {
            "**" => "strong",
            "*" => "emphasis",
            _ => "link",
        },
        Code::Point(_) => {
            if first.starts_with("``") {
                "literal"
            } else if first.starts_with(':') {
                "role"
            } else if first.starts_with('\\') {
                "escape"
            } else if first.starts_with('|') {
                "substitution"
            } else if first.starts_with('[') {
                "footnote-ref"
            } else if first.starts_with("http") || first.starts_with("ftp:") || first.starts_with("mailto:") {
                "url"
            } else {
                "reference"
            }
        }
    }
}

// ----------------------------------------------------------------------------------------------- writing

fn escape_text(chunk: &str, out: &mut String) {
    let mut prev: Option<char> = None;
    let mut it = chunk.chars().peekable();
    while let Some(c) = it.next() {
        let esc = match c {
            '*' | '`' | '|' | '\\' => true,
            '_' => prev.is_some_and(is_word) && it.peek().is_none_or(|&n| !is_word(n)),
            _ => false,
        };
        if esc {
            out.push('\\');
        }
        out.push(c);
        prev = Some(c);
    }
}

/// Splits a text piece at token-shaped substrings (`⟦/?[a-z]?\d+⟧`), which LocOS consumes whether or not the
/// number is known; the parts between them are separate escaping contexts.
fn split_tokens(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut last = 0usize;
    let mut i = 0usize;
    while let Some(off) = s[i..].find('\u{27E6}') {
        let st = i + off;
        let r = &s[st + 3..];
        let mut j = 0usize;
        let b = r.as_bytes();
        if b.first() == Some(&b'/') {
            j += 1;
        }
        if b.get(j).is_some_and(u8::is_ascii_lowercase) {
            j += 1;
        }
        let d0 = j;
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j > d0 && r[j..].starts_with('\u{27E7}') {
            parts.push(&s[last..st]);
            last = st + 3 + j + 3;
            i = last;
        } else {
            i = st + 3;
        }
    }
    parts.push(&s[last..]);
    parts
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            in_ws = false;
            out.push(c);
        }
    }
    out
}

fn before_ok(c: Option<char>) -> bool {
    c.is_none_or(|c| is_space(c) || "-:/'\"<([{".contains(c) || c == '\0' || punct_in(c, 1 | 4 | 8 | 16 | 32))
}

fn after_ok(c: Option<char>) -> bool {
    c.is_none_or(|c| is_space(c) || "-.,:;!?\\/'\")]}>".contains(c) || c == '\0' || punct_in(c, 2 | 4 | 8 | 16 | 32))
}

fn guard_boundaries(mut out: Vec<String>, spans: &[(usize, usize)]) -> String {
    if spans.is_empty() {
        return out.concat();
    }
    let mut ends = Vec::with_capacity(out.len());
    let mut n = 0usize;
    for p in &out {
        n += p.len();
        ends.push(n);
    }
    let joined = out.concat();
    let (mut b_esc, mut a_esc) = (BTreeSet::new(), BTreeSet::new());
    for &(a, b) in spans {
        let (start, end) = (ends[a] - out[a].len(), ends[b]);
        if start == end {
            continue;
        }
        if !before_ok(prev_char(&joined, start)) {
            b_esc.insert(a);
        }
        if !after_ok(next_char(&joined, end)) {
            a_esc.insert(b);
        }
    }
    for i in b_esc {
        out[i].insert_str(0, "\\ ");
    }
    for i in a_esc {
        out[i].push_str("\\ ");
    }
    out.concat()
}

/// `_render`: a translated unit back as one line of reStructuredText.
pub fn render(translation: &[Inline], codes: &BTreeMap<String, CodeData>) -> String {
    let mut items: Vec<(Option<String>, Option<&Inline>)> = Vec::new();
    let last_idx = translation.len().saturating_sub(1);
    for (k, it) in translation.iter().enumerate() {
        match it {
            Inline::Text(t) => {
                let mut c = collapse_ws(t);
                if k == 0 {
                    c = c.trim_start_matches(' ').to_string();
                }
                if k == last_idx {
                    c = c.trim_end_matches(' ').to_string();
                }
                items.push((Some(c), None));
            }
            tag => items.push((None, Some(tag))),
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut opened: Vec<String> = Vec::new();
    let mut open_at: HashMap<String, usize> = HashMap::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for (text, tag) in items {
        if let Some(t) = text {
            for part in split_tokens(&t) {
                let mut s = String::with_capacity(part.len());
                escape_text(part, &mut s);
                out.push(s);
            }
            continue;
        }
        let Some(Inline::Tag { id, kind, .. }) = tag else { continue };
        let Some(cd) = codes.get(id) else { continue };
        let closing = *kind == TagKind::Close;
        if cd.standalone.is_none() && cd.open.is_some() {
            if !closing {
                if !opened.contains(id) {
                    opened.push(id.clone());
                    open_at.insert(id.clone(), out.len());
                    out.push(plain(cd.open.as_deref().unwrap_or("")));
                }
            } else if opened.contains(id) {
                while let Some(k) = opened.pop() {
                    spans.push((open_at[&k], out.len()));
                    out.push(plain(codes.get(&k).and_then(|c| c.close.as_deref()).unwrap_or("")));
                    if &k == id {
                        break;
                    }
                }
            }
        } else if !closing {
            let val = cd.standalone.as_deref().unwrap_or("");
            if !is_escape_token(val) {
                spans.push((out.len(), out.len()));
            }
            out.push(plain(val));
        }
    }
    while let Some(k) = opened.pop() {
        spans.push((open_at[&k], out.len()));
        out.push(plain(codes.get(&k).and_then(|c| c.close.as_deref()).unwrap_or("")));
    }
    guard_boundaries(out, &spans)
}

/// `_ESCAPE_TOKEN.fullmatch`: a backslash and one character.
fn is_escape_token(v: &str) -> bool {
    let mut it = v.chars();
    it.next() == Some('\\') && it.next().is_some() && it.next().is_none()
}

/// `_PARA_START.match(rendered)`: text that would read as another block when it starts a paragraph.
pub fn para_start(r: &str) -> bool {
    let mut it = r.chars();
    let Some(c0) = it.next() else { return false };
    let rest = it.as_str();
    let ws_or_end = |t: &str| t.chars().next().is_none_or(is_space);
    if matches!(c0, '-' | '*' | '+' | '\u{2022}' | '\u{2023}' | '\u{2043}') && ws_or_end(rest) {
        return true;
    }
    // enumerator
    {
        let body = r.strip_prefix('(').unwrap_or(r);
        let try_end = |after: &str| -> bool { after.starts_with(['.', ')']) && ws_or_end(&after[1..]) };
        let digits = body.find(|c: char| !is_digit(c)).unwrap_or(body.len());
        if digits > 0 && try_end(&body[digits..]) {
            return true;
        }
        if body.starts_with('#') && try_end(&body[1..]) {
            return true;
        }
        if body.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && try_end(&body[1..]) {
            return true;
        }
        let roman = body.find(|c: char| !"ivxlcdmIVXLCDM".contains(c)).unwrap_or(body.len());
        if roman > 0 && try_end(&body[roman..]) {
            return true;
        }
    }
    if r.starts_with("..") && ws_or_end(&r[2..]) {
        return true;
    }
    if c0 == '|' && ws_or_end(rest) {
        return true;
    }
    if c0 == ':' {
        if let Some(n) = rest.chars().next() {
            if n != ':' && n != ' ' {
                if let Some(off) = rest.find(':') {
                    if off > 0 && ws_or_end(&rest[off + 1..]) {
                        return true;
                    }
                }
            }
        }
    }
    if r.starts_with(">>>") || r.starts_with("__ ") {
        return true;
    }
    let run = r.find(|c: char| !"-=~^\"'`#*+<>".contains(c)).unwrap_or(r.len());
    run >= 4 && r[run..].chars().all(is_space)
}
