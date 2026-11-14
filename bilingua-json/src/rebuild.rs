//! Writing a resource-dialect file: the translated messages are spliced into the original text; on a target
//! language, ICU plurals and go-i18n plural objects follow the target's CLDR categories, a suffix-style ARB plural
//! group (`itemCountOne`, `itemCountOther`) gets the categories it lacks, and an ARB-shaped file's `@@locale` names
//! the target.

use crate::dialect::{self, Dialect, CLDR};
use crate::plan::{self, Trans};
use crate::scan;
use crate::tree::{Node, Obj};
use crate::RAW_KEY;
use bilingua_model::*;
use std::collections::{HashMap, HashSet};

type Edit = (usize, usize, String);

pub fn splice(orig: &str, doc: &Document, d: Dialect, target: Option<&str>, locos: bool) -> Result<String, Error> {
    let root = crate::tree::scan(orig).map_err(|()| Error::Write("the skeleton is not a JSON resource file any more".into()))?;
    let msgs = dialect::messages(&root, d);
    let mut edits: Vec<Edit> = Vec::new();
    let mut head = 0usize;
    for m in &msgs {
        let plan = plan::plan_message(m.value, m.syntax);
        let k = plan.units.len();
        let units = doc.units.get(head..head + k).ok_or_else(|| Error::Write("the units no longer match the skeleton".into()))?;
        if units.first().is_some_and(|u| u.codes.get(RAW_KEY).and_then(|c| c.standalone.as_deref()) != Some(&orig[m.start..m.end])) {
            return Err(Error::Write("the units no longer match the skeleton".into()));
        }
        head += k;
        let touched = units.iter().any(|u| u.segments.iter().any(|s| s.has_target()));
        if !touched && target.is_none() {
            continue;
        }
        let mut tr = Trans { list: units.iter().map(|u| if u.segments.iter().all(|s| !s.has_target()) { None } else { Some(u.output()) }).collect(), at: 0 };
        let new = plan::rebuild_message(m.value, &plan, m.syntax, &mut tr, target);
        if new != m.value {
            edits.push((m.start, m.end, scan::quote(&new)));
        }
    }
    let mut object_edits: Vec<Edit> = Vec::new();
    if let (Some(t), Node::Obj(r)) = (target, &root) {
        match d {
            Dialect::Go => object_edits = go_plural_edits(orig, &root, &edits, t, locos),
            Dialect::Arb => {
                let changed: HashMap<usize, &String> = edits.iter().map(|e| (e.0, &e.2)).collect();
                let more = suffix_plural_edits(orig, r, t, &changed, locos);
                edits.extend(more);
                edits.push(locale_edit(orig, r, t, locos));
            }
            _ => {}
        }
    } else if let (Some(t), Dialect::Go) = (target, d) {
        object_edits = go_plural_edits(orig, &root, &edits, t, locos);
    }
    if !object_edits.is_empty() {
        let mut spans: Vec<(usize, usize)> = object_edits.iter().map(|e| (e.0, e.1)).collect();
        spans.sort_unstable();
        edits.retain(|e| {
            let i = spans.partition_point(|s| s.0 <= e.0);
            !(i > 0 && e.1 <= spans[i - 1].1)
        });
        edits.extend(object_edits);
    }
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(orig.len() + 64);
    let mut pos = 0usize;
    for (s, e, rep) in edits {
        if s < pos {
            continue;
        }
        out.push_str(&orig[pos..s]);
        out.push_str(&rep);
        pos = e;
    }
    out.push_str(&orig[pos..]);
    Ok(out)
}

/// `\n` or `\r\n`, whichever line ending the newline at `nl` ends; always `\n` with `lf` (LocOS).
fn newline_at(text: &str, nl: usize, lf: bool) -> &'static str {
    if !lf && nl > 0 && text.as_bytes()[nl - 1] == b'\r' {
        "\r\n"
    } else {
        "\n"
    }
}

/// Sets `@@locale` to the target, or adds it as the first entry, indented like the first key.
fn locale_edit(text: &str, root: &Obj, target: &str, lf: bool) -> Edit {
    let loc = scan::quote(&target.trim().replace('-', "_"));
    if let Some(p) = root.pairs.iter().rev().find(|p| p.key == "@@locale") {
        if p.value.is_str() {
            return (p.value.start(), p.value.end(), loc);
        }
    }
    if let Some(first) = root.pairs.first() {
        let key_start = first.key_start;
        let sep = match text[root.start..key_start].rfind('\n') {
            Some(rel) => {
                let nl = root.start + rel;
                format!("{}{}", newline_at(text, nl, lf), &text[nl + 1..key_start])
            }
            None => " ".to_string(),
        };
        return (key_start, key_start, format!("\"@@locale\": {loc},{sep}"));
    }
    (root.start, root.end, format!("{{\"@@locale\": {loc}}}"))
}

/// Suffix-style plurals (`itemCountOne`, `itemCountOther`): a message for each category the target uses that the
/// group lacks, holding the `Other` message, placed before it.
fn suffix_plural_edits(text: &str, root: &Obj, target: &str, changed: &HashMap<usize, &String>, lf: bool) -> Vec<Edit> {
    const WORDS: [&str; 6] = ["Zero", "One", "Two", "Few", "Many", "Other"];
    let mut out = Vec::new();
    let Some(cats) = crate::cldr::categories(target, false) else { return out };
    let all_keys: HashSet<&str> = root.pairs.iter().map(|p| p.key.as_str()).collect();
    let mut order: Vec<&str> = Vec::new();
    let mut groups: HashMap<&str, HashMap<&str, (usize, &Node)>> = HashMap::new();
    for p in &root.pairs {
        if !p.value.is_str() || !p.key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            continue;
        }
        for w in WORDS {
            if let Some(stem) = p.key.strip_suffix(w).filter(|s| !s.is_empty()) {
                if !groups.contains_key(stem) {
                    order.push(stem);
                }
                groups.entry(stem).or_default().insert(w, (p.key_start, &p.value));
                break;
            }
        }
    }
    for stem in order {
        let nodes = &groups[stem];
        let Some(&(key_at, other)) = nodes.get("Other") else { continue };
        if nodes.len() < 2 {
            continue;
        }
        let missing: Vec<String> = cats
            .iter()
            .map(|c| {
                let mut w = c.to_string();
                w[..1].make_ascii_uppercase();
                w
            })
            .filter(|w| !nodes.contains_key(w.as_str()) && !all_keys.contains(format!("{stem}{w}").as_str()))
            .collect();
        if missing.is_empty() {
            continue;
        }
        let value = changed.get(&other.start()).map(|s| s.to_string()).unwrap_or_else(|| text[other.start()..other.end()].to_string());
        let sep = match text[root.start..key_at].rfind('\n') {
            Some(rel) => {
                let nl = root.start + rel;
                let indent = &text[nl + 1..key_at];
                if indent.trim().is_empty() {
                    format!("{}{indent}", newline_at(text, nl, lf))
                } else {
                    " ".to_string()
                }
            }
            None => " ".to_string(),
        };
        let rep: String = missing.iter().map(|w| format!("{}: {value},{sep}", scan::quote(&format!("{stem}{w}")))).collect();
        out.push((key_at, key_at, rep));
    }
    out
}

/// Rewrites each go-i18n plural object so its category keys match the target: missing categories copy the
/// translated `other`, unused ones go. Returns one edit per rewritten object (covering the object).
fn go_plural_edits(text: &str, root: &Node, edits: &[Edit], target: &str, lf: bool) -> Vec<Edit> {
    let mut out = Vec::new();
    let Some(cats) = crate::cldr::categories(target, false) else { return out };
    let replaced: HashMap<(usize, usize), &String> = edits.iter().map(|e| ((e.0, e.1), &e.2)).collect();
    let slice = |n: &Node| -> String { replaced.get(&(n.start(), n.end())).map(|s| s.to_string()).unwrap_or_else(|| text[n.start()..n.end()].to_string()) };
    for obj in dialect::go_plural_objects(root) {
        let cat_keys: Vec<String> = obj.pairs.iter().map(|p| p.key.to_lowercase()).filter(|k| CLDR.contains(&k.as_str())).collect();
        if !cat_keys.iter().any(|k| k == "other") {
            continue;
        }
        let have: HashSet<&str> = cat_keys.iter().map(String::as_str).collect();
        let want: HashSet<&str> = cats.iter().copied().collect();
        if have == want {
            continue;
        }
        let Some(other) = obj.pairs.iter().find(|p| p.key.to_lowercase() == "other") else { continue };
        let other_json = slice(&other.value);
        let (indent, nl) = pair_indent(text, obj, lf);
        let mut entries: Vec<String> = Vec::new();
        let mut inserted = false;
        for p in &obj.pairs {
            let lk = p.key.to_lowercase();
            if CLDR.contains(&lk.as_str()) && !cats.contains(&lk.as_str()) {
                continue;
            }
            if lk == "other" && !inserted {
                for c in &cats {
                    if !have.contains(c) {
                        entries.push(format!("\"{c}\": {other_json}"));
                    }
                }
                inserted = true;
            }
            entries.push(format!("{}: {}", scan::quote(&p.key), slice(&p.value)));
        }
        let joiner = if nl.is_empty() { ", ".to_string() } else { format!(",{nl}{indent}") };
        let lead = if nl.is_empty() { String::new() } else { format!("{nl}{indent}") };
        let tail = if nl.is_empty() { String::new() } else { format!("{nl}{}", close_indent(text, obj)) };
        out.push((obj.start, obj.end, format!("{{{lead}{}{tail}}}", entries.join(&joiner))));
    }
    out
}

/// The indent of the object's first member and the newline to use ("" when the object is on one line).
fn pair_indent(text: &str, obj: &Obj, lf: bool) -> (String, &'static str) {
    let Some(first) = obj.pairs.first() else { return (String::new(), "") };
    let seg = &text[obj.start..first.value.start()];
    let Some(at) = seg.rfind('\n') else { return (String::new(), "") };
    let line = &seg[at + 1..];
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    (line[..indent].to_string(), newline_at(text, obj.start + at, lf))
}

/// The white space before the object's closing brace on its own line ("" otherwise).
fn close_indent(text: &str, obj: &Obj) -> String {
    let inner = &text[obj.start..obj.end - 1];
    match inner.rfind('\n') {
        Some(nl) => {
            let tail = &inner[nl + 1..];
            if tail.trim().is_empty() {
                tail.to_string()
            } else {
                String::new()
            }
        }
        None => String::new(),
    }
}
