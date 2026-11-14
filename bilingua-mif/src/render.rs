//! Writing a translated paragraph back as MIF statements.

use crate::esc::encode_string;
use crate::para::{unpack, HIDDEN};
use bilingua_model::{Error, Inline, TagKind, Unit};
use std::collections::{HashMap, HashSet};

/// A tag of the translation as the LocOS token grammar sees it: (is a close, kind letter, id).
type Tok<'a> = (bool, char, &'a str);

/// Pairs the translation writes whole (its first open, the first close after it) and nested as in the source:
/// directly inside the same enclosing pair. A restore statement returns to the font around its change, so a pair
/// anywhere else, or one whose enclosing pair was dropped, would format the wrong words: it is dropped (its words
/// stay, its font change goes).
fn live_pairs<'a>(
    parent: &HashMap<&'a str, Option<&'a str>>,
    toks: &[Tok<'a>],
) -> HashMap<&'a str, (usize, usize)> {
    let mut opens: HashMap<&str, usize> = HashMap::new();
    let mut closes: HashMap<&str, usize> = HashMap::new();
    for (k, &(slash, kind, tid)) in toks.iter().enumerate() {
        if !parent.contains_key(tid) || kind != 'g' {
            continue;
        }
        if !slash {
            opens.entry(tid).or_insert(k);
        } else if opens.contains_key(tid) {
            closes.entry(tid).or_insert(k);
        }
    }
    let mut live: HashMap<&str, (usize, usize)> =
        closes.iter().map(|(&t, &c)| (t, (opens[t], c))).collect();
    loop {
        let mut at: Vec<(usize, &str, bool)> = Vec::with_capacity(live.len() * 2);
        for (&t, &(o, c)) in &live {
            at.push((o, t, false));
            at.push((c, t, true));
        }
        at.sort_unstable();
        let mut stack: Vec<&str> = Vec::new();
        let mut bad: HashSet<&str> = HashSet::new();
        for (_, tid, end) in at {
            if !end {
                if parent[tid] != stack.last().copied() {
                    bad.insert(tid);
                }
                stack.push(tid);
            } else if stack.last() == Some(&tid) {
                stack.pop();
            } else {
                bad.insert(tid);
                if let Some(p) = stack.iter().position(|&s| s == tid) {
                    stack.remove(p);
                }
            }
        }
        if bad.is_empty() {
            return live;
        }
        for t in bad {
            live.remove(t);
        }
    }
}

#[derive(PartialEq, Clone)]
enum Piece<'a> {
    Text(String),
    Tok(&'a str),
    Open(&'a str),
    Close(&'a str),
}

/// A pair the translation emptied (`open close` with nothing between) is a font change around nothing.
fn drop_empty_pairs(pieces: Vec<Piece<'_>>) -> Vec<Piece<'_>> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    for p in pieces {
        match &p {
            Piece::Text(t) if t.is_empty() => continue,
            Piece::Close(c) if matches!(out.last(), Some(Piece::Open(o)) if o == c) => {
                out.pop();
                continue;
            }
            _ => {}
        }
        out.push(p);
    }
    out
}

/// The text of the translated paragraph's ParaLine statements (what replaces the source's first to last ParaLine).
pub fn render(unit: &Unit, output: &[Inline]) -> Result<String, Error> {
    let bad = || Error::Write(format!("unit {} lost its MIF rebuild data", unit.id));
    let hidden = unit.codes.get(HIDDEN).and_then(|h| h.open.as_deref()).ok_or_else(bad)?;
    let fields = unpack(hidden).ok_or_else(bad)?;
    let &[nl, ind, body, tail, extras] = fields.as_slice() else {
        return Err(bad());
    };
    let body = unpack(body).ok_or_else(bad)?;
    let tail = unpack(tail).ok_or_else(bad)?;
    let extras = unpack(extras).ok_or_else(bad)?;

    // the source's tags: point tokens in order, pairs and their enclosing pair
    let source = unit.source();
    let mut points: Vec<&str> = Vec::new();
    let mut parent: HashMap<&str, Option<&str>> = HashMap::new();
    let mut open_stack: Vec<&str> = Vec::new();
    for i in &source {
        if let Inline::Tag { id, kind, .. } = i {
            match kind {
                TagKind::Standalone => points.push(id),
                TagKind::Open => {
                    parent.insert(id, open_stack.last().copied());
                    open_stack.push(id);
                }
                TagKind::Close => {
                    if let Some(p) = open_stack.iter().position(|s| *s == id) {
                        open_stack.remove(p);
                    }
                }
            }
        }
    }
    let point_set: HashSet<&str> = points.iter().copied().collect();

    let toks: Vec<Tok> = output
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { id, kind, .. } => Some(match kind {
                TagKind::Standalone => (false, 'x', id.as_str()),
                TagKind::Open => (false, 'g', id.as_str()),
                TagKind::Close => (true, 'g', id.as_str()),
            }),
            Inline::Text(_) => None,
        })
        .collect();
    let live = live_pairs(&parent, &toks);

    let mut used: HashSet<&str> = HashSet::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut k = 0usize;
    for i in output {
        match i {
            Inline::Text(t) => pieces.push(Piece::Text(t.clone())),
            Inline::Tag { .. } => {
                let (slash, kind, tid) = toks[k];
                if parent.contains_key(tid) {
                    if let Some(&(o, c)) = live.get(tid) {
                        if k == o || k == c {
                            pieces.push(if slash { Piece::Close(tid) } else { Piece::Open(tid) });
                        }
                    }
                } else if kind == 'x' && !slash && point_set.contains(tid) && !used.contains(tid) {
                    used.insert(tid);
                    pieces.push(Piece::Tok(tid));
                }
                k += 1;
            }
        }
    }
    for p in &points {
        if !used.contains(p) {
            pieces.push(Piece::Tok(p));
        }
    }

    let code = |id: &str| unit.codes.get(id);
    let cind = format!("{ind} ");
    let mut lines: Vec<Vec<String>> = vec![body.iter().map(|s| s.to_string()).collect()];
    for p in drop_empty_pairs(pieces) {
        match p {
            Piece::Tok(id) => {
                let stmts = code(id)
                    .and_then(|c| c.open.as_deref())
                    .and_then(unpack)
                    .ok_or_else(bad)?;
                let last = lines.last_mut().expect("a line");
                last.extend(stmts.into_iter().map(String::from));
            }
            Piece::Open(id) | Piece::Close(id) => {
                let c = code(id).ok_or_else(bad)?;
                let raw = if matches!(p, Piece::Open(_)) { &c.open } else { &c.close };
                lines
                    .last_mut()
                    .expect("a line")
                    .push(raw.clone().ok_or_else(bad)?);
            }
            Piece::Text(t) => {
                let t = t.replace("\r\n", "\n").replace('\r', "\n");
                for (j, seg) in t.split('\n').enumerate() {
                    if j > 0 {
                        lines.last_mut().expect("a line").push("<Char HardReturn>".into());
                        lines.push(Vec::new());
                    }
                    if !seg.is_empty() {
                        lines
                            .last_mut()
                            .expect("a line")
                            .push(format!("<String `{}'>", encode_string(seg)));
                    }
                }
            }
        }
    }
    lines
        .last_mut()
        .expect("a line")
        .extend(tail.iter().map(|s| s.to_string()));
    let n = lines.len();
    let mut out: Vec<String> = Vec::with_capacity(n + extras.len());
    for (j, stmts) in lines.iter().enumerate() {
        let head = if j == 0 { "" } else { ind };
        let end = if j == n - 1 { ">" } else { "> # end of ParaLine" };
        let mut s = String::new();
        s.push_str(head);
        s.push_str("<ParaLine");
        s.push_str(nl);
        for st in stmts {
            s.push_str(&cind);
            s.push_str(st);
            s.push_str(nl);
        }
        s.push_str(ind);
        s.push_str(end);
        out.push(s);
    }
    for e in extras {
        out.push(format!("{ind}{e}"));
    }
    Ok(out.join(nl))
}
