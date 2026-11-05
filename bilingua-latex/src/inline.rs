//! Inline tokens: the text of one slot turned into text pieces, text-command pairs and point tokens (LocOS
//! `_tokenize`), and a translation turned back into LaTeX (LocOS `_render`).

use crate::chars::{char_at, py_alpha, py_space};
use crate::scan::{self, args, cs, end_of_args, find_end, is_verbatim_arg, verb_end, Txt, TEXT_INLINE_COMMANDS};
use bilingua_model::{CodeData, Inline, TagKind, Unit};
use regex::Regex;
use std::sync::OnceLock;

const MAX_DEPTH: usize = 40;
const MAX_TOKENS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Open(usize),
    Close(usize),
    Point(usize),
}

/// What the markup of token `n` (1-based, index `n - 1`) is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Pair(String, String),
    Point(String),
}

struct Tok<'a> {
    n: usize,
    map: Vec<Entry>,
    extra: &'a [String],
    out: Vec<Piece>,
}

impl Tok<'_> {
    fn keep(&self, name: &str) -> Option<usize> {
        if self.extra.iter().any(|e| e == name) {
            return Some(0);
        }
        TEXT_INLINE_COMMANDS.iter().find(|(n, _)| *n == name).map(|x| x.1)
    }
}

/// Collapse every run of space, tab, CR and LF to one space (never a no-break space).
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

fn flush(buf: &mut String, out: &mut Vec<Piece>) {
    if !buf.is_empty() {
        out.push(Piece::Text(collapse_ws(buf)));
        buf.clear();
    }
}

fn point(tok: &mut Tok, buf: &mut String, raw: &str) {
    tok.n += 1;
    tok.map.push(Entry::Point(raw.to_string()));
    flush(buf, &mut tok.out);
    tok.out.push(Piece::Point(tok.n));
}

fn pair(tok: &mut Tok, buf: &mut String, open: &str, inner: &str, depth: usize) {
    tok.n += 1;
    let n = tok.n;
    tok.map.push(Entry::Pair(open.to_string(), "}".to_string()));
    flush(buf, &mut tok.out);
    tok.out.push(Piece::Open(n));
    tokenize(inner, tok, depth + 1);
    tok.out.push(Piece::Close(n));
}

fn tokenize(s: &str, tok: &mut Tok, depth: usize) {
    let tx = Txt::new(s);
    let b = s.as_bytes();
    let n = s.len();
    let mut i = 0usize;
    let mut buf = String::new();
    while i < n {
        let c = b[i];
        if tok.n >= MAX_TOKENS {
            point(tok, &mut buf, &s[i..]);
            break;
        }
        match c {
            b'%' => {
                let mut e = find_end(s, "\n", i, n);
                while e < n && matches!(b[e], b' ' | b'\t') {
                    e += 1;
                }
                point(tok, &mut buf, &s[i..e]);
                i = e;
                continue;
            }
            b'~' => {
                buf.push('\u{a0}');
                i += 1;
                continue;
            }
            b'$' => {
                let e = if b.get(i + 1) == Some(&b'$') {
                    s[i + 2..].find("$$").map_or(n, |k| i + 2 + k + 2)
                } else {
                    tx.inline_math_end(i, n).unwrap_or(i + 1)
                };
                point(tok, &mut buf, &s[i..e]);
                i = e;
                continue;
            }
            b'{' => {
                let e = tx.match_brace(i, n, false);
                match e {
                    Some(e) if depth < MAX_DEPTH => {
                        let inner = &s[i + 1..e - 1];
                        pair(tok, &mut buf, "{", inner, depth);
                        i = e;
                    }
                    _ => {
                        let e = e.unwrap_or(i + 1);
                        point(tok, &mut buf, &s[i..e]);
                        i = e;
                    }
                }
                continue;
            }
            b'}' => {
                point(tok, &mut buf, "}");
                i += 1;
                continue;
            }
            b'\\' => {}
            _ => {
                let (ch, l) = char_at(s, i);
                buf.push(ch);
                i += l;
                continue;
            }
        }
        let (name, mut e) = cs(s, i);
        if matches!(name, "&" | "%" | "$" | "#" | "_" | "{" | "}") {
            buf.push_str(name);
            i = e;
            continue;
        }
        if name == "(" {
            e = s[e..].find("\\)").map_or(n, |k| e + k + 2);
            point(tok, &mut buf, &s[i..e]);
            i = e;
            continue;
        }
        if name == "verb" || name == "lstinline" {
            if e < n && b[e] == b'*' {
                e += 1;
            }
            if name == "lstinline" && e < n && b[e] == b'[' {
                e = tx.match_bracket(e, n).unwrap_or(e);
            }
            e = verb_end(&tx, e, n);
            point(tok, &mut buf, &s[i..e]);
            i = e;
            continue;
        }
        if name == "\\" {
            if e < n && b[e] == b'*' {
                e += 1;
            }
            e = end_of_args(&args(&tx, e, n, false, false), e);
            point(tok, &mut buf, &s[i..e]);
            i = e;
            continue;
        }
        let mut star = e;
        if star < n && b[star] == b'*' {
            star += 1;
        }
        let a = args(&tx, star, n, false, is_verbatim_arg(name));
        let keep = tok.keep(name);
        let braces: Vec<&scan::Arg> = a.iter().filter(|x| x.0 == b'{').collect();
        if let Some(k) = keep {
            if depth < MAX_DEPTH && braces.len() > k {
                let target = *braces[k];
                let open = &s[i..target.1 + 1];
                let inner = &s[target.1 + 1..target.2 - 1];
                pair(tok, &mut buf, open, inner, depth);
                i = target.2;
                continue;
            }
        }
        let mut end = end_of_args(&a, star);
        // a control word eats the following blanks in TeX; keep them in the token so the translated text keeps its
        // spacing unchanged
        if !name.is_empty() && name.chars().all(py_alpha) && a.is_empty() {
            let mut k = end;
            while k < n && matches!(b[k], b' ' | b'\t') {
                k += 1;
            }
            if k > end && (k >= n || b[k] != b'\n') {
                end = k;
            }
        }
        point(tok, &mut buf, &s[i..end]);
        i = end;
    }
    flush(&mut buf, &mut tok.out);
}

/// The pieces and token map of one slot; the text is stripped like Python `str.strip`.
pub fn tokenize_slot(raw: &str, extra: &[String]) -> (Vec<Piece>, Vec<Entry>) {
    let mut tok = Tok { n: 0, map: Vec::new(), extra, out: Vec::new() };
    tokenize(raw, &mut tok, 0);
    let mut out = tok.out;
    if let Some(Piece::Text(t)) = out.first_mut() {
        *t = t.trim_start_matches(py_space).to_string();
    }
    if let Some(Piece::Text(t)) = out.last_mut() {
        *t = t.trim_end_matches(py_space).to_string();
    }
    out.retain(|p| !matches!(p, Piece::Text(t) if t.is_empty()));
    (out, tok.map)
}

fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\u{27E6}/?[a-z]?\d+\u{27E7}").expect("placeholder pattern"))
}

/// LocOS `_translatable`: some letter outside the tokens.
pub fn translatable(pieces: &[Piece]) -> bool {
    pieces.iter().any(|p| match p {
        Piece::Text(t) => {
            if t.contains('\u{27E6}') {
                placeholder_re().replace_all(t, "").chars().any(py_alpha)
            } else {
                t.chars().any(py_alpha)
            }
        }
        _ => false,
    })
}

/// The Bilingua inlines and code table of a slot's pieces. Tag ids are the LocOS token numbers.
pub fn to_inlines(pieces: &[Piece], map: &[Entry]) -> (Vec<Inline>, Vec<(String, CodeData)>) {
    let mut inl = Vec::with_capacity(pieces.len());
    let mut codes = Vec::new();
    for p in pieces {
        match p {
            Piece::Text(t) => inl.push(Inline::Text(t.clone())),
            Piece::Open(n) => {
                if let Entry::Pair(o, c) = &map[n - 1] {
                    inl.push(Inline::tag(n.to_string(), TagKind::Open, o.clone()));
                    codes.push((
                        n.to_string(),
                        CodeData { open: Some(o.clone()), close: Some(c.clone()), label: Some(pair_label(o)), ..Default::default() },
                    ));
                }
            }
            Piece::Close(n) => {
                if let Entry::Pair(_, c) = &map[n - 1] {
                    inl.push(Inline::tag(n.to_string(), TagKind::Close, c.clone()));
                }
            }
            Piece::Point(n) => {
                if let Entry::Point(r) = &map[n - 1] {
                    inl.push(Inline::tag(n.to_string(), TagKind::Standalone, r.clone()));
                    codes.push((
                        n.to_string(),
                        CodeData { standalone: Some(r.clone()), label: Some(point_label(r)), ..Default::default() },
                    ));
                }
            }
        }
    }
    (inl, codes)
}

fn pair_label(open: &str) -> String {
    if open == "{" {
        "group".into()
    } else {
        let (name, _) = cs(open, 0);
        name.to_string()
    }
}

fn point_label(raw: &str) -> String {
    match raw.as_bytes().first() {
        Some(b'%') => "comment".into(),
        Some(b'$') => "math".into(),
        Some(b'\\') => {
            let (name, _) = cs(raw, 0);
            if name.is_empty() {
                "command".into()
            } else {
                name.to_string()
            }
        }
        _ => "markup".into(),
    }
}

// ------------------------------------------------------------------------------------------------ rendering

fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '\u{a0}' => out.push('~'),
            c => out.push(c),
        }
    }
}

/// A line break in a translation becomes a space (a blank line would start a new paragraph): every run of blanks
/// around line breaks is one space.
fn fold_newlines(s: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"[ \t]*(?:(?:\r\n|\r|\n)[ \t]*)+").expect("newline pattern"));
    re.replace_all(s, " ").into_owned()
}

/// A control word ends at the first non-letter: the text ends in `\` + ASCII letters preceded by an odd run of
/// backslashes (`\TeX`, not `\\TeX`).
fn ends_in_control_word(s: &str) -> bool {
    let b = s.as_bytes();
    let mut k = b.len();
    while k > 0 && b[k - 1].is_ascii_alphabetic() {
        k -= 1;
    }
    if k == b.len() {
        return false;
    }
    let mut slashes = 0;
    while k > 0 && b[k - 1] == b'\\' {
        k -= 1;
        slashes += 1;
    }
    slashes % 2 == 1
}

/// A point token that is `\\` or `\\*` with no `[..]` of its own.
fn is_bare_newline(raw: &str) -> bool {
    matches!(raw.trim_end_matches([' ', '\t']), "\\\\" | "\\\\*")
}

/// In the source, the text right after standalone tag `id` starts (after blanks) with `[`.
fn bracket_after_in_source(u: &Unit, id: &str) -> bool {
    let src = u.source();
    let Some(k) = src.iter().position(|i| matches!(i, Inline::Tag { id: t, kind: TagKind::Standalone, .. } if t == id)) else {
        return false;
    };
    matches!(src.get(k + 1), Some(Inline::Text(t)) if t.trim_start_matches([' ', '\t']).starts_with('['))
}

/// Strips the translation like Python `strip()` and merges neighbouring text.
pub fn normalize(inl: &[Inline]) -> Vec<Inline> {
    let mut v: Vec<Inline> = Vec::with_capacity(inl.len());
    for i in inl {
        match (i, v.last_mut()) {
            (Inline::Text(t), Some(Inline::Text(p))) => p.push_str(t),
            (Inline::Text(t), _) if t.is_empty() => {}
            _ => v.push(i.clone()),
        }
    }
    if let Some(Inline::Text(t)) = v.first_mut() {
        *t = t.trim_start_matches(py_space).to_string();
    }
    if let Some(Inline::Text(t)) = v.last_mut() {
        *t = t.trim_end_matches(py_space).to_string();
    }
    v.retain(|i| !matches!(i, Inline::Text(t) if t.is_empty()));
    v
}

/// The LaTeX for a translated unit. Braces always stay balanced: a close without its open is dropped, an open the
/// translation never closes (or closes out of order) is closed here. A point token that ends in a control word
/// gets `{}` when letters or a blank follow it.
pub fn render(u: &Unit, target: &[Inline]) -> String {
    let kind = u.context.kind.as_deref().unwrap_or("");
    // inside `[..]` (an item label, a short title) a `]` must sit in braces
    let t = normalize(target);
    let mut out: Vec<String> = Vec::new();
    let mut opened: Vec<&str> = Vec::new();
    let mut words: Vec<usize> = Vec::new();
    let mut breaks: Vec<usize> = Vec::new(); // a bare `\\` (it looks for `[` after blanks)
    for i in &t {
        match i {
            Inline::Text(x) => {
                let mut s = String::with_capacity(x.len());
                let folded = fold_newlines(x);
                // a token the translation invented (or a closing mark of a standalone code) is dropped, as LocOS does
                let folded = if folded.contains('\u{27E6}') { placeholder_re().replace_all(&folded, "").into_owned() } else { folded };
                escape_text(&folded, &mut s);
                if matches!(kind, "label" | "short") {
                    s = s.replace(']', "{]}");
                }
                out.push(s);
            }
            Inline::Tag { id, kind, .. } => {
                let Some(code) = u.codes.get(id) else { continue };
                match kind {
                    TagKind::Open => {
                        if let (Some(o), Some(_)) = (&code.open, &code.close) {
                            if !opened.contains(&id.as_str()) {
                                opened.push(id);
                                out.push(o.clone());
                            }
                        }
                    }
                    TagKind::Close => {
                        if opened.contains(&id.as_str()) {
                            while let Some(k) = opened.pop() {
                                if let Some(c) = u.codes.get(k).and_then(|c| c.close.clone()) {
                                    out.push(c);
                                }
                                if k == id {
                                    break;
                                }
                            }
                        }
                    }
                    TagKind::Standalone => {
                        if let Some(r) = &code.standalone {
                            if ends_in_control_word(r) {
                                words.push(out.len());
                            }
                            if is_bare_newline(r) && !bracket_after_in_source(u, id) {
                                breaks.push(out.len());
                            }
                            out.push(r.clone());
                        }
                    }
                }
            }
        }
    }
    while let Some(k) = opened.pop() {
        if let Some(c) = u.codes.get(k).and_then(|c| c.close.clone()) {
            out.push(c);
        }
    }
    // a paragraph or cell that starts with `[` would read as the optional argument of the command before it
    // (unless the source text started with one too: then it was already in a safe place)
    let raw_starts = u.codes.get("~raw").and_then(|c| c.standalone.as_deref()).is_some_and(|r| r.starts_with('['));
    if matches!(kind, "paragraph" | "cell") && !raw_starts {
        if let Some(first) = out.first_mut() {
            if let Some(rest) = first.strip_prefix('[') {
                *first = format!("{{[}}{rest}");
            }
        }
    }
    // a bare `\\` reads a following `[` (after blanks) as its optional length: a translation that puts text starting
    // with `[` after it gets `{[}`, unless the source had a `[` there too
    for &w in &breaks {
        if let Some(next) = out.get_mut(w + 1) {
            let lead = next.len() - next.trim_start_matches([' ', '\t']).len();
            if next[lead..].starts_with('[') {
                next.replace_range(lead..lead + 1, "{[}");
            }
        }
    }
    for &w in &words {
        let nxt = out[w + 1..].iter().find_map(|p| p.chars().next());
        if nxt.is_some_and(|c| py_alpha(c) || c == ' ' || c == '\t') {
            out[w].push_str("{}");
        }
    }
    out.concat()
}

/// True when the stripped text and tags of both are the same (a translation equal to the source changes nothing).
pub fn same(a: &[Inline], b: &[Inline]) -> bool {
    normalize(a) == normalize(b)
}
