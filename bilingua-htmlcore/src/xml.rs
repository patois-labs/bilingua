//! LocOS `html_guard._xml_view` and `_check_xml`: when the source page and the build are both well-formed XML
//! (XHTML, an EPUB chapter), an XML reader must find in the build only elements (name, namespace and attributes,
//! translated values aside) and script/style bodies the source has.
//!
//! LocOS parses with lxml (`resolve_entities="internal"`, no DTD loading, no network, `huge_tree=False`, no
//! recovery). This is a small well-formedness reader with the same answers on the cases that matter: an unknown
//! entity is an error unless the document names an external DTD (libxml2 then only warns), internal entities are
//! expanded, namespaces are resolved to `{uri}local`, and nesting past 256 levels (libxml2's limit) is not XML.
//! Anything it cannot read is "not XML", which skips the check, as a parse error does in LocOS.

use crate::browser::{free_attr, Attrs, Unsafe};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{HashMap, HashSet};

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const MAX_DEPTH: usize = 256;

/// The elements of a well-formed document as (signature, line), and its script/style bodies.
pub struct XmlView {
    pub sigs: Vec<(String, usize)>,
    pub bodies: HashSet<(String, String)>,
}

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// The attributes of a start tag `<name a="v" ...>` or `<name .../>`: (name, raw value), or `None` when the tag is
/// not well-formed (an attribute without a quoted value, a `<` in a value).
fn tag_parts(raw: &str) -> Option<(&str, Vec<(&str, &str)>)> {
    let inner = raw.strip_prefix('<')?.strip_suffix('>')?;
    let inner = inner.strip_suffix('/').unwrap_or(inner);
    let b = inner.as_bytes();
    let mut i = 0;
    while i < b.len() && !is_ws(b[i]) {
        i += 1;
    }
    let name = &inner[..i];
    if name.is_empty() {
        return None;
    }
    let mut attrs = Vec::new();
    loop {
        let ws = i;
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if i == ws {
            return None; // attributes must be separated by white space
        }
        let s = i;
        while i < b.len() && !is_ws(b[i]) && b[i] != b'=' {
            i += 1;
        }
        let an = &inner[s..i];
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        if an.is_empty() || b.get(i) != Some(&b'=') {
            return None;
        }
        i += 1;
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        let q = *b.get(i)?;
        if q != b'"' && q != b'\'' {
            return None;
        }
        let vs = i + 1;
        let ve = vs + b[vs..].iter().position(|&c| c == q)?;
        let v = &inner[vs..ve];
        if v.contains('<') {
            return None;
        }
        attrs.push((an, v));
        i = ve + 1;
    }
    Some((name, attrs))
}

/// `&...;` references expanded: numeric, the five predefined, internal entities; `None` for a bare `&`, a bad
/// number or an unknown name (unless `lenient`: a document with an external DTD, where libxml2 keeps the reference).
fn expand(s: &str, ents: &HashMap<String, String>, lenient: bool) -> Option<String> {
    if !s.contains('&') {
        return Some(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        let e = after.find(';')?;
        let name = &after[..e];
        if let Some(num) = name.strip_prefix('#') {
            let v = match num.strip_prefix('x') {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => num.parse::<u32>().ok()?,
            };
            let c = char::from_u32(v).filter(|c| is_xml_char(*c))?;
            out.push(c);
        } else {
            match name {
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "amp" => out.push('&'),
                "apos" => out.push('\''),
                "quot" => out.push('"'),
                _ => match ents.get(name) {
                    Some(v) => out.push_str(v),
                    None if lenient && !name.is_empty() => {}
                    None => return None,
                },
            }
        }
        rest = &after[e + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// (external DTD named, internal `<!ENTITY name "value">` declarations) of a DOCTYPE.
fn doctype(raw: &str) -> (bool, HashMap<String, String>) {
    let head = raw.split('[').next().unwrap_or(raw);
    let external = head.contains("PUBLIC") || head.contains("SYSTEM");
    let mut ents = HashMap::new();
    let mut rest = raw;
    while let Some(p) = rest.find("<!ENTITY") {
        rest = &rest[p + 8..];
        let t = rest.trim_start();
        if t.starts_with('%') {
            continue;
        }
        let n = t.find(|c: char| c.is_whitespace()).unwrap_or(t.len());
        let name = &t[..n];
        let v = t[n..].trim_start();
        if let Some(q) = v.chars().next().filter(|c| *c == '"' || *c == '\'') {
            if let Some(e) = v[1..].find(q) {
                ents.entry(name.to_string()).or_insert_with(|| v[1..1 + e].to_string());
            }
        }
    }
    (external, ents)
}

struct Open {
    local: String,
    scope_mark: usize,
    /// Index into `captures` when this element is a script or style.
    capture: Option<usize>,
}

/// LocOS `_xml_view`: the view of a well-formed XML document, else `None`.
pub fn xml_view(text: &str) -> Option<XmlView> {
    if !text.contains('<') || !text.chars().all(is_xml_char) {
        return None;
    }
    let mut rd = Reader::from_str(text);
    {
        let c = rd.config_mut();
        c.check_end_names = true;
        c.check_comments = true;
        c.expand_empty_elements = false;
        c.allow_dangling_amp = true; // references are checked by `expand`
    }
    let mut view = XmlView { sigs: Vec::new(), bodies: HashSet::new() };
    let mut stack: Vec<Open> = Vec::new();
    let mut scopes: Vec<(String, String)> = Vec::new();
    let mut captures: Vec<(String, String, bool)> = Vec::new(); // (name, text, open)
    let mut ents: HashMap<String, String> = HashMap::new();
    let mut lenient = false;
    let mut seen_root = false;
    let mut pos = 0usize;
    let mut line = 1usize;
    let mut line_pos = 0usize;
    loop {
        let ev = rd.read_event().ok()?;
        let end = rd.buffer_position() as usize;
        let start = pos;
        pos = end;
        let raw = text.get(start..end)?;
        match ev {
            Event::Eof => break,
            Event::Text(_) | Event::GeneralRef(_) => {
                if stack.is_empty() {
                    if !raw.bytes().all(is_ws) {
                        return None;
                    }
                    continue;
                }
                if raw.contains("]]>") {
                    return None;
                }
                let t = expand(raw, &ents, lenient)?;
                for c in captures.iter_mut().filter(|c| c.2) {
                    c.1.push_str(&t);
                }
            }
            Event::CData(_) => {
                if stack.is_empty() {
                    return None;
                }
                let inner = raw.get(9..raw.len().saturating_sub(3)).unwrap_or("");
                for c in captures.iter_mut().filter(|c| c.2) {
                    c.1.push_str(inner);
                }
            }
            Event::Start(_) | Event::Empty(_) => {
                let empty = matches!(ev, Event::Empty(_));
                if stack.is_empty() && seen_root {
                    return None;
                }
                seen_root = true;
                if stack.len() >= MAX_DEPTH {
                    return None;
                }
                let (qname, attrs) = tag_parts(raw)?;
                let mark = scopes.len();
                let mut plain: Vec<(&str, String)> = Vec::new();
                let mut names: HashSet<&str> = HashSet::new();
                for (an, v) in &attrs {
                    if !names.insert(an) {
                        return None;
                    }
                    let v = expand(v, &ents, lenient)?;
                    if *an == "xmlns" {
                        scopes.push((String::new(), v));
                    } else if let Some(p) = an.strip_prefix("xmlns:") {
                        if v.is_empty() {
                            return None;
                        }
                        scopes.push((p.to_string(), v));
                    } else {
                        plain.push((an, v));
                    }
                }
                let resolve = |prefix: &str| -> Option<String> {
                    if prefix == "xml" {
                        return Some(XML_NS.to_string());
                    }
                    scopes.iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| u.clone())
                };
                let split = |q: &str| -> (String, String) {
                    match q.split_once(':') {
                        Some((p, l)) => (p.to_string(), l.to_string()),
                        None => (String::new(), q.to_string()),
                    }
                };
                let (ep, el) = split(qname);
                let ens = if ep.is_empty() { resolve("").unwrap_or_default() } else { resolve(&ep)? };
                let tag = if ens.is_empty() { el.clone() } else { format!("{{{ens}}}{el}") };
                let local = el.to_lowercase();
                // (Clark key, local lower-case name, value)
                let mut keyed: Vec<(String, String, String)> = Vec::new();
                for (an, v) in plain {
                    let (ap, al) = split(an);
                    let key = if ap.is_empty() { al.clone() } else { format!("{{{}}}{al}", resolve(&ap)?) };
                    keyed.push((key, al.to_lowercase(), v));
                }
                let lowered: Attrs = keyed.iter().map(|(_, a, v)| (a.clone(), Some(v.clone()))).collect();
                let mut parts: Vec<String> = keyed
                    .iter()
                    .map(|(k, a, v)| if free_attr(&local, a, &lowered) { format!("{k}\u{0}\u{0}free") } else { format!("{k}\u{0}{v}") })
                    .collect();
                parts.sort();
                while let Some(off) = text[line_pos..start].find('\n') {
                    line += 1;
                    line_pos += off + 1;
                }
                view.sigs.push((format!("{tag}\u{2}{}", parts.join("\u{3}")), line));
                let is_body = local == "script" || local == "style";
                if empty {
                    scopes.truncate(mark);
                    if is_body {
                        view.bodies.insert((local, String::new()));
                    }
                } else {
                    let capture = is_body.then(|| {
                        captures.push((local.clone(), String::new(), true));
                        captures.len() - 1
                    });
                    stack.push(Open { local, scope_mark: mark, capture });
                }
            }
            Event::End(_) => {
                let open = stack.pop()?;
                scopes.truncate(open.scope_mark);
                if let Some(ci) = open.capture {
                    captures[ci].2 = false;
                    let body = std::mem::take(&mut captures[ci].1);
                    view.bodies.insert((open.local, body));
                }
            }
            Event::DocType(_) => {
                if seen_root {
                    return None;
                }
                let (ext, e) = doctype(raw);
                lenient = ext;
                ents = e;
            }
            Event::Comment(_) | Event::PI(_) | Event::Decl(_) => {}
        }
    }
    if !stack.is_empty() || !seen_root {
        return None;
    }
    Some(view)
}

/// LocOS `_check_xml(before, after)`: nothing to check unless both read as XML; then every element of `after`
/// must be one `before` has, and every script/style body too.
pub fn check_xml(before: &str, after: &str) -> Result<(), Unsafe> {
    let Some(xb) = xml_view(before) else { return Ok(()) };
    let Some(xa) = xml_view(after) else { return Ok(()) };
    let known: HashSet<&str> = xb.sigs.iter().map(|(s, _)| s.as_str()).collect();
    for (sig, line) in &xa.sigs {
        if !known.contains(sig.as_str()) {
            return Err(Unsafe { what: "an element read as XML".into(), line: *line, xml: true });
        }
    }
    for (name, body) in &xa.bodies {
        if !xb.bodies.contains(&(name.clone(), body.clone())) {
            return Err(Unsafe { what: format!("the content of a <{name}> element"), line: 1, xml: true });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_well_formed_xhtml_only() {
        assert!(xml_view("<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>a &amp; b</p></body></html>").is_some());
        assert!(xml_view("<p>a <br> b</p>").is_none());
        assert!(xml_view("<p>a &nbsp; b</p>").is_none());
        assert!(xml_view("<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\" \"x.dtd\"><p>a &nbsp; b</p>").is_some());
        assert!(xml_view("<p>a</p><p>b</p>").is_none());
        assert!(xml_view("<p a=b>x</p>").is_none());
    }

    #[test]
    fn refuses_a_new_element_and_a_new_script() {
        let before = "<div><p>x</p><script>var a = 1;</script></div>";
        assert!(check_xml(before, "<div><p>y</p><script>var a = 1;</script></div>").is_ok());
        assert!(check_xml(before, "<div><p>y<b>z</b></p><script>var a = 1;</script></div>").is_err());
        assert!(check_xml(before, "<div><p>y</p><script>var a = 2;</script></div>").is_err());
        assert!(check_xml(before, "<div><p title=\"z\">y</p><script>var a = 1;</script></div>").is_err());
        let titled = "<div><img alt=\"a\"/></div>";
        assert!(check_xml(titled, "<div><img alt=\"b\"/></div>").is_ok());
    }
}
