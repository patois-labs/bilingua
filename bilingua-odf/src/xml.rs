//! A lossless, zero-copy XML tree for the parts of an OpenDocument package.
//!
//! Every node is a byte range of the source text, so writing an unchanged tree back gives the input exactly. The
//! parser checks well-formedness the way libxml2 does for the cases that matter (matching tags, quoted attributes,
//! no duplicate attributes, only the five predefined entities and valid character references, bound namespace
//! prefixes, one root element, no illegal control characters), refuses any DOCTYPE or entity declaration, and
//! bounds depth and element count with the B6 limits. It never recurses on the input, so a deeply nested file
//! cannot overflow the stack.

use bilingua_model::{Error, LimitError, LimitKind, Limits};
use std::borrow::Cow;
use std::collections::HashMap;

pub const NIL: u32 = u32::MAX;

pub const NS_NONE: u16 = 0;
pub const NS_TEXT: u16 = 1;
pub const NS_TABLE: u16 = 2;
pub const NS_OFFICE: u16 = 3;
pub const NS_STYLE: u16 = 4;
pub const NS_FO: u16 = 5;
pub const NS_META: u16 = 6;
pub const NS_DC: u16 = 7;
pub const NS_XML: u16 = 8;
const KNOWN: [&str; 9] = [
    "",
    "urn:oasis:names:tc:opendocument:xmlns:text:1.0",
    "urn:oasis:names:tc:opendocument:xmlns:table:1.0",
    "urn:oasis:names:tc:opendocument:xmlns:office:1.0",
    "urn:oasis:names:tc:opendocument:xmlns:style:1.0",
    "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0",
    "urn:oasis:names:tc:opendocument:xmlns:meta:1.0",
    "http://purl.org/dc/elements/1.1/",
    "http://www.w3.org/XML/1998/namespace",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Elem,
    Text,
    Cdata,
    Comment,
    Pi,
}

#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub kind: Kind,
    /// Namespace id of an element (0 = none).
    pub ns: u16,
    /// Length of the qualified name; the name starts one byte after `s0`.
    pub qlen: u16,
    /// Offset of the local name inside the qualified name.
    pub loff: u16,
    pub parent: u32,
    pub first: u32,
    pub next: u32,
    /// Element: start tag. Other nodes: the whole node.
    pub s0: u32,
    pub s1: u32,
    /// Element: end tag (equal to `s1,s1` for a self-closing element).
    pub e0: u32,
    pub e1: u32,
}

pub fn invalid(detail: impl AsRef<str>) -> Error {
    Error::Parse {
        format: "odf".into(),
        msg: format!("this OpenDocument part has invalid XML ({}). Save the file again in LibreOffice.", detail.as_ref()),
    }
}

fn limit(kind: LimitKind, msg: String) -> Error {
    Error::Limit(LimitError::new(kind, msg))
}

pub struct Tree {
    pub src: String,
    pub nodes: Vec<Node>,
    extra_ns: Vec<String>,
    root_decls: HashMap<String, u16>,
    decls: HashMap<u32, Vec<(String, u16)>>,
}

/// One attribute of a start tag, as positions inside the tag text.
#[derive(Clone, Copy, Debug)]
pub struct AttrPos {
    pub name_start: usize,
    pub name_end: usize,
    /// First byte of the value (after the opening quote) and the byte of the closing quote.
    pub val_start: usize,
    pub val_end: usize,
    pub quote: u8,
}

/// Attribute positions in a start tag text (already known to be well formed).
pub fn scan_attrs(tag: &str) -> Vec<AttrPos> {
    let b = tag.as_bytes();
    let mut i = 1;
    let mut out = Vec::new();
    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] == b'>' || b[i] == b'/' {
            break;
        }
        let ns = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() && b[i] != b'>' {
            i += 1;
        }
        let ne = i;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            break;
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
            break;
        }
        let q = b[i];
        let vs = i + 1;
        let Some(len) = b[vs..].iter().position(|c| *c == q) else { break };
        out.push(AttrPos { name_start: ns, name_end: ne, val_start: vs, val_end: vs + len, quote: q });
        i = vs + len + 1;
    }
    out
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':' || c >= 0x80
}
fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'-' | b'.') || c >= 0x80
}

fn valid_qname(n: &[u8]) -> bool {
    if n.is_empty() || n.len() > 1000 || !is_name_start(n[0]) || !n.iter().all(|c| is_name_char(*c)) {
        return false;
    }
    let colons = n.iter().filter(|c| **c == b':').count();
    colons <= 1 && n[0] != b':' && n[n.len() - 1] != b':'
}

fn xml_char(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// True when every `&` in `s` starts a predefined entity or a valid character reference.
fn refs_ok(s: &str) -> bool {
    let b = s.as_bytes();
    let mut from = 0;
    while let Some(p) = s[from..].find('&') {
        let at = from + p + 1;
        let Some(semi) = b[at..].iter().take(12).position(|c| *c == b';') else { return false };
        let name = &s[at..at + semi];
        let ok = match name {
            "amp" | "lt" | "gt" | "quot" | "apos" => true,
            _ => match name.strip_prefix("#x") {
                Some(h) => !h.is_empty() && u32::from_str_radix(h, 16).is_ok_and(xml_char),
                None => name.strip_prefix('#').is_some_and(|d| !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit()) && d.parse::<u32>().is_ok_and(xml_char)),
            },
        };
        if !ok {
            return false;
        }
        from = at + semi + 1;
    }
    true
}

/// Decodes the predefined entities and character references of a text (CDATA wrappers removed).
pub fn unescape(s: &str) -> Cow<'_, str> {
    if let Some(inner) = s.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>")) {
        return Cow::Borrowed(inner);
    }
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        let Some(semi) = after.find(';').filter(|k| *k <= 10) else {
            out.push('&');
            rest = after;
            continue;
        };
        let ent = &after[..semi];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse::<u32>().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Escapes text for element content as libxml2 does: `&`, `<`, `>` and CR.
pub fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

/// Escapes a value for an attribute written in `quote`.
pub fn escape_attr(s: &str, quote: u8) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' if quote == b'"' => out.push_str("&quot;"),
            '\'' if quote == b'\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
    out
}

impl Tree {
    pub fn parse(src: String, l: &Limits) -> Result<Tree, Error> {
        if src.len() >= u32::MAX as usize - 1 {
            return Err(limit(LimitKind::InputBytes, "This file has an XML part that is too large. It was not processed.".into()));
        }
        // Characters XML 1.0 cannot hold.
        if src.bytes().any(|c| c < 0x20 && !matches!(c, 9 | 10 | 13)) || src.contains(['\u{FFFE}', '\u{FFFF}']) {
            return Err(invalid("a character that XML does not allow"));
        }
        let mut t = Tree { src, nodes: Vec::new(), extra_ns: Vec::new(), root_decls: HashMap::new(), decls: HashMap::new() };
        t.run(l)?;
        Ok(t)
    }

    fn ns_id(&mut self, uri: &str) -> u16 {
        if let Some(i) = KNOWN.iter().position(|k| *k == uri) {
            return i as u16;
        }
        if let Some(i) = self.extra_ns.iter().position(|k| k == uri) {
            return (KNOWN.len() + i) as u16;
        }
        self.extra_ns.push(uri.to_string());
        (KNOWN.len() + self.extra_ns.len() - 1) as u16
    }

    pub fn ns_uri(&self, id: u16) -> &str {
        let i = id as usize;
        if i < KNOWN.len() {
            KNOWN[i]
        } else {
            self.extra_ns.get(i - KNOWN.len()).map(String::as_str).unwrap_or("")
        }
    }

    /// The namespace bound to `prefix` in the scope of `node` ("" = the default namespace).
    fn lookup(&self, mut node: u32, prefix: &str) -> Option<u16> {
        if prefix == "xml" {
            return Some(NS_XML);
        }
        if !self.decls.is_empty() {
            while node != NIL {
                if let Some(d) = self.decls.get(&node) {
                    if let Some((_, id)) = d.iter().rev().find(|(p, _)| p == prefix) {
                        return Some(*id);
                    }
                }
                node = self.nodes[node as usize].parent;
            }
        }
        self.root_decls.get(prefix).copied()
    }

    fn run(&mut self, l: &Limits) -> Result<(), Error> {
        let n = self.src.len();
        let mut pos = 0usize;
        let mut stack: Vec<u32> = Vec::new();
        let mut lastc: Vec<u32> = vec![NIL];
        let mut root_seen = false;
        let mut elements = 0usize;
        let mut attrs: Vec<(u32, u32, u32, u32)> = Vec::new();
        let mut own: Vec<(String, u16)> = Vec::new();
        // Move the source out so `self` can be borrowed mutably while the text is read.
        let src = std::mem::take(&mut self.src);
        let b = src.as_bytes();
        let res = (|| -> Result<(), Error> {
            while pos < n {
                if b[pos] != b'<' {
                    let end = src[pos..].find('<').map_or(n, |k| pos + k);
                    let t = &src[pos..end];
                    if stack.is_empty() {
                        if !t.bytes().all(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r')) {
                            return Err(invalid("text outside the root element"));
                        }
                    } else {
                        if t.contains("]]>") || (t.contains('&') && !refs_ok(t)) {
                            return Err(invalid("bad text content"));
                        }
                        self.push(Kind::Text, &stack, &mut lastc, pos, end, 0, 0, 0, NS_NONE);
                    }
                    pos = end;
                    continue;
                }
                let rest = &src[pos..];
                if let Some(after) = rest.strip_prefix("<!--") {
                    let e = after.find("-->").ok_or_else(|| invalid("unterminated comment"))? + 4;
                    let body = &rest[4..e];
                    if body.contains("--") || body.ends_with('-') {
                        return Err(invalid("'--' inside a comment"));
                    }
                    if !stack.is_empty() {
                        self.push(Kind::Comment, &stack, &mut lastc, pos, pos + e + 3, 0, 0, 0, NS_NONE);
                    }
                    pos += e + 3;
                } else if rest.starts_with("<![CDATA[") {
                    let e = rest.find("]]>").ok_or_else(|| invalid("unterminated CDATA section"))?;
                    if stack.is_empty() {
                        return Err(invalid("CDATA outside the root element"));
                    }
                    self.push(Kind::Cdata, &stack, &mut lastc, pos, pos + e + 3, 0, 0, 0, NS_NONE);
                    pos += e + 3;
                } else if let Some(after) = rest.strip_prefix("<?") {
                    let e = after.find("?>").ok_or_else(|| invalid("unterminated processing instruction"))? + 2;
                    let target: &str = rest[2..e].split(|c: char| c.is_whitespace()).next().unwrap_or("");
                    if !valid_qname(target.as_bytes()) {
                        return Err(invalid("a processing instruction has no valid target"));
                    }
                    if target.eq_ignore_ascii_case("xml") && pos != 0 {
                        return Err(invalid("an XML declaration that is not at the start"));
                    }
                    if !stack.is_empty() {
                        self.push(Kind::Pi, &stack, &mut lastc, pos, pos + e + 2, 0, 0, 0, NS_NONE);
                    }
                    pos += e + 2;
                } else if rest.starts_with("<!") {
                    return Err(Error::Limit(LimitError::new(
                        LimitKind::XmlEntities,
                        "This OpenDocument file declares a DOCTYPE; it was not processed.",
                    )));
                } else if rest.starts_with("</") {
                    let e = rest.find('>').ok_or_else(|| invalid("unterminated end tag"))?;
                    let name = rest[2..e].trim_end_matches([' ', '\t', '\r', '\n']);
                    let Some(top) = stack.pop() else { return Err(invalid("an end tag with no start tag")) };
                    lastc.pop();
                    let node = &mut self.nodes[top as usize];
                    let q0 = node.s0 as usize + 1;
                    if &src[q0..q0 + node.qlen as usize] != name {
                        return Err(invalid("an end tag does not match its start tag"));
                    }
                    node.e0 = pos as u32;
                    node.e1 = (pos + e + 1) as u32;
                    pos += e + 1;
                } else {
                    if stack.is_empty() && root_seen {
                        return Err(invalid("more than one root element"));
                    }
                    // a start tag
                    let mut i = pos + 1;
                    while i < n && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>') {
                        i += 1;
                    }
                    if !valid_qname(&b[pos + 1..i]) {
                        return Err(invalid("an element has no valid name"));
                    }
                    let (qs, qe) = (pos + 1, i);
                    attrs.clear();
                    own.clear();
                    let mut self_closing = false;
                    let mut had_ws;
                    loop {
                        let ws0 = i;
                        while i < n && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
                            i += 1;
                        }
                        had_ws = i > ws0;
                        if i >= n {
                            return Err(invalid("unterminated start tag"));
                        }
                        if b[i] == b'>' {
                            i += 1;
                            break;
                        }
                        if b[i] == b'/' {
                            if i + 1 < n && b[i + 1] == b'>' {
                                self_closing = true;
                                i += 2;
                                break;
                            }
                            return Err(invalid("a stray '/' in a start tag"));
                        }
                        if !had_ws {
                            return Err(invalid("attributes must be separated by white space"));
                        }
                        let a0 = i;
                        while i < n && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'=' | b'>' | b'/') {
                            i += 1;
                        }
                        if !valid_qname(&b[a0..i]) {
                            return Err(invalid("an attribute has no valid name"));
                        }
                        let a1 = i;
                        while i < n && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
                            i += 1;
                        }
                        if i >= n || b[i] != b'=' {
                            return Err(invalid("an attribute has no value"));
                        }
                        i += 1;
                        while i < n && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
                            i += 1;
                        }
                        if i >= n || (b[i] != b'"' && b[i] != b'\'') {
                            return Err(invalid("an attribute value is not quoted"));
                        }
                        let q = b[i];
                        let v0 = i + 1;
                        let len = b[v0..].iter().position(|c| *c == q).ok_or_else(|| invalid("unterminated attribute value"))?;
                        let v1 = v0 + len;
                        let val = &src[v0..v1];
                        if val.contains('<') || (val.contains('&') && !refs_ok(val)) {
                            return Err(invalid("bad attribute value"));
                        }
                        let aname = &src[a0..a1];
                        if attrs.iter().any(|(s, e, _, _)| &src[*s as usize..*e as usize] == aname) {
                            return Err(invalid("a repeated attribute"));
                        }
                        attrs.push((a0 as u32, a1 as u32, v0 as u32, v1 as u32));
                        i = v1 + 1;
                    }
                    elements += 1;
                    if elements > l.max_xml_elements {
                        return Err(limit(
                            LimitKind::XmlElements,
                            format!("This file has more than {} XML elements in one part. It was not processed.", l.max_xml_elements),
                        ));
                    }
                    if !self_closing && stack.len() >= l.max_xml_depth {
                        return Err(limit(
                            LimitKind::XmlDepth,
                            format!("This file nests XML elements more than {} deep. It was not processed.", l.max_xml_depth),
                        ));
                    }
                    // namespace declarations of this element
                    for (a0, a1, v0, v1) in attrs.iter() {
                        let aname = &src[*a0 as usize..*a1 as usize];
                        let prefix = if aname == "xmlns" {
                            ""
                        } else if let Some(p) = aname.strip_prefix("xmlns:") {
                            p
                        } else {
                            continue;
                        };
                        let uri = unescape(&src[*v0 as usize..*v1 as usize]);
                        if uri.is_empty() && !prefix.is_empty() {
                            return Err(invalid("a namespace prefix bound to nothing"));
                        }
                        let id = if uri.is_empty() { NS_NONE } else { self.ns_id(&uri) };
                        own.push((prefix.to_string(), id));
                    }
                    let qname = &src[qs..qe];
                    let (prefix, loff) = match qname.split_once(':') {
                        Some((p, _)) => (p, p.len() + 1),
                        None => ("", 0),
                    };
                    let parent = stack.last().copied().unwrap_or(NIL);
                    let ns = if let Some((_, id)) = own.iter().rev().find(|(p, _)| p == prefix) {
                        *id
                    } else {
                        match self.lookup(parent, prefix) {
                            Some(id) => id,
                            None if prefix.is_empty() => NS_NONE,
                            None => return Err(invalid("an undeclared namespace prefix")),
                        }
                    };
                    // attribute prefixes must be bound too
                    for (a0, a1, _, _) in attrs.iter() {
                        let aname = &src[*a0 as usize..*a1 as usize];
                        if let Some((p, _)) = aname.split_once(':') {
                            if p != "xmlns" && p != "xml" && !own.iter().any(|(q, _)| q == p) && self.lookup(parent, p).is_none() {
                                return Err(invalid("an undeclared namespace prefix"));
                            }
                        }
                    }
                    let id = self.push(Kind::Elem, &stack, &mut lastc, pos, i, qe - qs, loff, 0, ns);
                    if !own.is_empty() {
                        let decl = std::mem::take(&mut own);
                        if id == 0 {
                            self.root_decls = decl.into_iter().collect();
                        } else {
                            self.decls.insert(id, decl);
                        }
                    }
                    root_seen = true;
                    let node = &mut self.nodes[id as usize];
                    node.e0 = i as u32;
                    node.e1 = i as u32;
                    if !self_closing {
                        stack.push(id);
                        lastc.push(NIL);
                    }
                    pos = i;
                }
            }
            if !stack.is_empty() {
                return Err(invalid("an element is not closed"));
            }
            if !root_seen {
                return Err(invalid("no root element"));
            }
            Ok(())
        })();
        self.src = src;
        res
    }

    #[allow(clippy::too_many_arguments)]
    fn push(&mut self, kind: Kind, stack: &[u32], lastc: &mut [u32], s0: usize, s1: usize, qlen: usize, loff: usize, _unused: u8, ns: u16) -> u32 {
        let id = self.nodes.len() as u32;
        let parent = stack.last().copied().unwrap_or(NIL);
        self.nodes.push(Node {
            kind,
            ns,
            qlen: qlen as u16,
            loff: loff as u16,
            parent,
            first: NIL,
            next: NIL,
            s0: s0 as u32,
            s1: s1 as u32,
            e0: s1 as u32,
            e1: s1 as u32,
        });
        if parent != NIL {
            let slot = lastc.last_mut().expect("level");
            if *slot == NIL {
                self.nodes[parent as usize].first = id;
            } else {
                self.nodes[*slot as usize].next = id;
            }
            *slot = id;
        }
        id
    }

    // ---- reading ----

    pub fn node(&self, i: u32) -> &Node {
        &self.nodes[i as usize]
    }
    pub fn qname(&self, i: u32) -> &str {
        let n = self.node(i);
        &self.src[n.s0 as usize + 1..n.s0 as usize + 1 + n.qlen as usize]
    }
    pub fn local(&self, i: u32) -> &str {
        let n = self.node(i);
        &self.src[n.s0 as usize + 1 + n.loff as usize..n.s0 as usize + 1 + n.qlen as usize]
    }
    pub fn prefix(&self, i: u32) -> &str {
        let n = self.node(i);
        if n.loff == 0 {
            ""
        } else {
            &self.src[n.s0 as usize + 1..n.s0 as usize + n.loff as usize]
        }
    }
    pub fn is(&self, i: u32, ns: u16, local: &str) -> bool {
        let n = self.node(i);
        n.kind == Kind::Elem && n.ns == ns && self.local(i) == local
    }
    pub fn start_tag(&self, i: u32) -> &str {
        let n = self.node(i);
        &self.src[n.s0 as usize..n.s1 as usize]
    }
    pub fn end_tag(&self, i: u32) -> &str {
        let n = self.node(i);
        &self.src[n.e0 as usize..n.e1 as usize]
    }
    pub fn raw(&self, i: u32) -> &str {
        let n = self.node(i);
        &self.src[n.s0 as usize..n.e1 as usize]
    }
    pub fn self_closing(&self, i: u32) -> bool {
        let n = self.node(i);
        n.kind == Kind::Elem && n.e0 == n.e1
    }
    pub fn children(&self, i: u32) -> Children<'_> {
        Children { t: self, at: self.node(i).first }
    }
    pub fn first_elem_child(&self, i: u32, ns: u16, local: &str) -> Option<u32> {
        self.children(i).find(|c| self.is(*c, ns, local))
    }
    /// The decoded text of a text or CDATA node.
    pub fn text(&self, i: u32) -> Cow<'_, str> {
        unescape(self.raw(i))
    }
    pub fn root(&self) -> u32 {
        0
    }

    fn attr_ns(&self, node: u32, qname: &str) -> u16 {
        match qname.split_once(':') {
            Some((p, _)) => self.lookup(node, p).unwrap_or(NS_NONE),
            None => NS_NONE,
        }
    }

    /// The qualified name an attribute with this expanded name has in the start tag, if present.
    pub fn attr_qname(&self, node: u32, ns: u16, local: &str) -> Option<&str> {
        let tag = self.start_tag(node);
        for a in scan_attrs(tag) {
            let q = &tag[a.name_start..a.name_end];
            let l = q.rsplit_once(':').map_or(q, |(_, l)| l);
            if l == local && self.attr_ns(node, q) == ns && !q.starts_with("xmlns") {
                return Some(q);
            }
        }
        None
    }

    /// The decoded value of an attribute by namespace and local name.
    pub fn attr(&self, node: u32, ns: u16, local: &str) -> Option<String> {
        let tag = self.start_tag(node);
        for a in scan_attrs(tag) {
            let q = &tag[a.name_start..a.name_end];
            let l = q.rsplit_once(':').map_or(q, |(_, l)| l);
            if l == local && self.attr_ns(node, q) == ns && !q.starts_with("xmlns") {
                return Some(unescape(&tag[a.val_start..a.val_end]).into_owned());
            }
        }
        None
    }

    /// A prefix bound to namespace `ns` in the scope of `node`.
    pub fn find_prefix(&self, node: u32, ns: u16) -> Option<String> {
        let mut n = node;
        while n != NIL {
            if let Some(d) = self.decls.get(&n) {
                if let Some((p, _)) = d.iter().rev().find(|(p, id)| *id == ns && !p.is_empty() && self.lookup(node, p) == Some(ns)) {
                    return Some(p.clone());
                }
            }
            n = self.nodes[n as usize].parent;
        }
        self.root_decls.iter().filter(|(p, id)| **id == ns && !p.is_empty() && self.lookup(node, p) == Some(ns)).map(|(p, _)| p.clone()).min()
    }

    /// Ancestors of `node`, nearest first.
    pub fn ancestors(&self, node: u32) -> Ancestors<'_> {
        Ancestors { t: self, at: self.node(node).parent }
    }
}

pub struct Children<'a> {
    t: &'a Tree,
    at: u32,
}
impl Iterator for Children<'_> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.at == NIL {
            return None;
        }
        let cur = self.at;
        self.at = self.t.nodes[cur as usize].next;
        Some(cur)
    }
}

pub struct Ancestors<'a> {
    t: &'a Tree,
    at: u32,
}
impl Iterator for Ancestors<'_> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.at == NIL {
            return None;
        }
        let cur = self.at;
        self.at = self.t.nodes[cur as usize].parent;
        Some(cur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(s: &str) -> Result<Tree, Error> {
        Tree::parse(s.to_string(), &Limits::default())
    }

    #[test]
    fn nodes_are_ranges_of_the_source() {
        let s = "<?xml version=\"1.0\"?>\n<a xmlns:t=\"urn:x\" k='v'><t:b x=\"1\"/>text &amp; <![CDATA[<c>]]><!-- c --></a>\n";
        let t = tree(s).unwrap();
        assert_eq!(t.qname(0), "a");
        assert_eq!(t.raw(0), "<a xmlns:t=\"urn:x\" k='v'><t:b x=\"1\"/>text &amp; <![CDATA[<c>]]><!-- c --></a>");
        let kids: Vec<u32> = t.children(0).collect();
        assert_eq!(kids.len(), 4);
        assert_eq!(t.local(kids[0]), "b");
        assert_eq!(t.prefix(kids[0]), "t");
        assert!(t.self_closing(kids[0]));
        assert_eq!(t.text(kids[1]), "text & ");
        assert_eq!(t.text(kids[2]), "<c>");
        assert_eq!(t.attr(kids[0], NS_NONE, "x").as_deref(), Some("1"));
        assert_eq!(t.attr(0, NS_NONE, "k").as_deref(), Some("v"));
    }

    #[test]
    fn well_formedness_is_enforced() {
        for bad in [
            "<a>",
            "<a></b>",
            "<a/><b/>",
            "<a>&nope;</a>",
            "<a>&#0;</a>",
            "<a x=1/>",
            "<a x=\"1\" x=\"2\"/>",
            "<a x=\"<\"/>",
            "<p:a/>",
            "<a p:x=\"1\"/>",
            "text<a/>",
            "<a/>text",
            "<a>]]></a>",
            "<a\u{1}/>",
            "<a><!-- -- --></a>",
            "",
            "<!DOCTYPE a [<!ENTITY x \"y\">]><a/>",
            "<!DOCTYPE a><a/>",
            "<a><?></a>",
            "<?><a/>",
            "<a><?x?></a><?>",
            "<a><!--></a>",
            "<a><!---></a>",
            "<a><![CDATA[]]></a",
        ] {
            assert!(tree(bad).is_err(), "{bad:?} must be refused");
        }
        assert!(tree("<a:b xmlns:a=\"u\"><c a:d=\"1\"/></a:b>").is_ok());
    }

    #[test]
    fn depth_and_count_are_limited_without_recursion() {
        let deep = "<a>".repeat(100_000) + &"</a>".repeat(100_000);
        assert!(matches!(tree(&deep), Err(Error::Limit(_))));
        let l = Limits::new().with_max_xml_elements(10);
        assert!(matches!(Tree::parse("<a>".to_string() + &"<b/>".repeat(20) + "</a>", &l), Err(Error::Limit(_))));
    }

    #[test]
    fn namespaces_resolve_by_uri_not_prefix() {
        let s = "<r xmlns:q=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"><q:p/><x:p xmlns:x=\"urn:other\"/><p xmlns=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"/></r>";
        let t = tree(s).unwrap();
        let k: Vec<u32> = t.children(0).collect();
        assert!(t.is(k[0], NS_TEXT, "p"));
        assert!(!t.is(k[1], NS_TEXT, "p"));
        assert!(t.is(k[2], NS_TEXT, "p"));
        assert_eq!(t.find_prefix(0, NS_TEXT).as_deref(), Some("q"));
    }
}
