//! Placeholders and HTML tags inside a string value become inline tags (the same set as `bilingua-json`). Each tag
//! has the label `placeholder` and its exact source markup as display (LocOS keeps them as text; the adapter
//! flattens them).

use bilingua_model::*;
use bilingua_model::locos;
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r"</?[A-Za-z][A-Za-z0-9:-]*(?:\s[^<>]*)?/?>", // HTML tag
            r"|\{\{[^{}]*\}\}",                           // {{count}}, {{ user.name }}
            r"|\$\{[^{}]*\}",                             // ${name}
            r"|\{[A-Za-z_0-9][A-Za-z0-9_.\-]*\}",         // {name}, {0}
            r"|%\([A-Za-z_][A-Za-z0-9_]*\)[-+0#]*\d*(?:\.\d+)?[sdifuxXeEgGc]", // %(name)s
            r"|%%",                                       // literal percent
            r"|%(?:\d+\$)?[-+0#]*\d*(?:\.\d+)?[sdifuoxXeEgGcp@]", // %s, %1$d, %.2f
        ))
        .expect("placeholder regex")
    })
}

const VOID: &[&str] = &["br", "hr", "img", "input", "wbr", "meta", "link", "area", "col", "source", "embed"];

struct Found {
    start: usize,
    end: usize,
    kind: TagKind,
    name: String, // lower case element name for HTML, empty for other placeholders
}

/// Splits a string value into text and tags. Returns the inlines and the original markup per tag id.
pub fn tag_placeholders(s: &str) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    // Every placeholder starts with one of these four bytes; most strings have none.
    if !s.bytes().any(|b| matches!(b, b'<' | b'{' | b'$' | b'%')) {
        return (if s.is_empty() { Vec::new() } else { vec![Inline::Text(s.to_string())] }, BTreeMap::new());
    }
    let mut found: Vec<Found> = Vec::new();
    for m in placeholder_re().find_iter(s) {
        let t = m.as_str();
        let (kind, name) = if let Some(rest) = t.strip_prefix("</") {
            (TagKind::Close, element_name(rest))
        } else if let Some(rest) = t.strip_prefix('<') {
            let name = element_name(rest);
            let void = t.ends_with("/>") || VOID.contains(&name.as_str());
            (if void { TagKind::Standalone } else { TagKind::Open }, name)
        } else {
            (TagKind::Standalone, String::new())
        };
        found.push(Found { start: m.start(), end: m.end(), kind, name });
    }

    // Pair open and close tags by element name. Unpaired ones become standalone.
    let mut partner: Vec<Option<usize>> = vec![None; found.len()];
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..found.len() {
        match found[i].kind {
            TagKind::Open => stack.push(i),
            TagKind::Close => {
                if let Some(pos) = stack.iter().rposition(|&o| found[o].name == found[i].name) {
                    let o = stack.remove(pos);
                    partner[o] = Some(i);
                    partner[i] = Some(o);
                }
            }
            TagKind::Standalone => {}
        }
    }

    let mut inl = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut ids: Vec<Option<String>> = vec![None; found.len()];
    let mut next = 1usize;
    let mut pos = 0usize;
    for (i, f) in found.iter().enumerate() {
        if f.start > pos {
            inl.push(Inline::Text(s[pos..f.start].to_string()));
        }
        pos = f.end;
        let markup = &s[f.start..f.end];
        let (kind, id) = match (f.kind, partner[i]) {
            (TagKind::Open, Some(_)) => {
                let id = next.to_string();
                next += 1;
                ids[i] = Some(id.clone());
                codes.insert(id.clone(), CodeData { open: Some(markup.to_string()), label: label(f), ..Default::default() });
                (TagKind::Open, id)
            }
            (TagKind::Close, Some(o)) => {
                let id = ids[o].clone().expect("open comes first");
                if let Some(c) = codes.get_mut(&id) {
                    c.close = Some(markup.to_string());
                }
                (TagKind::Close, id)
            }
            _ => {
                let id = next.to_string();
                next += 1;
                codes.insert(id.clone(), CodeData { standalone: Some(markup.to_string()), label: label(f), ..Default::default() });
                (TagKind::Standalone, id)
            }
        };
        if kind == TagKind::Standalone && !markup.starts_with('<') && locos::is_named_placeholder(markup) {
            inl.push(Inline::free_tag(id, markup));
        } else {
            inl.push(Inline::tag(id, kind, markup));
        }
    }
    if pos < s.len() {
        inl.push(Inline::Text(s[pos..].to_string()));
    }
    (inl, codes)
}

fn element_name(rest: &str) -> String {
    rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == ':' || *c == '-').collect::<String>().to_ascii_lowercase()
}

/// Every tag is labelled `placeholder`: Localization OS keeps these as plain text in a JSON5 unit (its segmenter
/// protects them later), so the LocOS adapter flattens a `placeholder` tag back to its display, the exact markup.
fn label(_f: &Found) -> Option<String> {
    Some("placeholder".to_string())
}
