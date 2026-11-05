//! One pass over a TTML / DFXP text: checks it is well-formed (no DTD, no custom entities, balanced, namespaces
//! declared, XML characters only, depth and element limits) and records what the writer needs: the content span of
//! every caption `<p>`, the `<tt>` start tag, every `xml:lang` value and the namespace facts for a right-to-left
//! `tts:direction`.
//!
//! This replaces the LocOS pair "hardened lxml parse plus a light tokenizer that must agree with it": one scanner
//! that reads well-formedness and offsets together, so there is nothing to disagree.

use crate::Limits;
use bilingua_model::{Error, LimitError, LimitKind};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashSet;

/// TTML 1/2 (and IMSC1), then the two DFXP candidate-recommendation drafts older `.dfxp` files still carry.
pub const TTML_NS: [&str; 3] = ["http://www.w3.org/ns/ttml", "http://www.w3.org/2006/10/ttaf1", "http://www.w3.org/2006/04/ttaf1"];
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";

/// TT Style namespaces for each TTML namespace, preferred first. TTML1/2 and IMSC1 use `ns/ttml#styling`; the DFXP
/// drafts define `ttaf1#style`, and some tools wrote `ttaf1#styling`: both are recognised.
pub const STYLING_NS: [&[&str]; 3] = [
    &["http://www.w3.org/ns/ttml#styling"],
    &["http://www.w3.org/2006/10/ttaf1#style", "http://www.w3.org/2006/10/ttaf1#styling"],
    &["http://www.w3.org/2006/04/ttaf1#style", "http://www.w3.org/2006/04/ttaf1#styling"],
];

fn is_styling(uri: &str) -> bool {
    STYLING_NS.iter().any(|v| v.contains(&uri))
}

/// Most namespace declarations in scope at once (libxml2 has no such cap; no real file comes near it).
const MAX_NS_IN_SCOPE: usize = 4096;

pub fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "ttml".into(), msg: msg.into() }
}

fn limit_err(kind: LimitKind, msg: String) -> Error {
    LimitError::new(kind, msg).into()
}

/// The attributes of a caption `<p>` the editor shows (timing and look stay in the skeleton).
#[derive(Debug, Clone, Default)]
pub struct CapAttrs {
    pub begin: Option<String>,
    pub end: Option<String>,
    pub dur: Option<String>,
    pub xml_id: Option<String>,
    pub style: Option<String>,
    pub region: Option<String>,
}

/// One caption `<p>` (under `<body>` and TTML `<div>` elements only).
#[derive(Debug, Clone)]
pub struct Cap {
    /// End of the start tag = start of the content.
    pub content_start: usize,
    pub content_end: usize,
    /// End of the qualified name inside the start tag (where an attribute can be inserted).
    pub name_end: usize,
    pub qname: String,
    pub preserve: bool,
    pub attrs: CapAttrs,
    /// `direction` attributes in a TT Style namespace on the start tag: (value start, value end, raw value).
    pub dir_attrs: Vec<(usize, usize, String)>,
    /// A prefix in scope for this `<p>`'s TT Style namespace (innermost declaration first), if any.
    pub style_prefix: Option<String>,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub caps: Vec<Cap>,
    /// Start and end of the `<tt>` start tag.
    pub root_tag: (usize, usize),
    pub root_name_end: usize,
    pub root_ns: usize,
    /// `xml:lang` of `<tt>` (None when absent).
    pub root_lang: Option<String>,
    /// Offset of the first TTML `<body>` start tag.
    pub body_start: Option<usize>,
    /// Every `xml:lang` attribute value in a start tag: (value start, value end, unescaped value).
    pub lang_ranges: Vec<(usize, usize, String)>,
    /// Every namespace prefix declared anywhere.
    pub prefixes: HashSet<String>,
}

struct RawAttr<'a> {
    name: &'a str,
    vstart: usize,
    vraw: &'a str,
}

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Body,
    Div,
    Other,
    Foreign,
}

struct Frame {
    ns_len: usize,
    styling_len: usize,
    role: Role,
    preserve: bool,
    cap: Option<usize>,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn xml_char(c: char) -> bool {
    matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn valid_name(s: &str) -> bool {
    let mut it = s.chars();
    let Some(f) = it.next() else { return false };
    if !(f.is_ascii_alphabetic() || f == '_' || f == ':' || (f as u32) >= 0xC0) {
        return false;
    }
    it.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':') || (c as u32) >= 0xB7)
}

/// Every `&` starts a predefined entity or a character reference to an XML character.
fn check_refs(s: &str, what: &str) -> Result<(), Error> {
    let b = s.as_bytes();
    let mut i = 0;
    while let Some(p) = b[i..].iter().position(|c| *c == b'&') {
        let at = i + p;
        let Some(len) = b[at + 1..b.len().min(at + 14)].iter().position(|c| *c == b';') else {
            return Err(perr(format!("not well-formed XML: bad entity reference in {what}")));
        };
        let name = &s[at + 1..at + 1 + len];
        let ok = match name {
            "lt" | "gt" | "amp" | "quot" | "apos" => true,
            _ => name.strip_prefix('#').is_some_and(|n| {
                let v = match n.strip_prefix('x') {
                    Some(h) if !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit()) => u32::from_str_radix(h, 16).ok(),
                    None if !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()) => n.parse().ok(),
                    _ => None,
                };
                v.and_then(char::from_u32).is_some_and(xml_char)
            }),
        };
        if !ok {
            return Err(perr(format!("not well-formed XML: undefined entity &{name}; in {what}")));
        }
        i = at + len + 2;
    }
    Ok(())
}

/// Predefined entities and character references (already checked by `check_refs`). Anything else stays as is.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        let Some(semi) = after.find(';') else {
            out.push_str(&rest[p..]);
            return out;
        };
        let name = &after[..semi];
        match name {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let v = match name.strip_prefix("#x") {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => name.strip_prefix('#').and_then(|d| d.parse().ok()),
                };
                match v.and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[p..p + semi + 2]),
                }
            }
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    out
}

/// Lexes a start tag `text[s..e]` (`e` just past the `>`): qualified name and attributes, checked for syntax.
fn lex_tag(text: &str, s: usize, e: usize) -> Result<(&str, Vec<RawAttr<'_>>), Error> {
    let bad = || perr("not well-formed XML: malformed start tag");
    let b = text.as_bytes();
    let mut limit = e - 1; // the '>'
    if b[limit - 1] == b'/' {
        limit -= 1;
    }
    let mut i = s + 1;
    let ns = i;
    while i < limit && !is_ws(b[i]) {
        i += 1;
    }
    let qname = &text[ns..i];
    if !valid_name(qname) || qname.matches(':').count() > 1 {
        return Err(bad());
    }
    let mut attrs: Vec<RawAttr> = Vec::new();
    loop {
        let before = i;
        while i < limit && is_ws(b[i]) {
            i += 1;
        }
        if i >= limit {
            break;
        }
        if i == before {
            return Err(bad());
        }
        let an = i;
        while i < limit && !is_ws(b[i]) && b[i] != b'=' {
            i += 1;
        }
        let name = &text[an..i];
        if !valid_name(name) || name.matches(':').count() > 1 {
            return Err(bad());
        }
        while i < limit && is_ws(b[i]) {
            i += 1;
        }
        if i >= limit || b[i] != b'=' {
            return Err(bad());
        }
        i += 1;
        while i < limit && is_ws(b[i]) {
            i += 1;
        }
        if i >= limit || !matches!(b[i], b'"' | b'\'') {
            return Err(bad());
        }
        let quote = b[i];
        let vs = i + 1;
        let Some(len) = b[vs..limit].iter().position(|c| *c == quote) else { return Err(bad()) };
        let vraw = &text[vs..vs + len];
        if vraw.contains('<') {
            return Err(perr("not well-formed XML: '<' in an attribute value"));
        }
        check_refs(vraw, "an attribute value")?;
        i = vs + len + 1;
        attrs.push(RawAttr { name, vstart: vs, vraw });
    }
    if attrs.len() > 8 {
        let mut seen = HashSet::new();
        if !attrs.iter().all(|a| seen.insert(a.name)) {
            return Err(perr("not well-formed XML: duplicate attribute"));
        }
    } else {
        for (k, a) in attrs.iter().enumerate() {
            if attrs[..k].iter().any(|o| o.name == a.name) {
                return Err(perr("not well-formed XML: duplicate attribute"));
            }
        }
    }
    Ok((qname, attrs))
}

/// `xml:lang` value ranges inside one start-tag token (offsets in `tok`), for the tags of a rebuilt `<p>` content.
pub fn tag_lang_ranges(tok: &str) -> Vec<(usize, usize)> {
    match lex_tag(tok, 0, tok.len()) {
        Ok((_, attrs)) => attrs.iter().filter(|a| a.name == "xml:lang").map(|a| (a.vstart, a.vstart + a.vraw.len())).collect(),
        Err(_) => Vec::new(),
    }
}

fn lookup<'a>(ns: &'a [(String, String)], prefix: &str) -> Option<&'a str> {
    if prefix == "xml" {
        return Some(XML_NS);
    }
    ns.iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| u.as_str())
}

fn ttml_index(uri: &str) -> Option<usize> {
    TTML_NS.iter().position(|n| *n == uri)
}

pub fn scan(text: &str, l: &Limits) -> Result<Scan, Error> {
    if text.bytes().filter(|b| *b == b'<').count() > l.max_units.saturating_mul(40) {
        return Err(limit_err(
            LimitKind::XmlElements,
            format!("This TTML file has far more markup than {} units allow. Split it into smaller files.", l.max_units),
        ));
    }
    if let Some(c) = text.chars().find(|c| !xml_char(*c)) {
        return Err(perr(format!("not well-formed XML: character U+{:04X} is not allowed", c as u32)));
    }
    if text.starts_with('\u{feff}') {
        // the decoder already took the file's one BOM; a second one is not XML (and quick-xml would skip it and
        // report offsets that no longer match this text)
        return Err(perr("not well-formed XML: a second byte-order mark at the start"));
    }
    let mut reader = Reader::from_str(text);
    {
        let c = reader.config_mut();
        c.check_comments = true;
        c.check_end_names = true;
    }
    let mut out = Scan::default();
    let mut ns: Vec<(String, String)> = Vec::new();
    // number of bindings in `ns` whose URI is a TT Style namespace (so most files skip the per-caption lookup)
    let mut styling_len = 0usize;
    let mut stack: Vec<Frame> = Vec::new();
    let mut root_seen = false;
    let mut root_ok = false;
    loop {
        let before = reader.buffer_position() as usize;
        let ev = reader.read_event().map_err(|e| perr(format!("not well-formed XML: {e}")))?;
        let after = reader.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Decl(_) => {
                if before != 0 {
                    return Err(perr("not well-formed XML: the XML declaration is not at the start"));
                }
            }
            Event::DocType(_) => return Err(perr("TTML file declares a DOCTYPE / DTD; refused (entity/XXE vector).")),
            Event::PI(_) | Event::Comment(_) => {}
            Event::CData(_) => {
                if stack.is_empty() {
                    return Err(perr("not well-formed XML: text outside the root element"));
                }
            }
            Event::Text(t) => {
                let b: &[u8] = &t;
                if stack.is_empty() {
                    if !b.iter().all(|c| is_ws(*c)) {
                        return Err(perr("not well-formed XML: text outside the root element"));
                    }
                } else {
                    if b.windows(3).any(|w| w == b"]]>") {
                        return Err(perr("not well-formed XML: ']]>' in text"));
                    }
                    if b.contains(&b'&') {
                        check_refs(&String::from_utf8_lossy(b), "text")?;
                    }
                }
            }
            Event::GeneralRef(r) => {
                if stack.is_empty() {
                    return Err(perr("not well-formed XML: reference outside the root element"));
                }
                let name = r.decode().map_err(|e| perr(format!("not well-formed XML: {e}")))?;
                check_refs(&format!("&{name};"), "text")?;
            }
            Event::Start(_) | Event::Empty(_) => {
                let empty = matches!(ev, Event::Empty(_));
                if stack.is_empty() {
                    if root_seen {
                        return Err(perr("not well-formed XML: more than one root element"));
                    }
                    root_seen = true;
                }
                let (qname, raw_attrs) = lex_tag(text, before, after)?;
                let ns_len = ns.len();
                let styling_before = styling_len;
                for a in &raw_attrs {
                    let (prefix, uri) = if a.name == "xmlns" {
                        (String::new(), unescape(a.vraw))
                    } else if let Some(p) = a.name.strip_prefix("xmlns:") {
                        if p == "xmlns" || p == "xml" && a.vraw != XML_NS || p != "xml" && (a.vraw == XML_NS || a.vraw == XMLNS_NS) {
                            return Err(perr("not well-formed XML: reserved namespace prefix or URI"));
                        }
                        if a.vraw.is_empty() {
                            return Err(perr(format!("not well-formed XML: namespace prefix {p} bound to nothing")));
                        }
                        (p.to_string(), unescape(a.vraw))
                    } else {
                        continue;
                    };
                    if !prefix.is_empty() {
                        out.prefixes.insert(prefix.clone());
                        if is_styling(&uri) {
                            styling_len += 1;
                        }
                    }
                    ns.push((prefix, uri));
                    if ns.len() > MAX_NS_IN_SCOPE {
                        return Err(limit_err(LimitKind::XmlElements, "This TTML file declares far too many namespaces at once.".into()));
                    }
                }
                let (prefix, local) = match qname.split_once(':') {
                    Some((p, l)) => (p, l),
                    None => ("", qname),
                };
                let uri = match lookup(&ns, prefix) {
                    Some(u) => Some(u).filter(|u| !u.is_empty()),
                    None if prefix.is_empty() => None,
                    None => return Err(perr(format!("not well-formed XML: namespace prefix {prefix} is not defined"))),
                };
                let mut expanded: Vec<(&str, &str)> = Vec::new();
                for a in &raw_attrs {
                    if let Some((p, loc)) = a.name.split_once(':') {
                        if p == "xmlns" {
                            continue;
                        }
                        match lookup(&ns, p) {
                            None => return Err(perr(format!("not well-formed XML: namespace prefix {p} is not defined"))),
                            Some(u) => {
                                if expanded.contains(&(u, loc)) {
                                    return Err(perr("not well-formed XML: duplicate attribute"));
                                }
                                expanded.push((u, loc));
                            }
                        }
                    }
                }
                let ttml = uri.and_then(ttml_index);
                if !root_ok && stack.is_empty() {
                    match ttml {
                        Some(i) if local == "tt" => {
                            root_ok = true;
                            out.root_tag = (before, after);
                            out.root_name_end = before + 1 + qname.len();
                            out.root_ns = i;
                            out.root_lang = raw_attrs.iter().find(|a| a.name == "xml:lang").map(|a| unescape(a.vraw));
                        }
                        _ => {}
                    }
                }
                for a in raw_attrs.iter().filter(|a| a.name == "xml:lang") {
                    out.lang_ranges.push((a.vstart, a.vstart + a.vraw.len(), unescape(a.vraw)));
                }
                let role = match (ttml.is_some(), local) {
                    (false, _) => Role::Foreign,
                    (true, "body") => Role::Body,
                    (true, "div") => Role::Div,
                    _ => Role::Other,
                };
                if role == Role::Body && out.body_start.is_none() {
                    out.body_start = Some(before);
                }
                let space = raw_attrs.iter().find(|a| a.name == "xml:space").map(|a| bilingua_model::py_strip(&unescape(a.vraw)) == "preserve");
                let preserve = space.unwrap_or_else(|| stack.last().is_some_and(|f| f.preserve));
                let mut cap = None;
                if let (Some(nsi), "p") = (ttml, local) {
                    if is_caption(&stack) {
                        cap = Some(out.caps.len());
                        out.caps.push(make_cap(before, after, qname, nsi, preserve, &raw_attrs, &ns, styling_len));
                    }
                }
                if empty {
                    if let Some(ci) = cap {
                        out.caps[ci].content_end = after;
                    }
                    ns.truncate(ns_len);
                    styling_len = styling_before;
                } else {
                    stack.push(Frame { ns_len, styling_len: styling_before, role, preserve, cap });
                }
            }
            Event::End(_) => {
                let Some(f) = stack.pop() else { return Err(perr("not well-formed XML: unbalanced end tag")) };
                ns.truncate(f.ns_len);
                styling_len = f.styling_len;
                if let Some(ci) = f.cap {
                    out.caps[ci].content_end = before;
                }
            }
        }
        if out.caps.len() > l.max_units.saturating_mul(4).max(1000) {
            return Err(limit_err(LimitKind::Units, format!("This file has more than {} segments. Split it into smaller files.", l.max_units)));
        }
    }
    if !root_seen {
        return Err(perr("not well-formed XML: no root element"));
    }
    if !stack.is_empty() {
        return Err(perr("not well-formed XML: unclosed element"));
    }
    if !root_ok {
        return Err(perr("not a TTML/DFXP document: the root element must be <tt> in the TTML namespace (http://www.w3.org/ns/ttml)."));
    }
    Ok(out)
}

/// A caption `<p>` sits in `<body>`, under TTML `<div>` elements only (TTML1 7.1.3/7.1.4). A `<p>` inside
/// `<metadata>`, a foreign element or another `<p>` is not one: taking it would overlap the enclosing caption.
fn is_caption(stack: &[Frame]) -> bool {
    for f in stack.iter().rev() {
        match f.role {
            Role::Body => return true,
            Role::Div => {}
            Role::Other | Role::Foreign => return false,
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn make_cap(start: usize, end: usize, qname: &str, nsi: usize, preserve: bool, attrs: &[RawAttr], ns: &[(String, String)], styling_len: usize) -> Cap {
    let get = |n: &str| attrs.iter().find(|a| a.name == n).map(|a| unescape(a.vraw));
    let a = CapAttrs { begin: get("begin"), end: get("end"), dur: get("dur"), xml_id: get("xml:id"), style: get("style"), region: get("region") };
    let (mut dir_attrs, mut style_prefix) = (Vec::new(), None);
    if styling_len > 0 {
        // effective prefix bindings, innermost first (an inner declaration hides an outer one of the same prefix)
        let mut seen: HashSet<&str> = HashSet::new();
        let eff: Vec<(&str, &str)> = ns.iter().rev().filter(|(p, _)| !p.is_empty() && seen.insert(p.as_str())).map(|(p, u)| (p.as_str(), u.as_str())).collect();
        for at in attrs {
            if let Some((p, "direction")) = at.name.split_once(':') {
                if eff.iter().any(|(k, u)| *k == p && is_styling(u)) {
                    dir_attrs.push((start_of(at), start_of(at) + at.vraw.len(), at.vraw.to_string()));
                }
            }
        }
        style_prefix = STYLING_NS[nsi].iter().find_map(|w| eff.iter().find(|(_, u)| u == w).map(|(k, _)| k.to_string()));
    }
    Cap { content_start: end, content_end: end, name_end: start + 1 + qname.len(), qname: qname.to_string(), preserve, attrs: a, dir_attrs, style_prefix }
}

fn start_of(a: &RawAttr) -> usize {
    a.vstart
}
