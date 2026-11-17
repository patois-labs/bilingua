//! A small lossless XML tree. Every node keeps its original bytes, so writing the tree back gives the input
//! exactly. Only what OOXML needs: no DOCTYPE, no entity declarations.

use std::collections::HashMap;
use std::sync::Arc;

pub const MAX_DEPTH: usize = 200;

#[derive(Debug, Clone)]
pub enum Node {
    /// Character data (entities still escaped) or a CDATA section.
    Text(String),
    /// Comment or processing instruction, kept as is.
    Raw(String),
    El(El),
}

#[derive(Debug, Clone, Default)]
pub struct El {
    pub name: Tag,
    /// The start tag as written, e.g. `<w:p w:rsidR="00A1">`.
    pub start: Tag,
    /// The end tag as written; empty for a self-closing element.
    pub end: Tag,
    pub children: Vec<Node>,
}

/// A tag string as written (an element name, a start tag or an end tag), shared by every element of one parse that
/// writes it the same way: a large part repeats `<w:r>`, `</w:t>` and `<w:t xml:space="preserve">` millions of
/// times, and each copy costs one reference count instead of one allocation. Reads like a `&str`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag(Arc<str>);

impl Tag {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Tag {
    fn default() -> Tag {
        Tag(Arc::from(""))
    }
}

impl std::ops::Deref for Tag {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Tag {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for Tag {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.0, f)
    }
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Tag {
    fn from(s: String) -> Tag {
        Tag(Arc::from(s))
    }
}

impl From<&str> for Tag {
    fn from(s: &str) -> Tag {
        Tag(Arc::from(s))
    }
}

impl From<&String> for Tag {
    fn from(s: &String) -> Tag {
        Tag(Arc::from(s.as_str()))
    }
}

impl From<Tag> for String {
    fn from(t: Tag) -> String {
        t.0.to_string()
    }
}

impl PartialEq<str> for Tag {
    fn eq(&self, o: &str) -> bool {
        &*self.0 == o
    }
}

impl PartialEq<&str> for Tag {
    fn eq(&self, o: &&str) -> bool {
        &*self.0 == *o
    }
}

impl PartialEq<String> for Tag {
    fn eq(&self, o: &String) -> bool {
        *self.0 == **o
    }
}

impl PartialEq<Tag> for str {
    fn eq(&self, o: &Tag) -> bool {
        self == &*o.0
    }
}

impl PartialEq<Tag> for &str {
    fn eq(&self, o: &Tag) -> bool {
        *self == &*o.0
    }
}

impl PartialEq<Tag> for String {
    fn eq(&self, o: &Tag) -> bool {
        **self == *o.0
    }
}

/// One [`Tag`] per distinct string within a parse.
#[derive(Default)]
struct Interner<'a>(HashMap<&'a str, Tag>);

impl<'a> Interner<'a> {
    fn get(&mut self, s: &'a str) -> Tag {
        self.0.entry(s).or_insert_with(|| Tag::from(s)).clone()
    }
}

impl Node {
    pub fn write_raw(&self, out: &mut String) {
        match self {
            Node::Text(s) | Node::Raw(s) => out.push_str(s),
            Node::El(e) => e.write_raw(out),
        }
    }
}

impl El {
    pub fn write_raw(&self, out: &mut String) {
        out.push_str(&self.start);
        for c in &self.children {
            c.write_raw(out);
        }
        out.push_str(&self.end);
    }
    pub fn raw(&self) -> String {
        let mut s = String::new();
        self.write_raw(&mut s);
        s
    }
    pub fn attrs(&self) -> Vec<(String, String)> {
        parse_attrs(&self.start)
    }
    pub fn attr(&self, name: &str) -> Option<String> {
        self.attrs()
            .into_iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
    pub fn elements(&self) -> impl Iterator<Item = &El> {
        self.children
            .iter()
            .filter_map(|n| if let Node::El(e) = n { Some(e) } else { None })
    }
    /// Concatenated character data of the direct text children, unescaped.
    pub fn text(&self) -> String {
        let mut s = String::new();
        for c in &self.children {
            if let Node::Text(t) = c {
                s.push_str(&unescape(t));
            }
        }
        s
    }
}

pub fn local_name(name: &str) -> &str {
    name.rsplit_once(':').map(|(_, l)| l).unwrap_or(name)
}

/// The prefix (with colon, or empty) a document uses for one namespace.
#[derive(Debug, Clone)]
pub struct Ns {
    pub pfx: String,
}

impl Ns {
    pub fn is(&self, name: &str, local: &str) -> bool {
        name.strip_prefix(self.pfx.as_str()).is_some_and(|r| r == local)
    }
    /// Finds the prefix bound to one of `uris` on the root element; `fallback` (with colon) otherwise.
    pub fn detect(nodes: &[Node], uris: &[&str], fallback: &str) -> Ns {
        let mut pfx = fallback.to_string();
        for n in nodes {
            if let Node::El(root) = n {
                for (k, v) in root.attrs() {
                    if uris.contains(&v.as_str()) {
                        if k == "xmlns" {
                            pfx = String::new();
                        } else if let Some(p) = k.strip_prefix("xmlns:") {
                            pfx = format!("{p}:");
                        }
                    }
                }
                break;
            }
        }
        Ns { pfx }
    }
}

pub fn parse(s: &str) -> Result<Vec<Node>, String> {
    let b = s.as_bytes();
    let n = b.len();
    let mut i = 0;
    let mut tags = Interner::default();
    // The open elements, each with where its children start in `kids`: the children of every open element wait on
    // one shared stack and move into an exact-size Vec when their element closes (no growth slack per element).
    let mut stack: Vec<(El, usize)> = Vec::new();
    let mut kids: Vec<Node> = Vec::new();
    let mut top: Vec<Node> = Vec::new();
    fn push(stack: &[(El, usize)], kids: &mut Vec<Node>, top: &mut Vec<Node>, node: Node) {
        if stack.is_empty() {
            top.push(node);
        } else {
            kids.push(node);
        }
    }
    while i < n {
        if b[i] != b'<' {
            let j = s[i..].find('<').map(|k| i + k).unwrap_or(n);
            push(&stack, &mut kids, &mut top, Node::Text(s[i..j].to_string()));
            i = j;
            continue;
        }
        let rest = &s[i..];
        if rest.starts_with("<!--") {
            let e = rest.find("-->").ok_or("unterminated comment")? + 3;
            push(&stack, &mut kids, &mut top, Node::Raw(rest[..e].to_string()));
            i += e;
        } else if rest.starts_with("<![CDATA[") {
            let e = rest.find("]]>").ok_or("unterminated CDATA section")? + 3;
            push(&stack, &mut kids, &mut top, Node::Text(rest[..e].to_string()));
            i += e;
        } else if rest.starts_with("<?") {
            let e = rest
                .find("?>")
                .ok_or("unterminated processing instruction")?
                + 2;
            push(&stack, &mut kids, &mut top, Node::Raw(rest[..e].to_string()));
            i += e;
        } else if rest.starts_with("<!") {
            return Err("DOCTYPE and entity declarations are not allowed".into());
        } else if rest.starts_with("</") {
            let e = rest.find('>').ok_or("unterminated end tag")?;
            let name = rest[2..e].trim();
            let (mut el, mark) = stack
                .pop()
                .ok_or_else(|| format!("unexpected end tag </{name}>"))?;
            if el.name != name {
                return Err(format!("end tag </{name}> does not match <{}>", el.name));
            }
            el.end = tags.get(&rest[..=e]);
            el.children = kids.drain(mark..).collect();
            push(&stack, &mut kids, &mut top, Node::El(el));
            i += e + 1;
        } else {
            let e = tag_end(rest)?;
            let raw = &rest[..=e];
            let name_end = raw[1..]
                .char_indices()
                .find(|&(_, c)| c.is_whitespace() || c == '/' || c == '>')
                .map_or(raw.len(), |(k, _)| k + 1);
            let name = &raw[1..name_end];
            if name.is_empty() {
                return Err("an element has no name".into());
            }
            let el = El {
                name: tags.get(name),
                start: tags.get(raw),
                end: tags.get(""),
                children: Vec::new(),
            };
            if raw.ends_with("/>") {
                push(&stack, &mut kids, &mut top, Node::El(el));
            } else {
                if stack.len() >= MAX_DEPTH {
                    return Err(format!("elements nested deeper than {MAX_DEPTH}"));
                }
                stack.push((el, kids.len()));
            }
            i += e + 1;
        }
    }
    if let Some((el, _)) = stack.last() {
        return Err(format!("element <{}> is not closed", el.name));
    }
    Ok(top)
}

fn tag_end(rest: &str) -> Result<usize, String> {
    let mut quote = 0u8;
    for (k, c) in rest.bytes().enumerate().skip(1) {
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Ok(k);
        }
    }
    Err("unterminated start tag".into())
}

pub fn parse_attrs(start: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b = start.as_bytes();
    let mut i = 1;
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
        let ks = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() && b[i] != b'>' {
            i += 1;
        }
        let key = &start[ks..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            continue;
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
            break;
        }
        let q = b[i];
        i += 1;
        let vs = i;
        while i < b.len() && b[i] != q {
            i += 1;
        }
        out.push((key.to_string(), unescape(&start[vs..i.min(b.len())])));
        i += 1;
    }
    out
}

pub fn unescape(s: &str) -> String {
    if let Some(inner) = s
        .strip_prefix("<![CDATA[")
        .and_then(|r| r.strip_suffix("]]>"))
    {
        return inner.to_string();
    }
    if !s.contains('&') {
        return s.to_string();
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
                .or_else(|| ent.strip_prefix("#X"))
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
    out
}

/// Escapes text for element content. Characters XML 1.0 cannot hold are dropped.
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_writes_back_exactly() {
        let s = "\u{feff}<?xml version=\"1.0\"?>\n<a x='1' y=\"2>\"><!-- c --><b/>t &amp; u<c>\n</c></a>\n";
        let nodes = parse(s).unwrap();
        let mut out = String::new();
        for n in &nodes {
            n.write_raw(&mut out);
        }
        assert_eq!(out, s);
    }

    #[test]
    fn refuses_doctype_and_mismatch() {
        assert!(parse("<!DOCTYPE a [<!ENTITY x 'y'>]><a/>").is_err());
        assert!(parse("<a><b></a></b>").is_err());
        assert!(parse("<a>").is_err());
        let deep = "<a>".repeat(300) + &"</a>".repeat(300);
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn entities() {
        assert_eq!(
            unescape("a &amp; b &#x65E5; &#26412; &foo; &"),
            "a & b 日 本 &foo; &"
        );
        assert_eq!(unescape("<![CDATA[x < y]]>"), "x < y");
        assert_eq!(escape_text("a<b&c\u{1}"), "a&lt;b&amp;c");
    }

    #[test]
    fn attributes() {
        let a = parse_attrs("<w:lang w:val=\"es-ES\" w:eastAsia='ja-JP'/>");
        assert_eq!(
            a,
            vec![
                ("w:val".into(), "es-ES".into()),
                ("w:eastAsia".into(), "ja-JP".into())
            ]
        );
    }
}
