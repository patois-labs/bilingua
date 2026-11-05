//! A small checked XML reader: one pass over a decoded text gives every element with the exact character spans of
//! its start tag, content and end tag, so the writer can splice only what it changes. It refuses what a strict
//! parser refuses (malformed markup, undefined entities, undeclared prefixes, a DOCTYPE, characters XML does not
//! allow) and bounds depth and element count through the B6 guard that runs before it.

use bilingua_model::{Error, LimitError, LimitKind};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashSet;

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

#[derive(Debug, Clone)]
pub struct Attr {
    pub qname: String,
    pub local: String,
    /// Namespace URI, empty for an unprefixed attribute.
    pub ns: String,
    /// Normalised and unescaped value.
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct El {
    pub qname: String,
    /// Local name, ASCII lower-cased (LocOS compares names that way).
    pub local: String,
    pub attrs: Vec<Attr>,
    pub parent: Option<usize>,
    /// Element children, in order.
    pub kids: Vec<usize>,
    /// Children of any kind (elements, comments, processing instructions): lxml's `len()`.
    pub nodes: usize,
    pub start: usize,
    pub start_end: usize,
    pub end_start: usize,
    pub end: usize,
    pub empty: bool,
    /// A `gs4tr` namespace is declared in scope.
    pub gs4tr: bool,
}

impl El {
    /// An unprefixed attribute, case sensitive (lxml `el.get(name)`).
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.ns.is_empty() && a.qname == name).map(|a| a.value.as_str())
    }
    /// An attribute by local name whose namespace contains `ns_part`.
    pub fn attr_in(&self, local: &str, ns_part: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.local == local && !a.ns.is_empty() && a.ns.contains(ns_part)).map(|a| a.value.as_str())
    }
    /// An attribute by local name and exact namespace URI.
    pub fn attr_ns(&self, local: &str, ns: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.local == local && a.ns == ns).map(|a| a.value.as_str())
    }
}

#[derive(Debug, Default)]
pub struct Tree {
    pub els: Vec<El>,
}

pub fn perr(fmt: &str, msg: impl Into<String>) -> Error {
    Error::Parse { format: fmt.to_string(), msg: msg.into() }
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
fn check_refs(fmt: &str, s: &str, what: &str) -> Result<(), Error> {
    let b = s.as_bytes();
    let mut i = 0;
    while let Some(p) = b[i..].iter().position(|c| *c == b'&') {
        let at = i + p;
        let Some(len) = b[at + 1..b.len().min(at + 14)].iter().position(|c| *c == b';') else {
            return Err(perr(fmt, format!("not well-formed XML: bad entity reference in {what}")));
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
            return Err(perr(fmt, format!("not well-formed XML: undefined entity &{name}; in {what}")));
        }
        i = at + len + 2;
    }
    Ok(())
}

/// Resolves predefined entities and character references (already checked by `check_refs`).
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

struct RawAttr<'a> {
    name: &'a str,
    vraw: &'a str,
}

/// Splits a start tag `text[s..e]` (`<name a="v" ...>` or `.../>`) into its name and attributes.
fn lex_tag<'a>(fmt: &str, text: &'a str, s: usize, e: usize) -> Result<(&'a str, Vec<RawAttr<'a>>), Error> {
    let bad = || perr(fmt, "not well-formed XML: malformed start tag");
    let b = text.as_bytes();
    let mut limit = e - 1; // the '>'
    if limit > s && b[limit - 1] == b'/' {
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
            return Err(perr(fmt, "not well-formed XML: '<' in an attribute value"));
        }
        check_refs(fmt, vraw, "an attribute value")?;
        i = vs + len + 1;
        attrs.push(RawAttr { name, vraw });
    }
    if attrs.len() > 8 {
        let mut seen = HashSet::new();
        if !attrs.iter().all(|a| seen.insert(a.name)) {
            return Err(perr(fmt, "not well-formed XML: duplicate attribute"));
        }
    } else {
        for (k, a) in attrs.iter().enumerate() {
            if attrs[..k].iter().any(|o| o.name == a.name) {
                return Err(perr(fmt, "not well-formed XML: duplicate attribute"));
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

/// The text of an attribute value as an XML parser hands it over: literal tab, newline and carriage return become
/// a space, then references resolve.
fn attr_value(vraw: &str) -> String {
    let norm: String;
    let v = if vraw.contains(['\t', '\r', '\n']) {
        norm = vraw.replace("\r\n", " ").chars().map(|c| if matches!(c, '\t' | '\r' | '\n') { ' ' } else { c }).collect();
        norm.as_str()
    } else {
        vraw
    };
    unescape(v)
}

/// Parses `text` completely. The tree is in document (start tag) order, so index order is `iter()` order.
pub fn parse(fmt: &str, text: &str) -> Result<Tree, Error> {
    let b = text.as_bytes();
    // XML 1.0 characters only
    if b.iter().any(|c| *c < 0x20 && !matches!(c, 9 | 10 | 13)) || b.windows(3).any(|w| w[0] == 0xEF && w[1] == 0xBF && (w[2] == 0xBE || w[2] == 0xBF)) {
        let c = text.chars().find(|c| !xml_char(*c)).unwrap_or('\u{0}');
        return Err(perr(fmt, format!("not well-formed XML: character U+{:04X} is not allowed", c as u32)));
    }
    let mut reader = Reader::from_str(text);
    {
        let c = reader.config_mut();
        c.check_comments = true;
        c.check_end_names = true;
    }
    let mut tree = Tree { els: Vec::new() };
    let mut ns: Vec<(String, String)> = Vec::new();
    // open elements: (element index, ns length before it, gs4tr in scope before it)
    let mut stack: Vec<(usize, usize, bool)> = Vec::new();
    let mut gs4tr_scope = false;
    let mut root_seen = false;
    loop {
        let before = reader.buffer_position() as usize;
        let ev = reader.read_event().map_err(|e| perr(fmt, format!("not well-formed XML: {e}")))?;
        let after = reader.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Decl(_) => {
                if before != 0 {
                    return Err(perr(fmt, "not well-formed XML: the XML declaration is not at the start"));
                }
            }
            Event::DocType(_) => return Err(perr(fmt, "This file declares a DOCTYPE / DTD; refused (it is never needed).")),
            Event::PI(_) | Event::Comment(_) => {
                if let Some(&(top, _, _)) = stack.last() {
                    tree.els[top].nodes += 1;
                }
            }
            Event::CData(_) => {
                if stack.is_empty() {
                    return Err(perr(fmt, "not well-formed XML: text outside the root element"));
                }
            }
            Event::Text(t) => {
                let tb: &[u8] = &t;
                if stack.is_empty() {
                    if !tb.iter().all(|c| is_ws(*c)) {
                        return Err(perr(fmt, "not well-formed XML: text outside the root element"));
                    }
                } else {
                    if tb.windows(3).any(|w| w == b"]]>") {
                        return Err(perr(fmt, "not well-formed XML: ']]>' in text"));
                    }
                    if tb.contains(&b'&') {
                        check_refs(fmt, &String::from_utf8_lossy(tb), "text")?;
                    }
                }
            }
            Event::GeneralRef(r) => {
                if stack.is_empty() {
                    return Err(perr(fmt, "not well-formed XML: reference outside the root element"));
                }
                let name = r.decode().map_err(|e| perr(fmt, format!("not well-formed XML: {e}")))?;
                check_refs(fmt, &format!("&{name};"), "text")?;
            }
            Event::Start(_) | Event::Empty(_) => {
                let empty = matches!(ev, Event::Empty(_));
                if stack.is_empty() {
                    if root_seen {
                        return Err(perr(fmt, "not well-formed XML: more than one root element"));
                    }
                    root_seen = true;
                }
                if stack.len() >= 1024 {
                    // the recursive walkers below stay on a bounded stack whatever limit the caller set
                    return Err(LimitError::new(LimitKind::XmlDepth, "This file nests XML elements more than 1024 deep. It was not processed.").into());
                }
                let (qname, raw_attrs) = lex_tag(fmt, text, before, after)?;
                let ns_len = ns.len();
                let scope_before = gs4tr_scope;
                for a in &raw_attrs {
                    if a.name == "xmlns" || a.name.starts_with("xmlns:") {
                        let uri = unescape(a.vraw);
                        if uri.contains("gs4tr") {
                            gs4tr_scope = true;
                        }
                        ns.push((a.name.strip_prefix("xmlns:").unwrap_or("").to_string(), uri));
                    }
                }
                let prefix = qname.split_once(':').map_or("", |(p, _)| p);
                if !prefix.is_empty() && lookup(&ns, prefix).is_none() {
                    return Err(perr(fmt, format!("not well-formed XML: namespace prefix {prefix} is not defined")));
                }
                let mut attrs = Vec::with_capacity(raw_attrs.len());
                for a in &raw_attrs {
                    if a.name == "xmlns" || a.name.starts_with("xmlns:") {
                        continue;
                    }
                    let (p, local) = a.name.split_once(':').map_or(("", a.name), |(p, l)| (p, l));
                    let uri = if p.is_empty() {
                        String::new()
                    } else {
                        match lookup(&ns, p) {
                            Some(u) => u.to_string(),
                            None => return Err(perr(fmt, format!("not well-formed XML: namespace prefix {p} is not defined"))),
                        }
                    };
                    attrs.push(Attr { qname: a.name.to_string(), local: local.to_string(), ns: uri, value: attr_value(a.vraw) });
                }
                let local = qname.rsplit(':').next().unwrap_or(qname).to_ascii_lowercase();
                let idx = tree.els.len();
                let parent = stack.last().map(|s| s.0);
                tree.els.push(El {
                    qname: qname.to_string(),
                    local,
                    attrs,
                    parent,
                    kids: Vec::new(),
                    nodes: 0,
                    start: before,
                    start_end: after,
                    end_start: after,
                    end: after,
                    empty,
                    gs4tr: gs4tr_scope,
                });
                if let Some(p) = parent {
                    tree.els[p].kids.push(idx);
                    tree.els[p].nodes += 1;
                }
                if empty {
                    ns.truncate(ns_len);
                    gs4tr_scope = scope_before;
                } else {
                    stack.push((idx, ns_len, scope_before));
                }
            }
            Event::End(_) => {
                let Some((idx, ns_len, scope)) = stack.pop() else { return Err(perr(fmt, "not well-formed XML: unbalanced end tag")) };
                ns.truncate(ns_len);
                gs4tr_scope = scope;
                tree.els[idx].end_start = before;
                tree.els[idx].end = after;
            }
        }
    }
    if !root_seen {
        return Err(perr(fmt, "not well-formed XML: no root element"));
    }
    if !stack.is_empty() {
        return Err(perr(fmt, "not well-formed XML: unclosed element"));
    }
    Ok(tree)
}

/// One piece of an element's direct content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Text(String),
    El(usize),
}

/// The character data of a span: newlines normalised, CDATA unwrapped, comments and processing instructions
/// dropped, references resolved.
pub fn chars_of(raw: &str) -> String {
    if !raw.contains(['<', '&', '\r']) {
        return raw.to_string();
    }
    let norm;
    let raw = if raw.contains('\r') {
        norm = raw.replace("\r\n", "\n").replace('\r', "\n");
        norm.as_str()
    } else {
        raw
    };
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(p) = rest.find('<') {
        out.push_str(&unescape(&rest[..p]));
        let r = &rest[p..];
        if let Some(body) = r.strip_prefix("<![CDATA[") {
            match body.find("]]>") {
                Some(e) => {
                    out.push_str(&body[..e]);
                    rest = &body[e + 3..];
                }
                None => return out,
            }
        } else if let Some(body) = r.strip_prefix("<!--") {
            match body.find("-->") {
                Some(e) => rest = &body[e + 3..],
                None => return out,
            }
        } else if let Some(body) = r.strip_prefix("<?") {
            match body.find("?>") {
                Some(e) => rest = &body[e + 2..],
                None => return out,
            }
        } else {
            // not markup the tree knows: keep it as text
            out.push('<');
            rest = &r[1..];
        }
    }
    out.push_str(&unescape(rest));
    out
}

impl Tree {
    /// The direct content of element `i`: text pieces and child elements in order.
    pub fn content(&self, text: &str, i: usize) -> Vec<Node> {
        let e = &self.els[i];
        let mut out = Vec::with_capacity(e.kids.len() * 2 + 1);
        if e.empty {
            return out;
        }
        let mut cur = e.start_end;
        for &k in &e.kids {
            let c = &self.els[k];
            if c.start > cur {
                let t = chars_of(&text[cur..c.start]);
                if !t.is_empty() {
                    out.push(Node::Text(t));
                }
            }
            out.push(Node::El(k));
            cur = c.end;
        }
        if e.end_start > cur {
            let t = chars_of(&text[cur..e.end_start]);
            if !t.is_empty() {
                out.push(Node::Text(t));
            }
        }
        out
    }

    /// All character data below element `i` (lxml `itertext`).
    pub fn text_of(&self, text: &str, i: usize) -> String {
        let mut s = String::new();
        self.collect_text(text, i, &mut s);
        s
    }

    fn collect_text(&self, text: &str, i: usize, out: &mut String) {
        for n in self.content(text, i) {
            match n {
                Node::Text(t) => out.push_str(&t),
                Node::El(k) => self.collect_text(text, k, out),
            }
        }
    }

    /// Number of elements below `i` (not counting it): they follow it in document order inside its span.
    pub fn descendants(&self, i: usize) -> usize {
        let end = self.els[i].end;
        let mut n = 0;
        while let Some(e) = self.els.get(i + 1 + n) {
            if e.start >= end {
                break;
            }
            n += 1;
        }
        n
    }

    /// Next element sibling of `i`.
    pub fn next_sibling(&self, i: usize) -> Option<usize> {
        let p = self.els[i].parent?;
        let kids = &self.els[p].kids;
        let at = kids.iter().position(|k| *k == i)?;
        kids.get(at + 1).copied()
    }

    /// First element child of `i` with the given (lower-case) local name.
    pub fn kid(&self, i: usize, local: &str) -> Option<usize> {
        self.els[i].kids.iter().copied().find(|k| self.els[*k].local == local)
    }
}

/// `str.strip()` as Python does it (Unicode white space plus the four information separators).
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

/// A start tag found by [`start_tags`].
pub struct StartTag {
    pub start: usize,
    pub end: usize,
    pub qname: String,
}

/// Every start tag of `text` in order (comments, CDATA, processing instructions and declarations skipped).
pub fn start_tags(text: &str) -> Vec<StartTag> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = b[i..].iter().position(|c| *c == b'<') {
        i += p;
        let rest = &b[i..];
        let skip_to = |pat: &[u8], from: usize| -> Option<usize> { b[from..].windows(pat.len()).position(|w| w == pat).map(|q| from + q + pat.len()) };
        if rest.starts_with(b"<!--") {
            match skip_to(b"-->", i + 4) {
                Some(e) => i = e,
                None => break,
            }
        } else if rest.starts_with(b"<![CDATA[") {
            match skip_to(b"]]>", i + 9) {
                Some(e) => i = e,
                None => break,
            }
        } else if rest.starts_with(b"<?") {
            match skip_to(b"?>", i + 2) {
                Some(e) => i = e,
                None => break,
            }
        } else if rest.starts_with(b"</") || rest.starts_with(b"<!") {
            match b[i..].iter().position(|c| *c == b'>') {
                Some(e) => i += e + 1,
                None => break,
            }
        } else {
            let (mut j, mut q) = (i + 1, 0u8);
            let mut end = None;
            while j < b.len() {
                let c = b[j];
                if q != 0 {
                    if c == q {
                        q = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    q = c;
                } else if c == b'>' {
                    end = Some(j + 1);
                    break;
                }
                j += 1;
            }
            let Some(e) = end else { break };
            let name_end = b[i + 1..e].iter().position(|c| is_ws(*c) || *c == b'>' || *c == b'/').map_or(e, |q| i + 1 + q);
            out.push(StartTag { start: i, end: e, qname: text[i + 1..name_end].to_string() });
            i = e;
        }
    }
    out
}

/// The unescaped value of attribute `name` in a start tag's text, if present.
pub fn tag_attr(tag: &str, name: &str) -> Option<String> {
    let (s, e) = (0, tag.len());
    let (_, attrs) = lex_tag("xml", tag, s, e).ok()?;
    attrs.iter().find(|a| a.name == name).map(|a| attr_value(a.vraw))
}

fn esc_attr(s: &str, quote: u8) -> String {
    let s = s.replace('&', "&amp;").replace('<', "&lt;").replace("]]>", "]]&gt;");
    if quote == b'"' {
        s.replace('"', "&quot;")
    } else {
        s.replace('\'', "&apos;")
    }
}

/// Sets one attribute in a start tag's text, keeping its quoting and every other character.
pub fn with_attr(tag: &str, qname: &str, value: &str) -> String {
    let b = tag.as_bytes();
    let limit = if tag.ends_with("/>") { tag.len() - 2 } else { tag.len() - 1 };
    // walk the attributes like lex_tag does, remembering the value span of `qname`
    let mut i = 1;
    while i < limit && !is_ws(b[i]) {
        i += 1;
    }
    loop {
        while i < limit && is_ws(b[i]) {
            i += 1;
        }
        if i >= limit {
            break;
        }
        let an = i;
        while i < limit && !is_ws(b[i]) && b[i] != b'=' {
            i += 1;
        }
        let name = &tag[an..i];
        while i < limit && is_ws(b[i]) {
            i += 1;
        }
        if i >= limit || b[i] != b'=' {
            break;
        }
        i += 1;
        while i < limit && is_ws(b[i]) {
            i += 1;
        }
        if i >= limit || !matches!(b[i], b'"' | b'\'') {
            break;
        }
        let quote = b[i];
        let vs = i + 1;
        let Some(len) = b[vs..limit].iter().position(|c| *c == quote) else { break };
        if name == qname {
            return format!("{}{}{}", &tag[..vs], esc_attr(value, quote), &tag[vs + len..]);
        }
        i = vs + len + 1;
    }
    let mut close = limit;
    while close > 0 && is_ws(b[close - 1]) {
        close -= 1;
    }
    format!("{} {qname}=\"{}\"{}", &tag[..close], esc_attr(value, b'"'), &tag[close..])
}
