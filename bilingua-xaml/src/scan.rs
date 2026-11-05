//! One pass over a well-formed XAML text: checks it (no DTD, no external or custom entities, balanced, namespaces
//! declared, XML characters only, depth and element limits) and finds the exact character spans of the units, so
//! the writer can splice only those.
//!
//! Resource strings (`sys:String` / `x:String`) come first, then (opt-in) view attributes, each in document order:
//! switching the option never moves a resource string's unit index.

use crate::Limits;
use bilingua_model::{Error, LimitError, LimitKind};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashSet;

const XAML_NS: [&str; 2] = ["http://schemas.microsoft.com/winfx/2006/xaml", "http://schemas.microsoft.com/winfx/2009/xaml"];
pub const VIEW_ATTRIBUTES: [&str; 5] = ["Content", "Text", "ToolTip", "Header", "Title"];
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

pub fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "xaml".into(), msg: msg.into() }
}

fn limit_err(kind: LimitKind, msg: String) -> Error {
    LimitError::new(kind, msg).into()
}

/// One span of text the writer may replace.
#[derive(Debug, Clone)]
pub struct Item {
    pub start: usize,
    pub end: usize,
    pub attr: bool,
    pub quote: u8,
    pub crlf: bool,
    /// An attribute written with the `{}` escape prefix (the prefix is outside the span).
    pub escaped: bool,
    /// False for a value the filter leaves alone (markup extension, no letters): only listed with `include_locked`.
    pub translatable: bool,
    pub value: String,
    pub key: Option<String>,
    /// `string` for a resource, the lower-case attribute name for a view attribute.
    pub kind: String,
    /// Local element name.
    pub element: String,
    pub note: Option<String>,
}

struct RawAttr<'a> {
    name: &'a str,
    vstart: usize,
    vraw: &'a str,
    quote: u8,
}

struct Frame {
    ns_len: usize,
    is_string: bool,
    content_start: usize,
    mixed: bool,
    key: Option<String>,
    element: String,
    note: Option<String>,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn xml_char(c: char) -> bool {
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

/// Entities already checked by `check_refs`.
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

fn is_word(c: char) -> bool {
    bilingua_model::py_is_word_char(c)
}

/// `{Name ...}` or `{prefix:Name ...}`, Name starting with a letter. `{0}` and `{}`-escaped text are not.
pub fn markup_ext(v: &str) -> bool {
    let mut it = v.trim_start().chars().peekable();
    if it.next() != Some('{') {
        return false;
    }
    while it.peek().is_some_and(|c| bilingua_model::py_isspace_char(*c)) {
        it.next();
    }
    let name = |it: &mut std::iter::Peekable<std::str::Chars>| -> bool {
        if !it.peek().is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') {
            return false;
        }
        while it.peek().is_some_and(|c| is_word(*c) || *c == '.') {
            it.next();
        }
        true
    };
    if !name(&mut it) {
        return false;
    }
    if it.peek() == Some(&':') {
        let mut probe = it.clone();
        probe.next();
        if name(&mut probe) {
            it = probe;
        }
    }
    it.peek().is_none_or(|c| bilingua_model::py_isspace_char(*c) || *c == ',' || *c == '}')
}

fn translatable(v: &str) -> bool {
    bilingua_model::py_has_alnum(v) && !markup_ext(v)
}

fn lex_tag<'a>(text: &'a str, s: usize, e: usize) -> Result<(&'a str, Vec<RawAttr<'a>>), Error> {
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
        attrs.push(RawAttr { name, vstart: vs, vraw, quote });
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

fn lookup<'a>(ns: &'a [(String, String)], prefix: &str) -> Option<&'a str> {
    if prefix == "xml" {
        return Some(XML_NS);
    }
    ns.iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| u.as_str())
}

fn is_string_ns(uri: &str) -> bool {
    if XAML_NS.contains(&uri) {
        return true;
    }
    let u: String = uri.chars().filter(|c| *c != ' ').collect();
    u == "clr-namespace:System" || u == "using:System" || u.starts_with("clr-namespace:System;")
}

fn trim_xml(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\r', '\n'])
}

pub fn walk(text: &str, with_attrs: bool, l: &Limits) -> Result<Vec<Item>, Error> {
    if text.bytes().filter(|b| *b == b'<').count() > l.max_units.saturating_mul(40) {
        return Err(limit_err(LimitKind::XmlElements, format!("This file has far more elements than {} units allow. Split it into smaller files.", l.max_units)));
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
    let mut ns: Vec<(String, String)> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut resources: Vec<Item> = Vec::new();
    let mut attrs_out: Vec<Item> = Vec::new();
    let mut root_seen = false;
    let mut note: Option<String> = None;
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
            Event::DocType(_) => return Err(perr("This XAML file declares a DOCTYPE / DTD; refused (XAML never needs one).")),
            Event::PI(_) => {
                if let Some(top) = stack.last_mut() {
                    top.mixed = true;
                }
            }
            Event::Comment(c) => {
                if let Some(top) = stack.last_mut() {
                    top.mixed = true;
                    note = std::str::from_utf8(&c).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                }
            }
            Event::CData(_) => {
                let Some(top) = stack.last_mut() else { return Err(perr("not well-formed XML: text outside the root element")) };
                top.mixed = true;
                note = None;
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
                    if !b.iter().all(|c| is_ws(*c)) {
                        note = None;
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
                note = None;
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
                for a in &raw_attrs {
                    if a.name == "xmlns" {
                        ns.push((String::new(), unescape(a.vraw)));
                    } else if let Some(p) = a.name.strip_prefix("xmlns:") {
                        ns.push((p.to_string(), unescape(a.vraw)));
                    }
                }
                let (prefix, local) = match qname.split_once(':') {
                    Some((p, l)) => (p, l),
                    None => ("", qname),
                };
                let uri = match lookup(&ns, prefix) {
                    Some(u) => Some(u.to_string()).filter(|u| !u.is_empty()),
                    None if prefix.is_empty() => None,
                    None => return Err(perr(format!("not well-formed XML: namespace prefix {prefix} is not defined"))),
                };
                for a in &raw_attrs {
                    if let Some((p, _)) = a.name.split_once(':') {
                        if p != "xmlns" && lookup(&ns, p).is_none() {
                            return Err(perr(format!("not well-formed XML: namespace prefix {p} is not defined")));
                        }
                    }
                }
                let key = raw_attrs
                    .iter()
                    .find(|a| {
                        let (p, loc) = a.name.split_once(':').unwrap_or(("", a.name));
                        matches!(loc, "Key" | "Name" | "Uid") && (p.is_empty() && loc == "Name" || lookup(&ns, p).is_some_and(|u| XAML_NS.contains(&u)))
                    })
                    .map(|a| unescape(a.vraw));
                let is_string = local == "String" && uri.as_deref().is_some_and(is_string_ns);
                if with_attrs && !local.contains('.') {
                    for a in &raw_attrs {
                        if !VIEW_ATTRIBUTES.contains(&a.name) {
                            continue;
                        }
                        let norm: String = a.vraw.replace("\r\n", " ").chars().map(|c| if matches!(c, '\t' | '\r' | '\n') { ' ' } else { c }).collect();
                        let mut value = unescape(&norm);
                        let (mut off, mut tr) = (0usize, true);
                        if value.starts_with("{}") {
                            if !a.vraw.starts_with("{}") {
                                continue;
                            }
                            off = 2;
                            value.drain(..2);
                        } else if markup_ext(&value) || value.trim_start().starts_with('{') {
                            tr = false;
                        }
                        if !bilingua_model::py_has_alpha(&value) {
                            tr = false;
                        }
                        if !tr && trim_xml(&value).is_empty() {
                            continue;
                        }
                        attrs_out.push(Item {
                            start: a.vstart + off,
                            end: a.vstart + a.vraw.len(),
                            attr: true,
                            quote: a.quote,
                            crlf: false,
                            escaped: off > 0,
                            translatable: tr,
                            value,
                            key: key.clone(),
                            kind: a.name.to_ascii_lowercase(),
                            element: local.to_string(),
                            note: None,
                        });
                    }
                }
                if let Some(top) = stack.last_mut() {
                    top.mixed = true;
                }
                if empty {
                    ns.truncate(ns_len);
                } else {
                    stack.push(Frame { ns_len, is_string, content_start: after, mixed: false, key, element: local.to_string(), note: note.take() });
                }
                note = None;
            }
            Event::End(_) => {
                let Some(f) = stack.pop() else { return Err(perr("not well-formed XML: unbalanced end tag")) };
                ns.truncate(f.ns_len);
                if f.is_string && !f.mixed {
                    let raw = &text[f.content_start..before];
                    let core = trim_xml(raw);
                    if !core.is_empty() {
                        let lead = raw.len() - raw.trim_start_matches([' ', '\t', '\r', '\n']).len();
                        let value = unescape(&core.replace("\r\n", "\n").replace('\r', "\n"));
                        let start = f.content_start + lead;
                        resources.push(Item {
                            start,
                            end: start + core.len(),
                            attr: false,
                            quote: 0,
                            crlf: raw.contains("\r\n"),
                            escaped: false,
                            translatable: translatable(&value),
                            value,
                            key: f.key,
                            kind: "string".into(),
                            element: f.element,
                            note: f.note,
                        });
                    }
                }
                note = None;
            }
        }
        if resources.len() + attrs_out.len() > l.max_units.saturating_mul(4).max(1000) {
            return Err(limit_err(LimitKind::Units, format!("This file has more than {} segments. Split it into smaller files.", l.max_units)));
        }
    }
    if !root_seen {
        return Err(perr("not well-formed XML: no root element"));
    }
    if !stack.is_empty() {
        return Err(perr("not well-formed XML: unclosed element"));
    }
    resources.extend(attrs_out);
    Ok(resources)
}
