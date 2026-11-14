//! A strict XML 1.0 reader (no DTD, no entities beyond the five predefined ones) that builds a [`Dom`] with source
//! spans. Iterative, depth capped. Namespace prefixes must be declared, as lxml requires.

use crate::dom::*;
use std::collections::HashSet;

pub enum PErr {
    Syntax(String),
    Depth(usize),
}

pub struct Parsed {
    pub nodes: Vec<Node>,
    pub uris: Vec<String>,
    pub root: Id,
    pub decl_encoding: Option<String>,
}

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

struct P<'a> {
    s: &'a str,
    b: &'a [u8],
    p: usize,
    nodes: Vec<Node>,
    uris: Vec<String>,
    scope: Vec<(String, u32)>,
    marks: Vec<usize>,
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

fn name_start(c: char) -> bool {
    c.is_ascii_alphabetic()
        || c == '_'
        || c == ':'
        || matches!(c as u32, 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0x2FF | 0x370..=0x37D | 0x37F..=0x1FFF | 0x200C..=0x200D | 0x2070..=0x218F | 0x2C00..=0x2FEF | 0x3001..=0xD7FF | 0xF900..=0xFDCF | 0xFDF0..=0xFFFD | 0x10000..=0xEFFFF)
}

fn name_char(c: char) -> bool {
    name_start(c) || c.is_ascii_digit() || c == '-' || c == '.' || matches!(c as u32, 0xB7 | 0x300..=0x36F | 0x203F..=0x2040)
}

pub fn parse(text: &str, max_depth: usize) -> Result<Parsed, PErr> {
    if text.len() >= u32::MAX as usize - 1 {
        return Err(PErr::Syntax("the file is too large for this reader".into()));
    }
    let mut p = P { s: text, b: text.as_bytes(), p: 0, nodes: Vec::new(), uris: Vec::new(), scope: Vec::new(), marks: Vec::new() };
    let enc = p.xml_decl()?;
    p.misc(true)?;
    let root = p.element_tree(max_depth)?;
    p.misc(false)?;
    Ok(Parsed { nodes: p.nodes, uris: p.uris, root, decl_encoding: enc })
}

impl<'a> P<'a> {
    fn err<T>(&self, msg: &str) -> Result<T, PErr> {
        let line = 1 + self.b[..self.p.min(self.b.len())].iter().filter(|c| **c == b'\n').count();
        Err(PErr::Syntax(format!("{msg} (line {line})")))
    }
    fn starts(&self, pat: &str) -> bool {
        self.b[self.p..].starts_with(pat.as_bytes())
    }
    fn skip_ws(&mut self) -> usize {
        let st = self.p;
        while self.p < self.b.len() && is_ws(self.b[self.p]) {
            self.p += 1;
        }
        self.p - st
    }

    fn xml_decl(&mut self) -> Result<Option<String>, PErr> {
        if !(self.starts("<?xml") && self.b.get(5).is_some_and(|c| is_ws(*c))) {
            return Ok(None);
        }
        let Some(end) = self.s[self.p..].find("?>") else { return self.err("unterminated XML declaration") };
        let inner = &self.s[self.p + 5..self.p + end];
        let mut enc = None;
        let mut rest = inner.trim_start_matches(|c: char| xml_ws(c));
        let mut idx = 0;
        while !rest.is_empty() {
            let Some(eq) = rest.find('=') else { return self.err("malformed XML declaration") };
            let key = rest[..eq].trim_matches(|c: char| xml_ws(c));
            let after = rest[eq + 1..].trim_start_matches(|c: char| xml_ws(c));
            let q = after.chars().next();
            if !matches!(q, Some('"') | Some('\'')) {
                return self.err("malformed XML declaration");
            }
            let q = q.unwrap_or('"');
            let Some(close) = after[1..].find(q) else { return self.err("malformed XML declaration") };
            let val = &after[1..1 + close];
            match (idx, key) {
                (0, "version") => {
                    if !val.starts_with("1.") || val.len() < 3 {
                        return self.err("unsupported XML version");
                    }
                }
                (_, "encoding") if enc.is_none() && idx <= 1 => enc = Some(val.to_string()),
                (_, "standalone") if idx >= 1 => {
                    if val != "yes" && val != "no" {
                        return self.err("malformed standalone value");
                    }
                }
                _ => return self.err("malformed XML declaration"),
            }
            idx += 1;
            rest = after[1 + close + 1..].trim_start_matches(|c: char| xml_ws(c));
        }
        self.p += end + 2;
        Ok(enc)
    }

    /// Whitespace, comments and processing instructions. Before the root (`prolog`) the next thing must be `<`.
    fn misc(&mut self, prolog: bool) -> Result<(), PErr> {
        loop {
            self.skip_ws();
            if self.p >= self.b.len() {
                return if prolog { self.err("no root element") } else { Ok(()) };
            }
            if self.starts("<!--") {
                self.comment()?;
            } else if self.starts("<?") {
                self.pi()?;
            } else if self.starts("<!") {
                return self.err("a DOCTYPE is not allowed");
            } else if prolog && self.b[self.p] == b'<' {
                return Ok(());
            } else {
                return self.err(if prolog { "start tag expected" } else { "extra content at the end of the document" });
            }
        }
    }

    fn check_chars(&self, from: usize, to: usize) -> Result<(), PErr> {
        let b = &self.b[from..to];
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                0..=8 | 11 | 12 | 14..=31 => return self.err("illegal character"),
                0xEF if b.get(i + 1) == Some(&0xBF) && matches!(b.get(i + 2), Some(0xBE) | Some(0xBF)) => return self.err("illegal character"),
                _ => {}
            }
            i += 1;
        }
        Ok(())
    }

    /// Comment node data: returns (start, end) of the whole comment and moves past it.
    fn comment(&mut self) -> Result<(usize, usize), PErr> {
        let st = self.p;
        let Some(e) = self.s[st + 4..].find("-->") else { return self.err("unterminated comment") };
        let body = &self.s[st + 4..st + 4 + e];
        if body.contains("--") || body.ends_with('-') {
            return self.err("'--' is not allowed in a comment");
        }
        self.check_chars(st + 4, st + 4 + e)?;
        self.p = st + 4 + e + 3;
        Ok((st, self.p))
    }

    fn pi(&mut self) -> Result<(usize, usize), PErr> {
        let st = self.p;
        self.p += 2;
        let (ns, ne) = self.read_name()?;
        let target = &self.s[ns..ne];
        if target.eq_ignore_ascii_case("xml") {
            return self.err("an XML declaration is only allowed at the start of the file");
        }
        if target.contains(':') {
            return self.err("a processing instruction target cannot contain a colon");
        }
        if !self.starts("?>") && self.skip_ws() == 0 {
            return self.err("space required after the processing instruction target");
        }
        let Some(e) = self.s[self.p..].find("?>") else { return self.err("unterminated processing instruction") };
        self.check_chars(self.p, self.p + e)?;
        self.p += e + 2;
        Ok((st, self.p))
    }

    fn read_name(&mut self) -> Result<(usize, usize), PErr> {
        let st = self.p;
        let mut it = self.s[st..].char_indices();
        match it.next() {
            Some((_, c)) if name_start(c) => {}
            _ => return self.err("name expected"),
        }
        let mut end = self.s.len();
        for (i, c) in it {
            if !name_char(c) {
                end = st + i;
                break;
            }
        }
        self.p = end;
        Ok((st, end))
    }

    fn new_node(&mut self, n: Node, parent: Option<Id>) -> Id {
        self.nodes.push(n);
        let id = (self.nodes.len() - 1) as Id;
        if let Some(p) = parent {
            self.nodes[id as usize].parent = p;
            self.nodes[p as usize].children.push(id);
        }
        id
    }

    fn element_tree(&mut self, max_depth: usize) -> Result<Id, PErr> {
        let mut stack: Vec<Id> = Vec::new();
        let mut root = NIL;
        loop {
            if self.p >= self.b.len() {
                return self.err("unexpected end of file");
            }
            if self.b[self.p] == b'<' && !self.starts("<![CDATA[") {
                if self.starts("</") {
                    self.end_tag(&mut stack)?;
                    if stack.is_empty() {
                        return Ok(root);
                    }
                } else if self.starts("<!--") {
                    let Some(&top) = stack.last() else { return self.err("comment outside the root") };
                    let (s, e) = self.comment()?;
                    let mut n = Node::blank(Kind::Comment);
                    n.src = Some((s as u32, e as u32));
                    n.dirty = false;
                    self.new_node(n, Some(top));
                } else if self.starts("<?") {
                    let Some(&top) = stack.last() else { return self.err("processing instruction outside the root") };
                    let (s, e) = self.pi()?;
                    let mut n = Node::blank(Kind::Pi);
                    n.src = Some((s as u32, e as u32));
                    n.dirty = false;
                    n.name = self.s[s + 2..e - 2].split(|c: char| xml_ws(c)).next().unwrap_or("").to_string();
                    self.new_node(n, Some(top));
                } else if self.starts("<!") {
                    return self.err("unexpected markup declaration");
                } else {
                    if stack.len() >= max_depth {
                        return Err(PErr::Depth(max_depth));
                    }
                    let (id, closed) = self.start_tag(stack.last().copied())?;
                    if root == NIL {
                        root = id;
                    }
                    if !closed {
                        stack.push(id);
                    } else if stack.is_empty() {
                        return Ok(root);
                    }
                }
            } else {
                let Some(&top) = stack.last() else { return self.err("text outside the root element") };
                self.text_run(top)?;
            }
        }
    }

    fn text_run(&mut self, parent: Id) -> Result<(), PErr> {
        let st = self.p;
        let n = self.b.len();
        while self.p < n {
            match self.b[self.p] {
                b'<' => {
                    if self.starts("<![CDATA[") {
                        let Some(e) = self.s[self.p + 9..].find("]]>") else { return self.err("unterminated CDATA section") };
                        self.check_chars(self.p + 9, self.p + 9 + e)?;
                        self.p += 9 + e + 3;
                    } else {
                        break;
                    }
                }
                b'&' => match decode_ref(&self.s[self.p..]) {
                    Some((_, len)) => self.p += len,
                    None => return self.err("undefined or malformed entity reference"),
                },
                b']' => {
                    if self.starts("]]>") {
                        return self.err("']]>' is not allowed in text");
                    }
                    self.p += 1;
                }
                0..=8 | 11 | 12 | 14..=31 => return self.err("illegal character"),
                0xEF if self.b.get(self.p + 1) == Some(&0xBF) && matches!(self.b.get(self.p + 2), Some(0xBE) | Some(0xBF)) => {
                    return self.err("illegal character")
                }
                _ => self.p += 1,
            }
        }
        let mut node = Node::blank(Kind::Text);
        node.src = Some((st as u32, self.p as u32));
        node.dirty = false;
        self.new_node(node, Some(parent));
        Ok(())
    }

    fn end_tag(&mut self, stack: &mut Vec<Id>) -> Result<(), PErr> {
        self.p += 2;
        let (ns, ne) = self.read_name()?;
        self.skip_ws();
        if self.p >= self.b.len() || self.b[self.p] != b'>' {
            return self.err("'>' expected in an end tag");
        }
        self.p += 1;
        let Some(top) = stack.pop() else { return self.err("unexpected end tag") };
        if self.nodes[top as usize].name != self.s[ns..ne] {
            return self.err("opening and ending tag mismatch");
        }
        let node = &mut self.nodes[top as usize];
        if let Some((s, _)) = node.src {
            node.src = Some((s, self.p as u32));
        }
        let mark = self.marks.pop().unwrap_or(0);
        self.scope.truncate(mark);
        Ok(())
    }

    fn attr_value(&self, raw: &str) -> Result<String, PErr> {
        let mut out = String::with_capacity(raw.len());
        let b = raw.as_bytes();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'<' => return self.err("'<' is not allowed in an attribute value"),
                b'&' => match decode_ref(&raw[i..]) {
                    Some((c, len)) => {
                        out.push(c);
                        i += len;
                    }
                    None => return self.err("undefined or malformed entity reference"),
                },
                b'\r' => {
                    out.push(' ');
                    i += if b.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                }
                b'\n' | b'\t' => {
                    out.push(' ');
                    i += 1;
                }
                0..=8 | 11 | 12 | 14..=31 => return self.err("illegal character"),
                _ => {
                    let ch = raw[i..].chars().next().unwrap_or(' ');
                    if !legal_char(ch) {
                        return self.err("illegal character");
                    }
                    out.push(ch);
                    i += ch.len_utf8();
                }
            }
        }
        Ok(out)
    }

    fn lookup(&self, prefix: &str) -> Option<u32> {
        self.scope.iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| *u)
    }

    fn intern(&mut self, uri: &str) -> u32 {
        if let Some(i) = self.uris.iter().position(|u| u == uri) {
            return i as u32 + 1;
        }
        self.uris.push(uri.to_string());
        self.uris.len() as u32
    }

    fn start_tag(&mut self, parent: Option<Id>) -> Result<(Id, bool), PErr> {
        let tag_start = self.p;
        self.p += 1;
        let (ns_, ne) = self.read_name()?;
        let name = self.s[ns_..ne].to_string();
        let mut attrs: Vec<(String, String)> = Vec::new();
        let mut decls: Vec<(String, String)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let closed;
        loop {
            let ws = self.skip_ws();
            if self.p >= self.b.len() {
                return self.err("unexpected end of file in a start tag");
            }
            match self.b[self.p] {
                b'>' => {
                    self.p += 1;
                    closed = false;
                    break;
                }
                b'/' => {
                    if self.starts("/>") {
                        self.p += 2;
                        closed = true;
                        break;
                    }
                    return self.err("'>' expected after '/'");
                }
                _ => {
                    if ws == 0 {
                        return self.err("space required between attributes");
                    }
                    let (as_, ae) = self.read_name()?;
                    let an = self.s[as_..ae].to_string();
                    self.skip_ws();
                    if self.p >= self.b.len() || self.b[self.p] != b'=' {
                        return self.err("attribute without a value");
                    }
                    self.p += 1;
                    self.skip_ws();
                    let q = self.b.get(self.p).copied();
                    if !matches!(q, Some(b'"') | Some(b'\'')) {
                        return self.err("attribute value must be quoted");
                    }
                    let q = q.unwrap_or(b'"');
                    let vs = self.p + 1;
                    let Some(l) = self.b[vs..].iter().position(|c| *c == q) else { return self.err("unterminated attribute value") };
                    let value = self.attr_value(&self.s[vs..vs + l])?;
                    self.p = vs + l + 1;
                    if !seen.insert(an.clone()) {
                        return self.err("duplicate attribute");
                    }
                    if an == "xmlns" {
                        decls.push((String::new(), value));
                    } else if let Some(pfx) = an.strip_prefix("xmlns:") {
                        if pfx.is_empty() || pfx.contains(':') || value.is_empty() {
                            return self.err("invalid namespace declaration");
                        }
                        decls.push((pfx.to_string(), value));
                    } else {
                        attrs.push((an, value));
                    }
                }
            }
        }
        let start_end = self.p;
        let mark = self.scope.len();
        for (pfx, uri) in &decls {
            let idx = if uri.is_empty() { 0 } else { self.intern(uri) };
            self.scope.push((pfx.clone(), idx));
        }
        // resolve names
        let ns = match self.qname_ns(&name, true) {
            Ok(k) => k,
            Err(e) => {
                self.scope.truncate(mark);
                return Err(e);
            }
        };
        for (an, _) in &attrs {
            if let Err(e) = self.qname_ns(an, false) {
                self.scope.truncate(mark);
                return Err(e);
            }
        }
        let mut n = Node::blank(Kind::Elem);
        n.name = name;
        n.ns = ns;
        n.attrs = attrs;
        n.nsdecls = decls;
        n.src = Some((tag_start as u32, start_end as u32));
        n.start_end = start_end as u32;
        n.self_closed = closed;
        n.dirty = false;
        n.attrs_dirty = false;
        let id = self.new_node(n, parent);
        if closed {
            self.scope.truncate(mark);
        } else {
            self.marks.push(mark);
        }
        Ok((id, closed))
    }

    fn qname_ns(&mut self, name: &str, element: bool) -> Result<u32, PErr> {
        match name.split_once(':') {
            None => {
                if element {
                    Ok(self.lookup("").unwrap_or(0))
                } else {
                    Ok(0)
                }
            }
            Some((pfx, local)) => {
                if pfx.is_empty() || local.is_empty() || local.contains(':') {
                    return self.err("invalid qualified name");
                }
                if pfx == "xml" {
                    return Ok(self.intern(XML_NS));
                }
                match self.lookup(pfx) {
                    Some(k) if k != 0 => Ok(k),
                    _ => self.err("namespace prefix is not defined"),
                }
            }
        }
    }
}
