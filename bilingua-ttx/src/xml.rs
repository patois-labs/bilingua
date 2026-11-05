//! A small well-formedness checking XML scanner that keeps the exact byte span of every element, so the writer
//! can splice. No DTD, no custom entities (the caller refuses DOCTYPE first), namespaces declared, XML characters
//! only. Positions are byte offsets into the decoded text.

use bilingua_model::{Error, LimitError, LimitKind, Limits};
use std::collections::HashSet;

pub fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "ttx".into(), msg: msg.into() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Tu,
    Tuv,
    Ut,
    Df,
    UserSettings,
    Body,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub struct Attr {
    pub name: (usize, usize),
    pub val: (usize, usize),
}

#[derive(Debug, Clone, Copy)]
pub enum Kid {
    /// Character data (`cdata` false) or a CDATA section (`cdata` true); the span is the whole raw text.
    Text { s: usize, e: usize, cdata: bool },
    Elem(usize),
    /// A comment or processing instruction.
    Misc,
}

#[derive(Debug)]
pub struct Elem {
    pub kind: Kind,
    pub name: (usize, usize),
    pub start: usize,
    pub start_end: usize,
    pub end_start: usize,
    pub end: usize,
    pub empty: bool,
    pub in_tu: bool,
    pub attrs: (usize, usize),
    pub kids: Vec<Kid>,
}

#[derive(Debug)]
pub struct Tree {
    pub elems: Vec<Elem>,
    pub attrs: Vec<Attr>,
}

impl Tree {
    pub fn attr<'a>(&self, text: &'a str, el: usize, name: &str) -> Option<&'a str> {
        let e = &self.elems[el];
        self.attrs[e.attrs.0..e.attrs.0 + e.attrs.1]
            .iter()
            .find(|a| &text[a.name.0..a.name.1] == name)
            .map(|a| &text[a.val.0..a.val.1])
    }
    /// Attribute value as an XML parser reports it (line ends and white space normalised, references resolved).
    pub fn attr_value(&self, text: &str, el: usize, name: &str) -> Option<String> {
        self.attr(text, el, name).map(attr_normalise)
    }
    pub fn qname<'a>(&self, text: &'a str, el: usize) -> &'a str {
        let e = &self.elems[el];
        &text[e.name.0..e.name.1]
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn valid_name(s: &str) -> bool {
    let mut it = s.chars();
    let Some(f) = it.next() else { return false };
    if !(f.is_ascii_alphabetic() || f == '_' || f == ':' || (f as u32) >= 0xC0) {
        return false;
    }
    it.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':') || (c as u32) >= 0xB7)
}

fn xml_char(c: char) -> bool {
    matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
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

/// XML line-end normalisation of raw input: CRLF and lone CR become LF.
pub fn line_ends(s: &str) -> String {
    if !s.contains('\r') {
        return s.to_string();
    }
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// The value of raw character data: line ends normalised, references resolved.
pub fn text_value(raw: &str) -> String {
    unescape(&line_ends(raw))
}

fn attr_normalise(raw: &str) -> String {
    let n = line_ends(raw);
    let n: String = n.chars().map(|c| if matches!(c, '\t' | '\n') { ' ' } else { c }).collect();
    unescape(&n)
}

fn limit(kind: LimitKind, msg: String) -> Error {
    LimitError::new(kind, msg).into()
}

fn find(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > b.len() {
        return None;
    }
    b[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn local_kind(qname: &str) -> Kind {
    let local = qname.rsplit(':').next().unwrap_or(qname);
    let l = local.as_bytes();
    let is = |s: &str| l.eq_ignore_ascii_case(s.as_bytes());
    if is("tu") {
        Kind::Tu
    } else if is("tuv") {
        Kind::Tuv
    } else if is("ut") {
        Kind::Ut
    } else if is("df") {
        Kind::Df
    } else if is("usersettings") {
        Kind::UserSettings
    } else if is("body") {
        Kind::Body
    } else {
        Kind::Other
    }
}

pub fn parse(text: &str, limits: &Limits) -> Result<Tree, Error> {
    let b = text.as_bytes();
    let n = b.len();
    // XML characters only (control characters and U+FFFE / U+FFFF are not allowed anywhere)
    if let Some(p) = b.iter().position(|c| *c < 0x20 && !matches!(*c, 9 | 10 | 13)) {
        return Err(perr(format!("not well-formed XML: character U+{:04X} is not allowed", b[p])));
    }
    if b.windows(3).any(|w| w == [0xEF, 0xBF, 0xBE] || w == [0xEF, 0xBF, 0xBF]) {
        return Err(perr("not well-formed XML: character U+FFFE or U+FFFF is not allowed"));
    }
    let mut elems: Vec<Elem> = Vec::new();
    let mut attrs: Vec<Attr> = Vec::new();
    let mut stack: Vec<(usize, usize, &str)> = Vec::new(); // (element, ns length at open, qname)
    let mut ns: Vec<&str> = Vec::new();
    let mut i = 0usize;
    let mut root_seen = false;
    if b.starts_with(b"<?xml") && b.get(5).is_some_and(|c| is_ws(*c)) {
        let Some(e) = find(b, 5, b"?>") else { return Err(perr("not well-formed XML: unterminated XML declaration")) };
        if !text[5..e].contains("version") {
            return Err(perr("not well-formed XML: the XML declaration has no version"));
        }
        i = e + 2;
    }
    while i < n {
        if b[i] != b'<' {
            let e = b[i..].iter().position(|c| *c == b'<').map_or(n, |p| p + i);
            if stack.is_empty() {
                if !b[i..e].iter().all(|c| is_ws(*c)) {
                    return Err(perr("not well-formed XML: text outside the root element"));
                }
            } else {
                let chunk = &text[i..e];
                if chunk.contains('&') {
                    check_refs(chunk, "text")?;
                }
                if chunk.contains("]]>") {
                    return Err(perr("not well-formed XML: ']]>' in text"));
                }
                let top = stack.last().map(|s| s.0).unwrap_or(0);
                elems[top].kids.push(Kid::Text { s: i, e, cdata: false });
            }
            i = e;
            continue;
        }
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            let Some(j) = find(b, i + 4, b"-->") else { return Err(perr("not well-formed XML: unterminated comment")) };
            let body = &text[i + 4..j];
            if body.contains("--") || body.ends_with('-') {
                return Err(perr("not well-formed XML: '--' inside a comment"));
            }
            if let Some(top) = stack.last() {
                elems[top.0].kids.push(Kid::Misc);
            }
            i = j + 3;
        } else if rest.starts_with(b"<![CDATA[") {
            let Some(top) = stack.last().map(|s| s.0) else { return Err(perr("not well-formed XML: CDATA outside the root element")) };
            let Some(j) = find(b, i + 9, b"]]>") else { return Err(perr("not well-formed XML: unterminated CDATA section")) };
            elems[top].kids.push(Kid::Text { s: i, e: j + 3, cdata: true });
            i = j + 3;
        } else if rest.starts_with(b"<?") {
            let Some(j) = find(b, i + 2, b"?>") else { return Err(perr("not well-formed XML: unterminated processing instruction")) };
            let target = text[i + 2..j].split(|c: char| c.is_ascii_whitespace()).next().unwrap_or("");
            if !valid_name(target) || target.eq_ignore_ascii_case("xml") {
                return Err(perr("not well-formed XML: bad processing instruction"));
            }
            if let Some(top) = stack.last() {
                elems[top.0].kids.push(Kid::Misc);
            }
            i = j + 2;
        } else if rest.starts_with(b"<!") {
            return Err(perr("not well-formed XML: unexpected markup declaration"));
        } else if rest.starts_with(b"</") {
            let Some(g) = b[i..].iter().position(|c| *c == b'>').map(|p| p + i) else { return Err(perr("not well-formed XML: unterminated end tag")) };
            let name = text[i + 2..g].trim_end_matches(|c: char| c.is_ascii_whitespace());
            let Some((idx, ns_len, qn)) = stack.pop() else { return Err(perr("not well-formed XML: unbalanced end tag")) };
            if name != qn {
                return Err(perr(format!("not well-formed XML: end tag </{name}> does not match <{qn}>")));
            }
            elems[idx].end_start = i;
            elems[idx].end = g + 1;
            ns.truncate(ns_len);
            i = g + 1;
        } else {
            // start tag
            let ns_len = ns.len();
            let mut j = i + 1;
            while j < n && !is_ws(b[j]) && b[j] != b'/' && b[j] != b'>' {
                j += 1;
            }
            let qname = &text[i + 1..j];
            if !valid_name(qname) || qname.matches(':').count() > 1 {
                return Err(perr("not well-formed XML: malformed start tag"));
            }
            let attr_first = attrs.len();
            let (empty, tag_end);
            loop {
                let mut k = j;
                while k < n && is_ws(b[k]) {
                    k += 1;
                }
                if k >= n {
                    return Err(perr("not well-formed XML: unterminated start tag"));
                }
                match b[k] {
                    b'>' => {
                        empty = false;
                        tag_end = k + 1;
                        break;
                    }
                    b'/' => {
                        if b.get(k + 1) != Some(&b'>') {
                            return Err(perr("not well-formed XML: malformed start tag"));
                        }
                        empty = true;
                        tag_end = k + 2;
                        break;
                    }
                    _ => {}
                }
                if k == j {
                    return Err(perr("not well-formed XML: malformed start tag"));
                }
                let an = k;
                while k < n && !is_ws(b[k]) && b[k] != b'=' && b[k] != b'>' && b[k] != b'/' {
                    k += 1;
                }
                let aname = &text[an..k];
                if !valid_name(aname) || aname.matches(':').count() > 1 {
                    return Err(perr("not well-formed XML: malformed attribute name"));
                }
                while k < n && is_ws(b[k]) {
                    k += 1;
                }
                if k >= n || b[k] != b'=' {
                    return Err(perr("not well-formed XML: attribute without a value"));
                }
                k += 1;
                while k < n && is_ws(b[k]) {
                    k += 1;
                }
                if k >= n || !matches!(b[k], b'"' | b'\'') {
                    return Err(perr("not well-formed XML: attribute value is not quoted"));
                }
                let q = b[k];
                let vs = k + 1;
                let Some(len) = b[vs..].iter().position(|c| *c == q) else { return Err(perr("not well-formed XML: unterminated attribute value")) };
                let vraw = &text[vs..vs + len];
                if vraw.contains('<') {
                    return Err(perr("not well-formed XML: '<' in an attribute value"));
                }
                if vraw.contains('&') {
                    check_refs(vraw, "an attribute value")?;
                }
                attrs.push(Attr { name: (an, an + aname.len()), val: (vs, vs + len) });
                j = vs + len + 1;
            }
            // duplicate attributes
            let names: Vec<&str> = attrs[attr_first..].iter().map(|a| &text[a.name.0..a.name.1]).collect();
            if names.len() > 8 {
                let mut seen = HashSet::new();
                if !names.iter().all(|x| seen.insert(*x)) {
                    return Err(perr("not well-formed XML: duplicate attribute"));
                }
            } else {
                for (x, a) in names.iter().enumerate() {
                    if names[..x].contains(a) {
                        return Err(perr("not well-formed XML: duplicate attribute"));
                    }
                }
            }
            for a in &attrs[attr_first..] {
                let nm = &text[a.name.0..a.name.1];
                if nm == "xmlns" {
                    ns.push("");
                } else if let Some(p) = nm.strip_prefix("xmlns:") {
                    ns.push(p);
                }
            }
            let declared = |p: &str| p == "xml" || ns.contains(&p);
            if let Some((p, _)) = qname.split_once(':') {
                if !declared(p) {
                    return Err(perr(format!("not well-formed XML: namespace prefix {p} is not defined")));
                }
            }
            for nm in &names {
                if let Some((p, _)) = nm.split_once(':') {
                    if p != "xmlns" && !declared(p) {
                        return Err(perr(format!("not well-formed XML: namespace prefix {p} is not defined")));
                    }
                }
            }
            let parent = stack.last().map(|s| s.0);
            if parent.is_none() {
                if root_seen {
                    return Err(perr("not well-formed XML: more than one root element"));
                }
                root_seen = true;
            }
            if stack.len() >= limits.max_xml_depth {
                return Err(limit(LimitKind::XmlDepth, format!("This file nests elements deeper than {} levels. It was not processed.", limits.max_xml_depth)));
            }
            let kind = local_kind(qname);
            let in_tu = parent.is_some_and(|p| elems[p].in_tu || elems[p].kind == Kind::Tu);
            let idx = elems.len();
            elems.push(Elem {
                kind,
                name: (i + 1, i + 1 + qname.len()),
                start: i,
                start_end: tag_end,
                end_start: tag_end,
                end: tag_end,
                empty,
                in_tu,
                attrs: (attr_first, attrs.len() - attr_first),
                kids: Vec::new(),
            });
            if let Some(p) = parent {
                elems[p].kids.push(Kid::Elem(idx));
            }
            if empty {
                ns.truncate(ns_len);
            } else {
                stack.push((idx, ns_len, qname));
            }
            i = tag_end;
        }
    }
    if !stack.is_empty() {
        return Err(perr("not well-formed XML: an element is not closed"));
    }
    if !root_seen {
        return Err(perr("not well-formed XML: no root element"));
    }
    Ok(Tree { elems, attrs })
}
