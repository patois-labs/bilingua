//! Inline codes of a `<Tuv>`: `<ut>` start/end pairs and points, `<df>` wrappers, as Bilingua tags numbered like the
//! LocOS tokens (`g` pairs, `x` points), and the writer that puts a translation back using the source's own bytes.

use crate::xml::{line_ends, text_value, Kid, Kind, Tree};
use bilingua_model::{merge_text, CodeData, Inline, TagKind};
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct Tok {
    pub inl: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
}

/// True when the text pieces, tags removed, hold more than white space.
pub fn has_text(inl: &[Inline]) -> bool {
    inl.iter().filter_map(|i| if let Inline::Text(t) = i { Some(t.as_str()) } else { None }).collect::<String>().trim() != ""
}

/// Leading and trailing white space of the tokenised text (it never reaches past a tag).
pub fn edges(inl: &[Inline]) -> (String, String) {
    let lead = match inl.first() {
        Some(Inline::Text(t)) => t[..t.len() - t.trim_start().len()].to_string(),
        _ => String::new(),
    };
    let trail = match inl.last() {
        Some(Inline::Text(t)) => t[t.trim_end().len()..].to_string(),
        _ => String::new(),
    };
    (lead, trail)
}

/// The inlines with the white space at both ends removed.
pub fn strip(mut inl: Vec<Inline>) -> Vec<Inline> {
    if let Some(Inline::Text(t)) = inl.first_mut() {
        *t = t.trim_start().to_string();
    }
    if let Some(Inline::Text(t)) = inl.last_mut() {
        *t = t.trim_end().to_string();
    }
    inl.retain(|i| !matches!(i, Inline::Text(t) if t.is_empty()));
    inl
}

fn chunk_text(text: &str, s: usize, e: usize, cdata: bool) -> String {
    if cdata {
        line_ends(&text[s + 9..e - 3])
    } else {
        text_value(&text[s..e])
    }
}

fn code_text(tree: &Tree, text: &str, el: usize) -> String {
    let mut s = String::new();
    for k in &tree.elems[el].kids {
        match *k {
            Kid::Text { s: a, e, cdata } => s.push_str(&chunk_text(text, a, e, cdata)),
            Kid::Elem(c) => s.push_str(&code_text(tree, text, c)),
            Kid::Misc => {}
        }
    }
    s
}

fn display(tree: &Tree, text: &str, el: usize) -> String {
    let d = code_text(tree, text, el);
    if !d.is_empty() {
        return d;
    }
    tree.attr_value(text, el, "DisplayText").filter(|v| !v.is_empty()).unwrap_or_else(|| "ut".to_string())
}

fn is_wrap(tree: &Tree, text: &str, el: usize) -> bool {
    let e = &tree.elems[el];
    if e.kids.iter().any(|k| !matches!(k, Kid::Text { .. })) {
        return true;
    }
    // leading text before any other node
    e.kids.iter().any(|k| match *k {
        Kid::Text { s, e, cdata } => !chunk_text(text, s, e, cdata).is_empty(),
        _ => false,
    })
}

pub fn tokenize(tree: &Tree, text: &str, tuv: usize) -> Tok {
    let mut t = Tok { inl: Vec::new(), codes: BTreeMap::new() };
    let mut n = 0usize;
    visit(tree, text, tuv, &mut t, &mut n);
    t.inl = merge_text(std::mem::take(&mut t.inl));
    t
}

fn visit(tree: &Tree, text: &str, el: usize, t: &mut Tok, n: &mut usize) {
    let kids = &tree.elems[el].kids;
    let is_ut = |k: &Kid| matches!(k, Kid::Elem(c) if tree.elems[*c].kind == Kind::Ut);
    let has_ut = kids.iter().any(is_ut);
    let mut partner: Vec<Option<usize>> = Vec::new();
    let mut num_of: Vec<usize> = Vec::new();
    if has_ut {
        partner = vec![None; kids.len()];
        num_of = vec![0; kids.len()];
        let mut stack: Vec<usize> = Vec::new();
        for (pos, k) in kids.iter().enumerate() {
            if let Kid::Elem(c) = k {
                if tree.elems[*c].kind != Kind::Ut {
                    continue;
                }
                let ty = tree.attr_value(text, *c, "Type").unwrap_or_default().trim().to_lowercase();
                if ty == "start" {
                    stack.push(pos);
                } else if ty == "end" {
                    if let Some(j) = stack.pop() {
                        partner[pos] = Some(j);
                        partner[j] = Some(pos);
                    }
                }
            }
        }
    }
    for (pos, k) in kids.iter().enumerate() {
        match *k {
            Kid::Text { s, e, cdata } => t.inl.push(Inline::Text(chunk_text(text, s, e, cdata))),
            Kid::Misc => {}
            Kid::Elem(c) => {
                let ce = &tree.elems[c];
                let lit = |x: usize| text[tree.elems[x].start..tree.elems[x].end].to_string();
                let label = tree.attr_value(text, c, "DisplayText");
                match (ce.kind, if has_ut { partner[pos] } else { None }) {
                    (Kind::Ut, Some(p)) if p > pos => {
                        *n += 1;
                        let id = n.to_string();
                        let Kid::Elem(close) = kids[p] else { unreachable!("partner is an element") };
                        t.codes.insert(id.clone(), CodeData { open: Some(lit(c)), close: Some(lit(close)), standalone: None, label });
                        t.inl.push(Inline::tag(id, TagKind::Open, display(tree, text, c)));
                        num_of[pos] = *n;
                    }
                    (Kind::Ut, Some(p)) => {
                        let id = num_of[p].to_string();
                        t.inl.push(Inline::tag(id, TagKind::Close, display(tree, text, c)));
                    }
                    (Kind::Df, _) if is_wrap(tree, text, c) => {
                        *n += 1;
                        let id = n.to_string();
                        let open = text[ce.start..ce.start_end].to_string();
                        let close = text[ce.end_start..ce.end].to_string();
                        t.codes.insert(id.clone(), CodeData { open: Some(open.clone()), close: Some(close.clone()), standalone: None, label: Some("df".into()) });
                        t.inl.push(Inline::tag(id.clone(), TagKind::Open, open));
                        visit(tree, text, c, t, n);
                        t.inl.push(Inline::tag(id, TagKind::Close, close));
                    }
                    _ => {
                        *n += 1;
                        let id = n.to_string();
                        let d = if ce.kind == Kind::Ut { display(tree, text, c) } else { lit(c) };
                        t.codes.insert(id.clone(), CodeData { open: None, close: None, standalone: Some(lit(c)), label });
                        t.inl.push(Inline::tag(id, TagKind::Standalone, d));
                    }
                }
            }
        }
    }
}

/// Gives the tags of an existing target the ids of the source tags with the same markup (in order); a tag the
/// source does not have gets a new id `tN` and its own code.
pub fn map_target(src: &BTreeMap<String, CodeData>, tgt: Tok) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let key = |c: &CodeData| -> (bool, String, String) {
        match (&c.open, &c.close, &c.standalone) {
            (Some(o), Some(cl), _) => (true, o.clone(), cl.clone()),
            (_, _, Some(s)) => (false, s.clone(), String::new()),
            _ => (false, String::new(), String::new()),
        }
    };
    let mut ids: Vec<&String> = src.keys().collect();
    ids.sort_by_key(|id| std::cmp::Reverse(id.parse::<usize>().unwrap_or(0)));
    let mut free: HashMap<(bool, String, String), Vec<String>> = HashMap::new();
    for id in ids {
        free.entry(key(&src[id])).or_default().push(id.clone());
    }
    let mut renamed: HashMap<String, String> = HashMap::new();
    let mut extra = BTreeMap::new();
    let mut fresh = 0usize;
    let mut out = Vec::with_capacity(tgt.inl.len());
    for i in tgt.inl {
        match i {
            Inline::Tag { id, kind, display, free: f } => {
                let new = if kind == TagKind::Close {
                    renamed.get(&id).cloned().unwrap_or_else(|| id.clone())
                } else {
                    let c = tgt.codes.get(&id);
                    let hit = c.and_then(|c| free.get_mut(&key(c)).and_then(|v| v.pop()));
                    let nid = hit.unwrap_or_else(|| {
                        fresh += 1;
                        let nid = format!("t{fresh}");
                        if let Some(c) = c {
                            extra.insert(nid.clone(), c.clone());
                        }
                        nid
                    });
                    renamed.insert(id.clone(), nid.clone());
                    nid
                };
                out.push(Inline::Tag { id: new, kind, display, free: f });
            }
            t => out.push(t),
        }
    }
    (out, extra)
}

/// A comparable form of a target, to tell whether it changed since it was read.
pub fn signature(inl: &[Inline]) -> String {
    let mut s = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let k = match kind {
                    TagKind::Open => 'o',
                    TagKind::Close => 'c',
                    TagKind::Standalone => 's',
                };
                s.push_str(&format!("\u{27E6}{k}{id}\u{27E7}"));
            }
        }
    }
    s
}

pub fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace("]]>", "]]&gt;")
}

/// XML 1.0 characters only; pasted Word breaks become newlines; line ends normalised.
fn sanitize(s: &str) -> String {
    let s = if s.contains('\r') { line_ends(s) } else { s.to_string() };
    s.chars()
        .map(|c| if matches!(c, '\u{0B}' | '\u{0C}') { '\n' } else { c })
        .filter(|c| !matches!(*c as u32, 0..=8 | 0x0E..=0x1F | 0xFFFE | 0xFFFF))
        .collect()
}

/// The XML content for a translation, using the source's own bytes for every code. Tags the source does not have,
/// repeated tags and stray closes are dropped and anything left open is closed, so the result is well-formed.
/// `None` when nothing but white space is left.
pub fn render(inl: &[Inline], codes: &BTreeMap<String, CodeData>, crlf: bool) -> Option<String> {
    let cleaned: Vec<Inline> = inl
        .iter()
        .map(|i| match i {
            Inline::Text(t) => Inline::Text(sanitize(t)),
            x => x.clone(),
        })
        .collect();
    let v = strip(merge_text(cleaned));
    if v.is_empty() {
        return None;
    }
    let nl = |s: String| if crlf { s.replace('\n', "\r\n") } else { s };
    let mut out = String::new();
    let mut used: HashSet<&str> = HashSet::new();
    let mut stack: Vec<(&str, Option<&str>)> = Vec::new();
    for i in &v {
        match i {
            Inline::Text(t) => {
                out.push_str(&nl(esc_text(t)));
            }
            // hidden splice data (`~raw`, `~meta`) is never a code a translation can place
            Inline::Tag { id, .. } if id.starts_with('~') => {}
            Inline::Tag { id, kind, .. } => {
                let c = codes.get(id.as_str());
                match kind {
                    TagKind::Close => {
                        if stack.last().is_some_and(|(k, _)| *k == id.as_str()) {
                            if let Some((_, Some(close))) = stack.pop() {
                                out.push_str(close);
                            }
                        }
                    }
                    TagKind::Open => match c {
                        Some(CodeData { open: Some(o), close, .. }) if !used.contains(id.as_str()) => {
                            used.insert(id.as_str());
                            out.push_str(o);
                            stack.push((id.as_str(), Some(close.as_deref().unwrap_or(""))));
                        }
                        _ => stack.push((id.as_str(), None)),
                    },
                    TagKind::Standalone => {
                        if used.contains(id.as_str()) {
                            continue;
                        }
                        match c {
                            Some(CodeData { standalone: Some(s), .. }) => out.push_str(s),
                            Some(CodeData { open: Some(o), close: Some(cl), .. }) => {
                                out.push_str(o);
                                out.push_str(cl);
                            }
                            _ => continue,
                        }
                        used.insert(id.as_str());
                    }
                }
            }
        }
    }
    for (_, close) in stack.into_iter().rev() {
        if let Some(c) = close {
            out.push_str(c);
        }
    }
    Some(out)
}
