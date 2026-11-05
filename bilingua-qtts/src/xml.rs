//! One pass over a decoded XML text: checks that it is well formed and safe (one root, balanced and unique
//! attributes, only the five predefined entities and character references, no DTD internal subset, XML characters
//! only, declared namespace prefixes, depth and element limits) and records the exact character span of every
//! element, so the writer can splice only what it changes and keep every other byte.
//!
//! Elements are numbered in document order. Nothing here executes or resolves anything.

use crate::Limits;
use bilingua_model::{Error, LimitError, LimitKind};


pub fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "Qt Linguist .ts".into(), msg: msg.into() }
}

fn not_wf(msg: impl AsRef<str>) -> Error {
    perr(format!("not well-formed XML: {}", msg.as_ref()))
}

/// One attribute of a start tag, as ranges in the text.
#[derive(Debug, Clone, Copy)]
pub struct Attr {
    pub name: (usize, usize),
    pub val: (usize, usize),
    pub quote: u8,
    /// From the white space before the name to just after the closing quote.
    pub whole: (usize, usize),
}

#[derive(Debug, Clone, Copy)]
pub struct El {
    /// The `<` of the start tag.
    pub start: usize,
    /// End of the qualified name (it starts at `start + 1`).
    pub name_end: usize,
    /// Just after the `>` of the start tag.
    pub stag_end: usize,
    /// Start of the end tag (== `stag_end` for an element written `<a/>`).
    pub etag_start: usize,
    /// Just after the end tag.
    pub end: usize,
    pub empty: bool,
    /// Index of the last element inside this one (itself when it has none).
    pub sub_end: usize,
    pub attr_lo: usize,
    pub attr_hi: usize,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub els: Vec<El>,
    pub attrs: Vec<Attr>,
}

impl Scan {
    pub fn qname<'a>(&self, text: &'a str, i: usize) -> &'a str {
        let e = &self.els[i];
        &text[e.start + 1..e.name_end]
    }

    /// The name without a namespace prefix.
    pub fn local<'a>(&self, text: &'a str, i: usize) -> &'a str {
        let q = self.qname(text, i);
        q.rsplit_once(':').map_or(q, |(_, l)| l)
    }

    pub fn attrs_of(&self, i: usize) -> &[Attr] {
        &self.attrs[self.els[i].attr_lo..self.els[i].attr_hi]
    }

    /// The value of the attribute `name` (as written, no entity decoding).
    pub fn attr_raw<'a>(&self, text: &'a str, i: usize, name: &str) -> Option<&'a str> {
        self.attrs_of(i).iter().find(|a| &text[a.name.0..a.name.1] == name).map(|a| &text[a.val.0..a.val.1])
    }

    /// The value of the attribute `name`, entities decoded and white space normalised as an XML parser does.
    pub fn attr(&self, text: &str, i: usize, name: &str) -> Option<String> {
        self.attr_raw(text, i, name).map(decode_attr)
    }

    /// Direct child elements of `i`, in order.
    pub fn children(&self, i: usize) -> Children<'_> {
        Children { scan: self, next: i + 1, last: self.els[i].sub_end }
    }

    /// Index of the element whose start tag begins at `pos`.
    pub fn at(&self, pos: usize) -> Option<usize> {
        self.els.binary_search_by(|e| e.start.cmp(&pos)).ok()
    }
}

pub struct Children<'a> {
    scan: &'a Scan,
    next: usize,
    last: usize,
}

impl Iterator for Children<'_> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        // an element with no descendants has sub_end == itself, so `next > last` is the end
        if self.next > self.last || self.next >= self.scan.els.len() {
            return None;
        }
        let i = self.next;
        self.next = self.scan.els[i].sub_end + 1;
        Some(i)
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn xml_char(c: char) -> bool {
    matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == ':' || (c as u32) >= 0xC0
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':') || (c as u32) >= 0xB7
}

fn valid_name(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(f) if name_start(f) => it.all(name_char),
        _ => false,
    }
}

/// The end of the name starting at `i`.
fn name_end(text: &str, i: usize) -> usize {
    let b = text.as_bytes();
    let mut j = i;
    while j < b.len() && !is_ws(b[j]) && !matches!(b[j], b'/' | b'>' | b'=' | b'<' | b'"' | b'\'') {
        j += 1;
    }
    j
}

/// Checks the references and characters of a text or attribute value.
fn check_chars(s: &str, in_attr: bool) -> Result<(), Error> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'&' => {
                let Some(len) = b[i + 1..b.len().min(i + 12)].iter().position(|c| *c == b';') else {
                    return Err(not_wf("a '&' that does not start an entity reference"));
                };
                let name = &s[i + 1..i + 1 + len];
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
                    return Err(if valid_name(name) {
                        perr(format!("This Qt Linguist .ts file uses an undefined entity (&{name};); refused."))
                    } else {
                        not_wf("a '&' that does not start an entity reference")
                    });
                }
                i += len + 2;
                continue;
            }
            b'<' if in_attr => return Err(not_wf("'<' in an attribute value")),
            b']' if !in_attr && b[i..].starts_with(b"]]>") => return Err(not_wf("']]>' in text")),
            0..=8 | 0x0B | 0x0C | 0x0E..=0x1F => return Err(not_wf("a control character that XML cannot hold")),
            0xEF if b[i..].starts_with(&[0xEF, 0xBF, 0xBE]) || b[i..].starts_with(&[0xEF, 0xBF, 0xBF]) => {
                return Err(not_wf("a character that XML cannot hold"))
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

/// Entity and character references of an already checked text, line ends normalised (`\r\n` and `\r` to `\n`).
pub fn decode_text(s: &str) -> String {
    let s = if s.contains('\r') { s.replace("\r\n", "\n").replace('\r', "\n") } else { s.to_string() };
    unescape(&s)
}

/// An attribute value: references decoded, tabs and line ends turned into spaces (XML attribute normalisation).
pub fn decode_attr(s: &str) -> String {
    let n: String = s.replace("\r\n", " ").chars().map(|c| if matches!(c, '\n' | '\r' | '\t') { ' ' } else { c }).collect();
    unescape(&n)
}

fn unescape(s: &str) -> String {
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

struct Frame {
    el: usize,
    ns_len: usize,
}

/// Scans `text`. With `fragment` the text is one element with no prolog or epilog rules and namespace prefixes
/// are not checked (the writer re-reads a single `<translation>` that was already checked as part of a file).
pub fn scan(text: &str, fragment: bool, limits: &Limits) -> Result<Scan, Error> {
    let b = text.as_bytes();
    let mut sc = Scan::default();
    let mut stack: Vec<Frame> = Vec::new();
    let mut ns: Vec<String> = Vec::new(); // declared prefixes, innermost last
    let mut pos = 0usize;
    let mut root_done = false;
    let mut doctype_seen = false;
    let skip_ws = |mut i: usize| {
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        i
    };
    while pos < b.len() {
        if stack.is_empty() {
            pos = skip_ws(pos);
            if pos >= b.len() {
                break;
            }
            if b[pos] != b'<' {
                return Err(not_wf(if root_done { "text after the root element" } else { "text before the root element" }));
            }
        } else {
            // text up to the next markup
            let lt = b[pos..].iter().position(|c| *c == b'<').map(|p| p + pos).ok_or_else(|| not_wf("the file ends inside an element"))?;
            if lt > pos {
                check_chars(&text[pos..lt], false)?;
                pos = lt;
            }
        }
        // b[pos] == '<'
        let rest = &b[pos..];
        if rest.starts_with(b"<!--") {
            let end = text[pos + 4..].find("-->").ok_or_else(|| not_wf("a comment that never ends"))? + pos + 4;
            let inner = &text[pos + 4..end];
            if inner.contains("--") || inner.ends_with('-') {
                return Err(not_wf("'--' inside a comment"));
            }
            check_chars(inner, false).map_err(|_| not_wf("a character that XML cannot hold in a comment"))?;
            pos = end + 3;
        } else if rest.starts_with(b"<![CDATA[") {
            if stack.is_empty() {
                return Err(not_wf("a CDATA section outside the root element"));
            }
            let end = text[pos + 9..].find("]]>").ok_or_else(|| not_wf("a CDATA section that never ends"))? + pos + 9;
            for (k, c) in text[pos + 9..end].bytes().enumerate() {
                if matches!(c, 0..=8 | 0x0B | 0x0C | 0x0E..=0x1F) {
                    let _ = k;
                    return Err(not_wf("a control character that XML cannot hold"));
                }
            }
            pos = end + 3;
        } else if rest.starts_with(b"<?") {
            let end = text[pos + 2..].find("?>").ok_or_else(|| not_wf("a processing instruction that never ends"))? + pos + 2;
            let body = &text[pos + 2..end];
            let target = &body[..body.find(|c: char| c.is_ascii_whitespace()).unwrap_or(body.len())];
            if target.eq_ignore_ascii_case("xml") {
                if pos != 0 || target != "xml" {
                    return Err(not_wf("the XML declaration is not at the start of the file"));
                }
            } else if !valid_name(target) {
                return Err(not_wf("a processing instruction without a name"));
            }
            check_chars(body, false).map_err(|_| not_wf("a character that XML cannot hold in a processing instruction"))?;
            pos = end + 2;
        } else if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
            if !stack.is_empty() || root_done || doctype_seen || fragment {
                return Err(not_wf("a DOCTYPE in the wrong place"));
            }
            doctype_seen = true;
            let (mut j, mut q) = (pos + 9, 0u8);
            loop {
                let Some(&c) = b.get(j) else { return Err(not_wf("a DOCTYPE that never ends")) };
                if q != 0 {
                    if c == q {
                        q = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    q = c;
                } else if c == b'[' {
                    return Err(perr(
                        "This Qt Linguist .ts file declares a DTD internal subset; refused (resource files never need one, and it is an entity-expansion / XXE vector).",
                    ));
                } else if c == b'>' {
                    break;
                } else if c == b'<' {
                    return Err(not_wf("'<' inside a DOCTYPE"));
                }
                j += 1;
            }
            pos = j + 1;
        } else if rest.starts_with(b"<!") {
            return Err(not_wf("a markup declaration where an element is expected"));
        } else if rest.starts_with(b"</") {
            let Some(top) = stack.pop() else { return Err(not_wf("an end tag with no start tag")) };
            let ne = name_end(text, pos + 2);
            let name = &text[pos + 2..ne];
            let el = &mut sc.els[top.el];
            if name != &text[el.start + 1..el.name_end] {
                return Err(not_wf(format!("end tag </{name}> does not match <{}>", &text[el.start + 1..el.name_end])));
            }
            let close = skip_ws(ne);
            if b.get(close) != Some(&b'>') {
                return Err(not_wf("an end tag that is not closed"));
            }
            el.etag_start = pos;
            el.end = close + 1;
            sc.els[top.el].sub_end = sc.els.len() - 1;
            ns.truncate(top.ns_len);
            pos = close + 1;
            if stack.is_empty() {
                root_done = true;
            }
        } else {
            // a start tag
            if root_done && !fragment {
                return Err(not_wf("a second root element"));
            }
            if fragment && root_done {
                return Err(not_wf("more than one element"));
            }
            let ne = name_end(text, pos + 1);
            let qname = &text[pos + 1..ne];
            if !valid_name(qname) {
                return Err(not_wf("a start tag without a valid name"));
            }
            let attr_lo = sc.attrs.len();
            let mut j = ne;
            let ns_len = ns.len();
            let (stag_end, empty);
            loop {
                let after_ws = skip_ws(j);
                let Some(&c) = b.get(after_ws) else { return Err(not_wf("a start tag that never ends")) };
                if c == b'>' {
                    stag_end = after_ws + 1;
                    empty = false;
                    break;
                }
                if c == b'/' {
                    if b.get(after_ws + 1) != Some(&b'>') {
                        return Err(not_wf("a '/' inside a start tag"));
                    }
                    stag_end = after_ws + 2;
                    empty = true;
                    break;
                }
                if after_ws == j {
                    return Err(not_wf("attributes must be separated by white space"));
                }
                let ae = name_end(text, after_ws);
                let aname = &text[after_ws..ae];
                if !valid_name(aname) {
                    return Err(not_wf("an attribute without a valid name"));
                }
                let mut k = skip_ws(ae);
                if b.get(k) != Some(&b'=') {
                    return Err(not_wf(format!("attribute {aname} has no value")));
                }
                k = skip_ws(k + 1);
                let q = match b.get(k) {
                    Some(&q @ (b'"' | b'\'')) => q,
                    _ => return Err(not_wf(format!("attribute {aname} is not quoted"))),
                };
                let vend = b[k + 1..].iter().position(|c| *c == q).map(|p| p + k + 1).ok_or_else(|| not_wf("an attribute value that never ends"))?;
                check_chars(&text[k + 1..vend], true)?;
                if sc.attrs[attr_lo..].iter().any(|a| &text[a.name.0..a.name.1] == aname) {
                    return Err(not_wf(format!("attribute {aname} appears twice")));
                }
                if aname == "xmlns" {
                    ns.push(String::new());
                } else if let Some(p) = aname.strip_prefix("xmlns:") {
                    ns.push(p.to_string());
                }
                sc.attrs.push(Attr { name: (after_ws, ae), val: (k + 1, vend), quote: q, whole: (j, vend + 1) });
                j = vend + 1;
            }
            if !fragment {
                let declared = |p: &str| p == "xml" || p == "xmlns" || ns.iter().any(|d| d == p);
                if let Some((p, _)) = qname.split_once(':') {
                    if !declared(p) {
                        return Err(not_wf(format!("namespace prefix {p} is not defined")));
                    }
                }
                for a in &sc.attrs[attr_lo..] {
                    if let Some((p, _)) = text[a.name.0..a.name.1].split_once(':') {
                        if !declared(p) {
                            return Err(not_wf(format!("namespace prefix {p} is not defined")));
                        }
                    }
                }
            }
            if sc.els.len() >= limits.max_xml_elements {
                return Err(LimitError::new(LimitKind::XmlElements, format!("This file has more than {} XML elements in one part. It was not processed.", limits.max_xml_elements)).into());
            }
            let idx = sc.els.len();
            sc.els.push(El {
                start: pos,
                name_end: ne,
                stag_end,
                etag_start: stag_end,
                end: stag_end,
                empty,
                sub_end: idx,
                attr_lo,
                attr_hi: sc.attrs.len(),
            });
            pos = stag_end;
            if empty {
                ns.truncate(ns_len);
                if stack.is_empty() {
                    root_done = true;
                }
            } else {
                if stack.len() >= limits.max_xml_depth {
                    return Err(LimitError::new(LimitKind::XmlDepth, format!("This file nests XML elements more than {} deep. It was not processed.", limits.max_xml_depth)).into());
                }
                stack.push(Frame { el: idx, ns_len });
            }
        }
    }
    if !stack.is_empty() {
        return Err(not_wf("the file ends inside an element"));
    }
    if !root_done {
        return Err(not_wf("there is no root element"));
    }
    Ok(sc)
}

/// A node inside an element's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Text (references decoded, CDATA included).
    Text(String),
    /// A child element, comment or processing instruction, as written: `(start, end, label)`.
    Raw(usize, usize, String),
}

/// The content of element `i` as a sequence of text and non-text nodes. The file was scanned before, so this
/// does not re-check it.
pub fn content(sc: &Scan, text: &str, i: usize) -> Vec<Node> {
    let e = &sc.els[i];
    let mut out: Vec<Node> = Vec::new();
    let push_text = |out: &mut Vec<Node>, t: String| {
        if t.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(Node::Text(prev)) => prev.push_str(&t),
            _ => out.push(Node::Text(t)),
        }
    };
    let (mut p, end) = (e.stag_end, e.etag_start);
    while p < end {
        let lt = text[p..end].find('<').map_or(end, |q| q + p);
        if lt > p {
            push_text(&mut out, decode_text(&text[p..lt]));
        }
        if lt >= end {
            break;
        }
        let rest = &text[lt..end];
        if rest.starts_with("<!--") {
            let e = rest.find("-->").map_or(rest.len(), |q| q + 3);
            out.push(Node::Raw(lt, lt + e, "comment".into()));
            p = lt + e;
        } else if rest.starts_with("<![CDATA[") {
            let close = rest.find("]]>").unwrap_or(rest.len());
            let body = &rest[9..close.max(9)];
            let norm = if body.contains('\r') { body.replace("\r\n", "\n").replace('\r', "\n") } else { body.to_string() };
            push_text(&mut out, norm);
            p = lt + (close + 3).min(rest.len());
        } else if rest.starts_with("<?") {
            let e = rest.find("?>").map_or(rest.len(), |q| q + 2);
            out.push(Node::Raw(lt, lt + e, "processing-instruction".into()));
            p = lt + e;
        } else if let Some(c) = sc.at(lt) {
            out.push(Node::Raw(lt, sc.els[c].end, sc.local(text, c).to_string()));
            p = sc.els[c].end;
        } else {
            break; // cannot happen in a scanned file
        }
    }
    out
}

/// What an element's `.text` is: the text before its first child element, comment or instruction.
pub fn leading_text(nodes: &[Node]) -> String {
    match nodes.first() {
        Some(Node::Text(t)) => t.clone(),
        _ => String::new(),
    }
}
