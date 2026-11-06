//! Namespace-aware helpers on the shared tree: what lxml's Clark names give LocOS's DOCX filter.

use bilingua_xmltree::{Dom, Kind};

pub const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
pub const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
pub const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
pub const DC: &str = "http://purl.org/dc/elements/1.1/";

pub fn is_w(d: &Dom, id: usize, local: &str) -> bool {
    d.is(id, W, local)
}

/// `el.get(qn("w:<local>"))`.
pub fn get_w<'a>(d: &'a Dom, id: usize, local: &str) -> Option<&'a str> {
    d.get_ns(id, W, local)
}

pub fn find_w(d: &Dom, id: usize, local: &str) -> Option<usize> {
    d.find(id, W, local)
}

pub fn parent(d: &Dom, id: usize) -> Option<usize> {
    d.nodes[id].parent
}

/// The element children of `id` (lxml's `for child in el`, comments and processing instructions left out).
pub fn kids(d: &Dom, id: usize) -> Vec<usize> {
    d.element_children(id).collect()
}

/// `"".join(t.text for t in el.iter(w:t))`: the text of every `w:t` in and under `el` (lxml's `.text`, the text
/// before a `w:t`'s first child).
pub fn t_text(d: &Dom, el: usize) -> String {
    d.iter(el).into_iter().filter(|&t| is_w(d, t, "t")).map(|t| d.lead_text(t)).collect()
}

/// The prefix bound to `ns` in a scope, innermost first, not shadowed by a later binding of the same prefix
/// (`attr`: a prefixed one, as an attribute needs).
fn prefix_in(d: &Dom, scope: usize, ns: &str, attr: bool) -> Option<String> {
    let sc = &d.scopes[scope];
    for (i, (p, u)) in sc.iter().enumerate().rev() {
        if u == ns && (!attr || !p.is_empty()) && !sc[i + 1..].iter().any(|(q, _)| q == p) {
            return Some(p.clone());
        }
    }
    None
}

/// A new element `{ns}local` for a place under `parent` (python-docx `OxmlElement`, lxml reusing the prefix the
/// namespace has there, or declaring `want_prefix` on the element when it has none). Not attached yet.
pub fn new_el(d: &mut Dom, parent: usize, ns: &str, local: &str, want_prefix: &str) -> usize {
    let scope = d.nodes[parent].scope;
    match prefix_in(d, scope, ns, false) {
        Some(p) => {
            let name = if p.is_empty() { local.to_string() } else { format!("{p}:{local}") };
            d.new_element(&name, scope, Some(ns.to_string()))
        }
        None => {
            let mut s = d.scopes[scope].clone();
            s.push((want_prefix.to_string(), ns.to_string()));
            d.scopes.push(s);
            let sc = d.scopes.len() - 1;
            let id = d.new_element(&format!("{want_prefix}:{local}"), sc, Some(ns.to_string()));
            d.set(id, &format!("xmlns:{want_prefix}"), ns);
            id
        }
    }
}

pub fn new_w(d: &mut Dom, parent: usize, local: &str) -> usize {
    new_el(d, parent, W, local, "w")
}

/// `el.set("{ns}local", value)`: the attribute already there is replaced, else it is added with the prefix the
/// namespace has on the element (declared on it when there is none).
pub fn set_ns(d: &mut Dom, id: usize, ns: &str, local: &str, want_prefix: &str, value: &str) {
    let found = d.nodes[id].attrs.iter().map(|(k, _)| k.clone()).find(|k| {
        !(k == "xmlns" || k.starts_with("xmlns:")) && d.attr_ns(id, k).as_deref() == Some(ns) && k.split_once(':').map(|(_, l)| l) == Some(local)
    });
    if let Some(k) = found {
        d.set(id, &k, value);
        return;
    }
    let scope = d.nodes[id].scope;
    let prefix = match prefix_in(d, scope, ns, true) {
        Some(p) => p,
        None => {
            let mut s = d.scopes[scope].clone();
            s.push((want_prefix.to_string(), ns.to_string()));
            d.scopes.push(s);
            d.nodes[id].scope = d.scopes.len() - 1;
            d.set(id, &format!("xmlns:{want_prefix}"), ns);
            want_prefix.to_string()
        }
    };
    d.set(id, &format!("{prefix}:{local}"), value);
}

pub fn set_w(d: &mut Dom, id: usize, local: &str, value: &str) {
    set_ns(d, id, W, local, "w", value);
}

/// lxml's `addnext` / `addprevious` / `append` move a node and declare what its names need in the new place;
/// elements moved or copied here keep the namespace scope they were made in, so before writing each element gets
/// the declarations its own and its attributes' prefixes need where its ancestors do not make them.
pub fn reconcile(d: &mut Dom) {
    let root = d.root;
    // (prefix, uri) bindings made by the real ancestors, innermost last; frames remember where to truncate
    let mut bound: Vec<(String, String)> = vec![("xml".into(), "http://www.w3.org/XML/1998/namespace".into())];
    let mut stack: Vec<(usize, usize, usize)> = Vec::new(); // (node, next child, bound length before it)
    let enter = |d: &mut Dom, id: usize, bound: &mut Vec<(String, String)>| {
        for (k, v) in d.nodes[id].attrs.iter() {
            if k == "xmlns" {
                bound.push((String::new(), v.clone()));
            } else if let Some(p) = k.strip_prefix("xmlns:") {
                bound.push((p.to_string(), v.clone()));
            }
        }
        let name = d.nodes[id].name.clone();
        let mut need: Vec<String> = vec![name.split_once(':').map(|(p, _)| p.to_string()).unwrap_or_default()];
        for (k, _) in d.nodes[id].attrs.iter() {
            if k == "xmlns" || k.starts_with("xmlns:") {
                continue;
            }
            if let Some((p, _)) = k.split_once(':') {
                if p != "xml" {
                    need.push(p.to_string());
                }
            }
        }
        for p in need {
            let scope = d.nodes[id].scope;
            let want = d.scopes[scope].iter().rev().find(|(q, _)| *q == p).map(|(_, u)| u.clone()).unwrap_or_default();
            let have = bound.iter().rev().find(|(q, _)| *q == p).map(|(_, u)| u.clone()).unwrap_or_default();
            if want != have && !(p.is_empty() && want.is_empty() && have.is_empty()) {
                let key = if p.is_empty() { "xmlns".to_string() } else { format!("xmlns:{p}") };
                if want.is_empty() && !p.is_empty() {
                    continue; // an unbound prefix cannot be declared away
                }
                d.set(id, &key, &want);
                bound.push((p, want));
            }
        }
    };
    if d.nodes[root].kind != Kind::Element {
        return;
    }
    enter(d, root, &mut bound);
    stack.push((root, 0, 1));
    while let Some(&(node, i, _)) = stack.last() {
        let kids = &d.nodes[node].children;
        if i >= kids.len() {
            let (_, _, len) = stack.pop().unwrap();
            bound.truncate(len);
            continue;
        }
        let c = kids[i];
        stack.last_mut().unwrap().1 += 1;
        if d.nodes[c].kind == Kind::Element {
            let len = bound.len();
            enter(d, c, &mut bound);
            stack.push((c, 0, len));
        }
    }
}

/// The part as python-docx writes it (`serialize_part_xml`: lxml, UTF-8, standalone declaration).
pub fn part_bytes(d: &mut Dom) -> Vec<u8> {
    reconcile(d);
    let mut s = String::from("<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n");
    s.push_str(&d.serialize());
    s.into_bytes()
}

/// The bytes of an XML part as text: UTF-8 (a BOM dropped) or UTF-16 with a BOM.
pub fn decode(data: &[u8]) -> Option<String> {
    if let Some(b) = data.strip_prefix(&[0xEF, 0xBB, 0xBF][..]) {
        return std::str::from_utf8(b).ok().map(str::to_string);
    }
    let utf16 = |b: &[u8], le: bool| -> Option<String> {
        if !b.len().is_multiple_of(2) {
            return None;
        }
        let units: Vec<u16> = b.chunks(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        String::from_utf16(&units).ok()
    };
    if let Some(b) = data.strip_prefix(&[0xFF, 0xFE][..]) {
        return utf16(b, true);
    }
    if let Some(b) = data.strip_prefix(&[0xFE, 0xFF][..]) {
        return utf16(b, false);
    }
    std::str::from_utf8(data).ok().map(str::to_string)
}

/// The canonical form of XML bytes ([`Dom::canonical`]), or None when they are not well-formed.
pub fn canonical(data: &[u8]) -> Option<String> {
    bilingua_xmltree::parse(&decode(data)?).ok().map(|d| d.canonical())
}
