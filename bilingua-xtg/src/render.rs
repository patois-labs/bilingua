//! A translated unit back to XPress Tags text (LocOS `_render`): text re-escaped, pairs written whole or not at
//! all, point tags that the translation dropped appended at the end.

use crate::lex::is_definition;
use crate::para::{resets_id, GROUPS, RESET_ALL};
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Literal text to XPress Tags text: `\ < @` escaped, a line break becomes `<\n>` (a paragraph is one line),
/// other C0 controls dropped.
pub fn escape_text(s: &str, out: &mut String) {
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '\\' => out.push_str("<\\\\>"),
            '<' => out.push_str("<\\<>"),
            '@' => out.push_str("<\\@>"),
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                out.push_str("<\\n>");
            }
            '\n' => out.push_str("<\\n>"),
            c if c == '\t' || (c as u32) >= 0x20 => out.push(c),
            _ => {}
        }
    }
}

struct M<'a> {
    slash: bool,
    pair_kind: bool,
    tid: &'a str,
    /// No text between the previous match and this one.
    adj_prev: bool,
}

pub fn render(codes: &BTreeMap<String, CodeData>, hidden: &BTreeMap<String, String>, new: &[Inline]) -> String {
    let paired: HashSet<&str> = codes.iter().filter(|(_, c)| c.open.is_some()).map(|(k, _)| k.as_str()).collect();
    let code_str = |id: &str| hidden.get(id).map(String::as_str);
    let groups: Vec<Vec<&str>> = hidden
        .get(GROUPS)
        .map(|s| s.split(';').map(|g| g.split(',').collect()).collect())
        .unwrap_or_default();
    let reset_all: HashSet<&str> = code_str(RESET_ALL).map(|s| s.split(',').collect()).unwrap_or_default();
    let resets = |id: &str| code_str(&resets_id(id)).unwrap_or("").to_string();

    // matches, in order
    let mut ms: Vec<M> = Vec::new();
    let mut any_text_since_last = false;
    for i in new {
        match i {
            Inline::Text(t) => {
                if !t.is_empty() {
                    any_text_since_last = true;
                }
            }
            Inline::Tag { id, kind, .. } => {
                ms.push(M {
                    slash: *kind == TagKind::Close,
                    pair_kind: *kind != TagKind::Standalone,
                    tid: id.as_str(),
                    adj_prev: !any_text_since_last,
                });
                any_text_since_last = false;
            }
        }
    }

    // the pairs the translation writes whole: first open, then the first close after it
    let mut opens: HashMap<&str, usize> = HashMap::new();
    let mut closes: HashMap<&str, usize> = HashMap::new();
    for (k, m) in ms.iter().enumerate() {
        if !paired.contains(m.tid) || !m.pair_kind {
            continue;
        }
        if !m.slash {
            opens.entry(m.tid).or_insert(k);
        } else if opens.contains_key(m.tid) {
            closes.entry(m.tid).or_insert(k);
        }
    }
    let live: HashMap<&str, (usize, usize)> = closes.iter().map(|(&t, &c)| (t, (opens[t], c))).collect();

    let mut close_as: HashMap<&str, String> = HashMap::new();
    let mut single: HashSet<&str> = HashSet::new();
    for group in &groups {
        let ks: Vec<Option<usize>> = group.iter().map(|t| live.get(t).map(|p| p.1)).collect();
        let intact = ks.iter().all(Option::is_some)
            && ks.windows(2).all(|w| matches!((w[0], w[1]), (Some(a), Some(b)) if b == a + 1 && ms[b].adj_prev));
        let Some((&last, init)) = group.split_last() else { continue };
        for &t in init {
            close_as.insert(t, if intact { String::new() } else { resets(t) });
        }
        let last_close = codes.get(last).and_then(|c| c.close.clone()).unwrap_or_default();
        close_as.insert(last, if intact { last_close } else if reset_all.contains(last) { "<$>".into() } else { resets(last) });
        if group.len() == 1 {
            single.insert(last);
        }
    }
    let write: HashMap<usize, &str> = live.iter().flat_map(|(&t, &(o, c))| [(o, t), (c, t)]).collect();

    let mut used: HashSet<&str> = HashSet::new();
    let mut emptied: HashSet<&str> = HashSet::new();
    let mut out = String::new();
    let mut pending = String::new();
    let mut k = 0usize;
    for i in new {
        match i {
            Inline::Text(t) => pending.push_str(t),
            Inline::Tag { .. } => {
                escape_text(&pending, &mut out);
                pending.clear();
                let m = &ms[k];
                let this = k;
                k += 1;
                let tid = m.tid;
                if paired.contains(tid) {
                    if write.get(&this) != Some(&tid) || !m.pair_kind {
                        continue;
                    }
                    if m.slash {
                        if !emptied.contains(tid) {
                            out.push_str(close_as.get(tid).map(String::as_str).unwrap_or(""));
                        }
                        continue;
                    }
                    let c = live[tid].1;
                    if single.contains(tid) && c == this + 1 && ms[c].adj_prev {
                        emptied.insert(tid);
                        continue;
                    }
                    out.push_str(codes.get(tid).and_then(|c| c.open.as_deref()).unwrap_or(""));
                    continue;
                }
                if !m.slash && !m.pair_kind && !used.contains(tid) {
                    if let Some(raw) = codes.get(tid).and_then(|c| c.standalone.as_deref()) {
                        used.insert(tid);
                        out.push_str(raw);
                    }
                }
            }
        }
    }
    escape_text(&pending, &mut out);
    // point tags the translation dropped, in source order
    let mut left: Vec<(u64, &str, &str)> = codes
        .iter()
        .filter(|(k, c)| !k.starts_with('~') && c.standalone.is_some() && !paired.contains(k.as_str()) && !used.contains(k.as_str()))
        .map(|(k, c)| (k.parse::<u64>().unwrap_or(u64::MAX), k.as_str(), c.standalone.as_deref().unwrap_or("")))
        .collect();
    left.sort();
    for (_, _, raw) in left {
        out.push_str(raw);
    }
    if out.starts_with('~') && is_definition(&out) {
        // a translation reading `~Name=<` would turn the paragraph into a bookmark definition
        out.replace_range(0..1, "<\\#126>");
    }
    out
}
