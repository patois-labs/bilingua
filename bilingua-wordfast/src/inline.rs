//! Inline codes: the pieces a walker finds in a source or target element, the Bilingua tags and code table they
//! become, and the writer that turns a translated tag sequence back into markup using the source's own bytes.

use crate::text::{esc_text, xml_target_text};
use crate::xml::py_strip;
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::{BTreeMap, HashMap, HashSet};

/// What a walker found, in order.
pub enum Piece {
    Text(String),
    Code(CodePiece),
}

pub struct CodePiece {
    /// Identifies the element so a target code finds its source code.
    pub key: String,
    pub kind: TagKind,
    pub free: bool,
    pub display: String,
    /// Source token number (source walks only).
    pub id: usize,
    /// The element's own markup: the start tag for `Open`, the end tag for `Close`, the whole element for a point.
    pub raw: String,
    /// For `Open`: the markup of the matching close.
    pub raw_close: String,
}

pub struct Source {
    pub inlines: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    pub keys: HashMap<String, (String, TagKind, String, bool)>,
    pub lead: String,
    pub trail: String,
}

fn push_text(v: &mut Vec<Inline>, s: String) {
    if s.is_empty() {
        return;
    }
    if let Some(Inline::Text(t)) = v.last_mut() {
        t.push_str(&s);
    } else {
        v.push(Inline::Text(s));
    }
}

/// Removes the white space at both ends of the content (Python `strip`), returning what was removed.
fn strip_edges(v: &mut Vec<Inline>) -> (String, String) {
    if v.iter().all(|i| matches!(i, Inline::Text(t) if py_strip(t).is_empty())) {
        v.clear();
        return (String::new(), String::new());
    }
    let mut lead = String::new();
    let mut trail = String::new();
    if let Some(Inline::Text(t)) = v.first_mut() {
        let k = t.len() - t.trim_start_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)).len();
        lead = t[..k].to_string();
        t.drain(..k);
    }
    if let Some(Inline::Text(t)) = v.last_mut() {
        let k = t.trim_end_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)).len();
        trail = t[k..].to_string();
        t.truncate(k);
    }
    v.retain(|i| !matches!(i, Inline::Text(t) if t.is_empty()));
    (lead, trail)
}

pub fn build_source(pieces: Vec<Piece>) -> Source {
    let mut inlines = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut keys = HashMap::new();
    for p in pieces {
        match p {
            Piece::Text(t) => push_text(&mut inlines, t),
            Piece::Code(c) => {
                let id = c.id.to_string();
                match c.kind {
                    TagKind::Open => {
                        codes.insert(id.clone(), CodeData { open: Some(c.raw), close: Some(c.raw_close), ..Default::default() });
                    }
                    TagKind::Close => {}
                    TagKind::Standalone => {
                        codes.insert(id.clone(), CodeData { standalone: Some(c.raw), ..Default::default() });
                    }
                }
                keys.insert(c.key, (id.clone(), c.kind, c.display.clone(), c.free));
                inlines.push(Inline::Tag { id, kind: c.kind, display: c.display, free: c.free });
            }
        }
    }
    let (lead, trail) = strip_edges(&mut inlines);
    Source { inlines, codes, keys, lead, trail }
}

/// The target of an existing translation: its codes are the source codes with the same key; others are dropped
/// (their text stays).
pub fn build_target(pieces: Vec<Piece>, source: &Source) -> Vec<Inline> {
    let mut inlines = Vec::new();
    let mut used: HashSet<(String, TagKind)> = HashSet::new();
    for p in pieces {
        match p {
            Piece::Text(t) => push_text(&mut inlines, t),
            Piece::Code(c) => {
                if let Some((id, kind, display, free)) = source.keys.get(&c.key) {
                    if used.insert((id.clone(), *kind)) {
                        inlines.push(Inline::Tag { id: id.clone(), kind: *kind, display: display.clone(), free: *free });
                    }
                }
            }
        }
    }
    strip_edges(&mut inlines);
    inlines
}

/// A comparison string for a tag sequence (text and tag identities), used to tell a changed target from the one
/// that was loaded.
pub fn flat(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } => {
                s.push('\u{1}');
                s.push(match kind {
                    TagKind::Open => 'o',
                    TagKind::Close => 'c',
                    TagKind::Standalone => 's',
                });
                s.push_str(id);
                s.push('\u{2}');
            }
        }
    }
    s
}

/// Writes a translation as XML content: text escaped, every code taken from the source's table. A code the source
/// does not have, or a second use of one, is dropped; a stray close is ignored; whatever is left open is closed,
/// so the result is always well-formed. Leading and trailing white space of the translation is replaced by the
/// source's own (`lead`, `trail`).
pub fn render(target: &[Inline], codes: &BTreeMap<String, CodeData>, lead: &str, trail: &str) -> String {
    let mut items: Vec<Inline> = Vec::with_capacity(target.len());
    for i in target {
        match i {
            Inline::Text(t) => push_text(&mut items, xml_target_text(t)),
            t => items.push(t.clone()),
        }
    }
    let _ = strip_edges(&mut items);
    let mut out = String::new();
    out.push_str(lead);
    let mut used: HashSet<&str> = HashSet::new();
    // open codes: (id, close markup or None for a dropped open)
    let mut stack: Vec<(&str, Option<&str>)> = Vec::new();
    for i in &items {
        match i {
            Inline::Text(t) => esc_text(t, &mut out),
            // the unit's hidden entries (ids starting with `~`: its raw region, splice text) are never tags; a target
            // that names one (a document edited by hand) must not splice them into the file
            Inline::Tag { id, .. } if id.starts_with('~') => {}
            Inline::Tag { id, kind, .. } => {
                let code = codes.get(id.as_str());
                match kind {
                    TagKind::Open => match code {
                        Some(CodeData { open: Some(o), close: Some(c), .. }) if !used.contains(id.as_str()) => {
                            used.insert(id);
                            out.push_str(o);
                            stack.push((id, Some(c)));
                        }
                        _ => stack.push((id, None)),
                    },
                    TagKind::Close => {
                        if stack.last().is_some_and(|(k, _)| *k == id.as_str()) {
                            if let Some((_, Some(c))) = stack.pop() {
                                out.push_str(c);
                            }
                        }
                    }
                    TagKind::Standalone => {
                        if used.contains(id.as_str()) {
                            continue;
                        }
                        match code {
                            Some(CodeData { standalone: Some(s), .. }) => {
                                used.insert(id);
                                out.push_str(s);
                            }
                            // a paired element used as a point is written empty
                            Some(CodeData { open: Some(o), close: Some(c), .. }) => {
                                used.insert(id);
                                out.push_str(o);
                                out.push_str(c);
                            }
                            _ => {}
                        }
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
    out.push_str(trail);
    out
}
