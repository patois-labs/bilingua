//! A small lossless XML tree. Every node keeps its original bytes, so writing the tree back gives the input
//! exactly. Only what OOXML needs: no DOCTYPE, no entity declarations.

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
    pub name: String,
    /// The start tag as written, e.g. `<w:p w:rsidR="00A1">`.
    pub start: String,
    /// The end tag as written; empty for a self-closing element.
    pub end: String,
    pub children: Vec<Node>,
    /// Paragraph number within the part (1-based), set by [`number`].
    pub ord: usize,
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
        self.attrs().into_iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }
    pub fn elements(&self) -> impl Iterator<Item = &El> {
        self.children.iter().filter_map(|n| if let Node::El(e) = n { Some(e) } else { None })
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

/// Namespace prefixes of the main WordprocessingML and markup-compatibility namespaces, with the colon.
#[derive(Debug, Clone)]
pub struct Ns {
    pub w: String,
    pub mc: String,
}

impl Ns {
    pub fn default_ns() -> Ns {
        Ns { w: "w:".into(), mc: "mc:".into() }
    }
    pub fn is_w(&self, name: &str, local: &str) -> bool {
        name.strip_prefix(self.w.as_str()).is_some_and(|r| r == local)
    }
    pub fn is_mc(&self, name: &str, local: &str) -> bool {
        name.strip_prefix(self.mc.as_str()).is_some_and(|r| r == local)
    }
    pub fn attr(&self, e: &El, local: &str) -> Option<String> {
        let full = format!("{}{}", self.w, local);
        e.attr(&full)
    }
    pub fn detect(nodes: &[Node]) -> Ns {
        let mut ns = Ns::default_ns();
        for n in nodes {
            if let Node::El(root) = n {
                for (k, v) in root.attrs() {
                    let wml = v == "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                        || v == "http://purl.oclc.org/ooxml/wordprocessingml/main";
                    let mcns = v == "http://schemas.openxmlformats.org/markup-compatibility/2006";
                    if wml {
                        if k == "xmlns" {
                            ns.w = String::new();
                        } else if let Some(p) = k.strip_prefix("xmlns:") {
                            ns.w = format!("{p}:");
                        }
                    }
                    if mcns {
                        if let Some(p) = k.strip_prefix("xmlns:") {
                            ns.mc = format!("{p}:");
                        }
                    }
                }
                break;
            }
        }
        ns
    }
}

pub fn parse(s: &str) -> Result<Vec<Node>, String> {
    let b = s.as_bytes();
    let n = b.len();
    let mut i = 0;
    let mut stack: Vec<El> = Vec::new();
    let mut top: Vec<Node> = Vec::new();
    fn push(stack: &mut [El], top: &mut Vec<Node>, node: Node) {
        match stack.last_mut() {
            Some(p) => p.children.push(node),
            None => top.push(node),
        }
    }
    while i < n {
        if b[i] != b'<' {
            let j = s[i..].find('<').map(|k| i + k).unwrap_or(n);
            push(&mut stack, &mut top, Node::Text(s[i..j].to_string()));
            i = j;
            continue;
        }
        let rest = &s[i..];
        if rest.starts_with("<!--") {
            let e = rest.find("-->").ok_or("unterminated comment")? + 3;
            push(&mut stack, &mut top, Node::Raw(rest[..e].to_string()));
            i += e;
        } else if rest.starts_with("<![CDATA[") {
            let e = rest.find("]]>").ok_or("unterminated CDATA section")? + 3;
            push(&mut stack, &mut top, Node::Text(rest[..e].to_string()));
            i += e;
        } else if rest.starts_with("<?") {
            let e = rest.find("?>").ok_or("unterminated processing instruction")? + 2;
            push(&mut stack, &mut top, Node::Raw(rest[..e].to_string()));
            i += e;
        } else if rest.starts_with("<!") {
            return Err("DOCTYPE and entity declarations are not allowed".into());
        } else if rest.starts_with("</") {
            let e = rest.find('>').ok_or("unterminated end tag")?;
            let name = rest[2..e].trim();
            let mut el = stack.pop().ok_or_else(|| format!("unexpected end tag </{name}>"))?;
            if el.name != name {
                return Err(format!("end tag </{name}> does not match <{}>", el.name));
            }
            el.end = rest[..=e].to_string();
            push(&mut stack, &mut top, Node::El(el));
            i += e + 1;
        } else {
            let e = tag_end(rest)?;
            let raw = &rest[..=e];
            let name: String = raw[1..]
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>')
                .collect();
            if name.is_empty() {
                return Err("an element has no name".into());
            }
            let el = El { name, start: raw.to_string(), ..Default::default() };
            if raw.ends_with("/>") {
                push(&mut stack, &mut top, Node::El(el));
            } else {
                if stack.len() >= MAX_DEPTH {
                    return Err(format!("elements nested deeper than {MAX_DEPTH}"));
                }
                stack.push(el);
            }
            i += e + 1;
        }
    }
    if let Some(el) = stack.last() {
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

/// Numbers every element whose name is `para` (1-based, document order).
pub fn number(nodes: &mut [Node], para: &str, counter: &mut usize) {
    for n in nodes {
        if let Node::El(e) = n {
            if e.name == para {
                *counter += 1;
                e.ord = *counter;
            }
            number(&mut e.children, para, counter);
        }
    }
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
    if let Some(inner) = s.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>")) {
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
        assert_eq!(unescape("a &amp; b &#x65E5; &#26412; &foo; &"), "a & b 日 本 &foo; &");
        assert_eq!(unescape("<![CDATA[x < y]]>"), "x < y");
        assert_eq!(escape_text("a<b&c\u{1}"), "a&lt;b&amp;c");
    }

    #[test]
    fn attributes() {
        let a = parse_attrs("<w:lang w:val=\"es-ES\" w:eastAsia='ja-JP'/>");
        assert_eq!(a, vec![("w:val".into(), "es-ES".into()), ("w:eastAsia".into(), "ja-JP".into())]);
    }
}
