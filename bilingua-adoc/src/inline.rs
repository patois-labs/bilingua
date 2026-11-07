//! Inline syntax: the `_INLINE_RE` alternation of the LocOS filter as a scanner.
//!
//! At each position the alternatives are tried in the pattern's order (hard break, attribute reference,
//! passthroughs, monospace, anchor, xref, macro, URL, escape, unconstrained marks, constrained marks, super- and
//! subscript). Every matcher follows the backtracking behaviour of the Python pattern it replaces (lazy and greedy
//! quantifiers, optional groups tried first).

use crate::lex::*;
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::BTreeMap;

const MAX_TOKENS: usize = 1000;
const MAX_DEPTH: usize = 8;

pub struct Tok {
    pub n: usize,
    pub codes: BTreeMap<String, CodeData>,
}

impl Tok {
    pub fn new() -> Tok {
        Tok { n: 0, codes: BTreeMap::new() }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Point(&'static str),
    Macro,
    Xref,
    Pair(&'static str), // uf, cf, ss
}

struct Found {
    kind: Kind,
    end: usize,
    // macro: name (index into NAMES), mtext range. pair: marker position, marker, inner range
    a: usize,
    b: usize,
    c: usize,
    mark: [char; 2],
    mlen: usize,
    name: &'static str,
}

fn point(kind: &'static str, end: usize) -> Found {
    Found { kind: Kind::Point(kind), end, a: 0, b: 0, c: 0, mark: ['\0'; 2], mlen: 0, name: "" }
}

fn at(s: &[char], i: usize) -> Option<char> {
    bilingua_model::work::tick(1);
    s.get(i).copied()
}

fn starts(s: &[char], i: usize, lit: &str) -> bool {
    let mut k = i;
    bilingua_model::work::tick(1);
    for c in lit.chars() {
        if s.get(k) != Some(&c) {
            return false;
        }
        k += 1;
    }
    true
}

fn is_ht(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Lazy `[^\r\n]{min,max}?` followed by `closer`: the end of the match.
fn lazy_close(s: &[char], start: usize, min: usize, max: usize, closer: &[char]) -> Option<usize> {
    let mut k = 0usize;
    loop {
        let p = start + k;
        if k >= min && s.len() >= p + closer.len() && s[p..p + closer.len()] == *closer {
            return Some(p + closer.len());
        }
        if k == max {
            return None;
        }
        match s.get(p) {
            Some(c) if !is_nl(*c) => k += 1,
            _ => return None,
        }
    }
}

fn hb(s: &[char], i: usize) -> Option<usize> {
    if !is_ht(*s.get(i)?) || at(s, i + 1) != Some('+') {
        return None;
    }
    let mut j = i + 2;
    while at(s, j).is_some_and(is_ht) {
        j += 1;
    }
    match at(s, j) {
        Some('\r') => j += if at(s, j + 1) == Some('\n') { 2 } else { 1 },
        Some('\n') => j += 1,
        _ => return None,
    }
    while at(s, j).is_some_and(is_ht) {
        j += 1;
    }
    Some(j)
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn attr(s: &[char], i: usize) -> Option<usize> {
    let mut j = i;
    if s[j] == '\\' {
        j += 1;
    }
    if at(s, j) != Some('{') {
        return None;
    }
    j += 1;
    // name: [A-Za-z0-9_][A-Za-z0-9_-]{0,100}
    if at(s, j).is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
        let mut k = j + 1;
        while at(s, k).is_some_and(name_char) && k - (j + 1) <= 100 {
            k += 1;
        }
        if k - (j + 1) <= 100 && at(s, k) == Some('}') {
            return Some(k + 1);
        }
    }
    for p in ["set:", "counter2:", "counter:"] {
        if starts(s, j, p) {
            let st = j + p.len();
            let mut k = st;
            while at(s, k).is_some_and(|c| c != '}' && !is_nl(c)) && k - st <= 200 {
                k += 1;
            }
            if k > st && k - st <= 200 && at(s, k) == Some('}') {
                return Some(k + 1);
            }
        }
    }
    None
}

fn passm(s: &[char], i: usize) -> Option<usize> {
    if !starts(s, i, "pass:") {
        return None;
    }
    let mut j = i + 5;
    while at(s, j).is_some_and(|c| c.is_ascii_lowercase() || c == ',') {
        j += 1;
    }
    if at(s, j) != Some('[') {
        return None;
    }
    let st = j + 1;
    let mut k = st;
    while at(s, k).is_some_and(|c| c != ']' && !is_nl(c)) && k - st <= 500 {
        k += 1;
    }
    if k - st <= 500 && at(s, k) == Some(']') {
        Some(k + 1)
    } else {
        None
    }
}

/// `(?<![\w<ch>])<ch>[^<ch>\s](?:[^<ch>\r\n]{0,500}?[^<ch>\s])?<ch>(?![\w<ch>])` for `+` and the backtick.
fn single(s: &[char], i: usize, ch: char) -> Option<usize> {
    if s[i] != ch {
        return None;
    }
    if i > 0 && (is_word(s[i - 1]) || s[i - 1] == ch) {
        return None;
    }
    let x = at(s, i + 1)?;
    if x == ch || is_space(x) {
        return None;
    }
    let ahead = |q: usize| !at(s, q).is_some_and(|c| is_word(c) || c == ch);
    let base = i + 2;
    for k in 0..=500 {
        let p = base + k;
        let Some(c) = at(s, p) else { break };
        if c != ch && !is_space(c) && at(s, p + 1) == Some(ch) && ahead(p + 2) {
            return Some(p + 2);
        }
        if c == ch || is_nl(c) {
            break;
        }
    }
    if at(s, base) == Some(ch) && ahead(base + 1) {
        return Some(base + 1);
    }
    None
}

/// A bracketed run: `open` then 1..=max chars that are not `open`, `close` or a line break, then `close`.
fn run_between(s: &[char], i: usize, open: char, close: char, max: usize) -> Option<usize> {
    if at(s, i) != Some(open) {
        return None;
    }
    let st = i + 1;
    let mut k = st;
    while at(s, k).is_some_and(|c| c != open && c != close && !is_nl(c)) && k - st <= max {
        k += 1;
    }
    if k > st && k - st <= max && at(s, k) == Some(close) {
        Some(k + 1)
    } else {
        None
    }
}

const NAMES: &[&str] = &[
    "link", "xref", "mailto", "footnote", "footnoteref", "image", "kbd", "btn", "menu", "icon", "stem", "latexmath", "asciimath",
    "anchor", "indexterm", "indexterm2", "http", "https", "ftp", "ftps", "irc", "file",
];

fn macro_at(s: &[char], i: usize) -> Option<Found> {
    if i > 0 && s[i - 1] == '\\' {
        return None;
    }
    let name: &'static str = NAMES.iter().copied().find(|n| starts(s, i, n) && at(s, i + n.len()) == Some(':'))?;
    // Asciidoctor reads `link:`, `xref:`, `image:` and the other macros anywhere, even glued to a letter (a
    // translation may put a word right before one); the short common names such as `stem:` or `menu:` still need one.
    if !matches!(name, "link" | "xref" | "mailto" | "image" | "footnote" | "kbd" | "btn") && i > 0 && is_word(s[i - 1]) {
        return None;
    }
    let t0 = i + name.len() + 1;
    let mut t = t0;
    while at(s, t).is_some_and(|c| !is_space(c) && c != '[' && c != ']') && t - t0 <= 500 {
        t += 1;
    }
    if t - t0 > 500 || at(s, t) != Some('[') {
        return None;
    }
    let x0 = t + 1;
    let mut x = x0;
    while at(s, x).is_some_and(|c| c != ']' && !is_nl(c)) && x - x0 <= 500 {
        x += 1;
    }
    if x - x0 > 500 || at(s, x) != Some(']') {
        return None;
    }
    Some(Found { kind: Kind::Macro, end: x + 1, a: x0, b: x, c: 0, mark: ['\0'; 2], mlen: 0, name })
}

fn url(s: &[char], i: usize) -> Option<usize> {
    if i > 0 && (is_word(s[i - 1]) || matches!(s[i - 1], '/' | '\\' | '<')) {
        return None;
    }
    let sch = ["https://", "http://", "ftps://", "ftp://", "irc://"].iter().find(|p| starts(s, i, p))?;
    let st = i + sch.len();
    let mut k = st;
    while at(s, k).is_some_and(|c| !is_space(c) && !matches!(c, '[' | ']' | '<' | '>' | '"')) {
        k += 1;
    }
    // the last character of the run that is not trailing punctuation ends the URL
    (st..k).rev().find(|&j| !matches!(s[j], '.' | ',' | ';' | ':' | '!' | '?' | ')' | '\'')).map(|j| j + 1)
}

fn esc(s: &[char], i: usize) -> Option<usize> {
    if s[i] == '\\' && at(s, i + 1).is_some_and(|c| "|!*_`#^~{[<+".contains(c)) {
        Some(i + 2)
    } else {
        None
    }
}

/// The optional `[role]` before a mark: end of the role, or `i` when there is none (`None` = a `[` that is no role).
fn role_end(s: &[char], i: usize) -> Option<usize> {
    if s[i] == '[' {
        run_between(s, i, '[', ']', 100)
    } else {
        Some(i)
    }
}

fn uf(s: &[char], i: usize) -> Option<Found> {
    let m = role_end(s, i)?;
    let c = at(s, m)?;
    if !matches!(c, '*' | '_' | '#') || at(s, m + 1) != Some(c) {
        return None;
    }
    let st = m + 2;
    let end = lazy_close(s, st, 1, 500, &[c, c])?;
    Some(Found { kind: Kind::Pair("uf"), end, a: m, b: st, c: end - 2, mark: [c, c], mlen: 2, name: "" })
}

fn cf(s: &[char], i: usize) -> Option<Found> {
    if i > 0 && (is_word(s[i - 1]) || matches!(s[i - 1], '*' | '_' | '#' | '\\')) {
        return None;
    }
    let m = role_end(s, i)?;
    let c = at(s, m)?;
    if !matches!(c, '*' | '_' | '#') {
        return None;
    }
    let x = at(s, m + 1)?;
    if x == c || is_space(x) {
        return None;
    }
    let ahead = |q: usize| !at(s, q).is_some_and(|d| is_word(d) || matches!(d, '*' | '_' | '#'));
    let base = m + 2;
    let found = |close: usize| Found {
        kind: Kind::Pair("cf"),
        end: close + 1,
        a: m,
        b: m + 1,
        c: close,
        mark: [c, c],
        mlen: 1,
        name: "",
    };
    for k in 0..=500 {
        let p = base + k;
        let Some(d) = at(s, p) else { break };
        if !is_space(d) && at(s, p + 1) == Some(c) && ahead(p + 2) {
            return Some(found(p + 1));
        }
        if is_nl(d) {
            break;
        }
    }
    if at(s, base) == Some(c) && ahead(base + 1) {
        return Some(found(base));
    }
    None
}

fn ss(s: &[char], i: usize) -> Option<Found> {
    let c = s[i];
    if c != '^' && c != '~' {
        return None;
    }
    let st = i + 1;
    let mut k = st;
    while at(s, k).is_some_and(|d| !is_space(d) && d != '^' && d != '~') && k - st <= 200 {
        k += 1;
    }
    if k > st && k - st <= 200 && at(s, k) == Some(c) {
        Some(Found { kind: Kind::Pair("ss"), end: k + 1, a: i, b: st, c: k, mark: [c, c], mlen: 1, name: "" })
    } else {
        None
    }
}

fn match_at(s: &[char], i: usize) -> Option<Found> {
    let c = s[i];
    if is_ht(c) {
        return hb(s, i).map(|e| point("break", e));
    }
    if c == '\\' || c == '{' {
        if let Some(e) = attr(s, i) {
            return Some(point("attribute", e));
        }
    }
    if c == '+' {
        if starts(s, i, "+++") {
            if let Some(e) = lazy_close(s, i + 3, 0, 500, &['+', '+', '+']) {
                return Some(point("passthrough", e));
            }
        }
        if starts(s, i, "++") {
            if let Some(e) = lazy_close(s, i + 2, 1, 500, &['+', '+']) {
                return Some(point("passthrough", e));
            }
        }
        if let Some(e) = single(s, i, '+') {
            return Some(point("passthrough", e));
        }
    }
    if c == 'p' {
        if let Some(e) = passm(s, i) {
            return Some(point("passthrough", e));
        }
    }
    if c == '`' {
        if starts(s, i, "``") {
            if let Some(e) = lazy_close(s, i + 2, 1, 500, &['`', '`']) {
                return Some(point("code", e));
            }
        }
        if let Some(e) = single(s, i, '`') {
            return Some(point("code", e));
        }
    }
    if c == '[' && starts(s, i, "[[") {
        // an anchor: two brackets, 1 to 200 characters that are no bracket or line break, two closing brackets
        let st = i + 2;
        let mut k = st;
        while at(s, k).is_some_and(|d| d != '[' && d != ']' && !is_nl(d)) && k - st <= 200 {
            k += 1;
        }
        if k > st && k - st <= 200 && starts(s, k, "]]") {
            return Some(point("anchor", k + 2));
        }
    }
    if c == '<' && starts(s, i, "<<") {
        let st = i + 2;
        let mut k = st;
        while at(s, k).is_some_and(|d| d != '<' && d != '>' && !is_nl(d)) && k - st <= 300 {
            k += 1;
        }
        if k > st && k - st <= 300 && starts(s, k, ">>") {
            return Some(Found { kind: Kind::Xref, end: k + 2, a: st, b: k, c: 0, mark: ['\0'; 2], mlen: 0, name: "" });
        }
    }
    if c.is_ascii_lowercase() {
        if let Some(f) = macro_at(s, i) {
            return Some(f);
        }
        if let Some(e) = url(s, i) {
            return Some(point("url", e));
        }
    }
    if c == '\\' {
        if let Some(e) = esc(s, i) {
            return Some(point("escape", e));
        }
    }
    if matches!(c, '[' | '*' | '_' | '#') {
        if let Some(f) = uf(s, i) {
            return Some(f);
        }
        if let Some(f) = cf(s, i) {
            return Some(f);
        }
    }
    if c == '^' || c == '~' {
        return ss(s, i);
    }
    None
}

fn string(s: &[char]) -> String {
    s.iter().collect()
}

fn push_text(out: &mut Vec<Inline>, s: &[char]) {
    if s.is_empty() {
        return;
    }
    let p = plain(&string(s));
    if !p.is_empty() {
        out.push(Inline::Text(p));
    }
}

impl Tok {
    fn point(&mut self, raw: String, label: &str, out: &mut Vec<Inline>) {
        self.n += 1;
        let id = self.n.to_string();
        self.codes.insert(id.clone(), CodeData::new().with_standalone(raw.clone()).with_label(label.to_string()));
        out.push(Inline::tag(id, TagKind::Standalone, raw));
    }

    fn pair(&mut self, open: String, inner: &[char], close: String, label: &str, depth: usize, out: &mut Vec<Inline>) {
        self.n += 1;
        let id = self.n.to_string();
        self.codes
            .insert(id.clone(), CodeData::new().with_open(open.clone()).with_close(close.clone()).with_label(label.to_string()));
        out.push(Inline::tag(id.clone(), TagKind::Open, open));
        tokenize(inner, self, depth + 1, out);
        out.push(Inline::tag(id, TagKind::Close, close));
    }
}

fn mark_label(c: char) -> &'static str {
    match c {
        '*' => "bold",
        '_' => "italic",
        '#' => "mark",
        '^' => "superscript",
        _ => "subscript",
    }
}

/// `_tokenize`: appends the text and tags of `s` to `out`.
pub fn tokenize(s: &[char], tok: &mut Tok, depth: usize, out: &mut Vec<Inline>) {
    let mut pos = 0usize;
    let mut i = 0usize;
    while i < s.len() {
        let Some(f) = match_at(s, i) else {
            i += 1;
            continue;
        };
        if tok.n >= MAX_TOKENS {
            break;
        }
        push_text(out, &s[pos..i]);
        pos = f.end;
        let start = i;
        i = f.end;
        let raw = || string(&s[start..f.end]);
        if depth >= MAX_DEPTH {
            tok.point(raw(), "placeholder", out);
            continue;
        }
        match f.kind {
            Kind::Point(label) => tok.point(raw(), label, out),
            Kind::Macro => {
                let text = &s[f.a..f.b];
                let name = f.name;
                let paired = matches!(name, "link" | "xref" | "mailto" | "footnote" | "http" | "https" | "ftp" | "ftps" | "irc" | "file")
                    && text.iter().any(|c| is_letter(*c))
                    && !text.contains(&'=')
                    && !text.contains(&'"');
                if paired {
                    let mut core_len = text.len();
                    while core_len > 0 && text[core_len - 1] == '^' {
                        core_len -= 1;
                    }
                    let core = &text[..core_len];
                    let lead = core.iter().take_while(|c| is_space(**c)).count();
                    let mut head = string(&s[start..f.a]);
                    head.push_str(&string(&core[..lead]));
                    let mut close = string(&text[core_len..]);
                    close.push(']');
                    let label = if name == "footnote" { "footnote" } else if name == "xref" { "xref" } else { "link" };
                    tok.pair(head, &core[lead..], close, label, depth, out);
                } else {
                    let label = match name {
                        "image" | "icon" => "image",
                        "kbd" | "btn" | "menu" => "ui",
                        "stem" | "latexmath" | "asciimath" => "math",
                        _ => "macro",
                    };
                    tok.point(raw(), label, out);
                }
            }
            Kind::Xref => {
                let body = &s[f.a..f.b];
                match body.iter().position(|c| *c == ',') {
                    Some(comma) if body[comma + 1..].iter().any(|c| is_letter(*c)) => {
                        let label = &body[comma + 1..];
                        let lead = label.iter().take_while(|c| is_space(**c)).count();
                        let mut head = String::from("<<");
                        head.push_str(&string(&body[..=comma]));
                        head.push_str(&string(&label[..lead]));
                        tok.pair(head, &label[lead..], ">>".into(), "xref", depth, out);
                    }
                    _ => tok.point(raw(), "xref", out),
                }
            }
            Kind::Pair(which) => {
                let mark: String = f.mark[..f.mlen].iter().collect();
                let (open, inner): (String, &[char]) = match which {
                    "ss" => (mark.clone(), &s[f.b..f.c]),
                    "uf" => (string(&s[start..f.b]), &s[f.b..f.c]),
                    _ => (string(&s[start..f.b]), &s[f.b..f.c]),
                };
                let label = mark_label(f.mark[0]);
                tok.pair(open, inner, mark, label, depth, out);
            }
        }
    }
    push_text(out, &s[pos..]);
}

/// The unit of one slot: its tags and text, trimmed like Python's `str.strip`.
pub fn unit_of(raw: &str) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let chars: Vec<char> = raw.chars().collect();
    let mut tok = Tok::new();
    let mut out = Vec::new();
    tokenize(&chars, &mut tok, 0, &mut out);
    if let Some(Inline::Text(t)) = out.first_mut() {
        *t = t.trim_start_matches(is_space).to_string();
    }
    if let Some(Inline::Text(t)) = out.last_mut() {
        *t = t.trim_end_matches(is_space).to_string();
    }
    (bilingua_model::merge_text(out), tok.codes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bilingua_model::profile::tokens::to_locos_text;

    fn t(raw: &str) -> String {
        to_locos_text(&unit_of(raw).0).0
    }

    #[test]
    fn marks_and_points() {
        assert_eq!(t("a *b* c"), "a \u{27E6}g1\u{27E7}b\u{27E6}/g1\u{27E7} c");
        assert_eq!(t("use `x` now"), "use \u{27E6}x1\u{27E7} now");
        assert_eq!(t("Use %s or {0} here."), "Use %s or \u{27E6}x1\u{27E7} here.");
        assert_eq!(t("a +\nb"), "a\u{27E6}x1\u{27E7}b");
        assert_eq!(t("a\nb"), "a b");
    }
}
