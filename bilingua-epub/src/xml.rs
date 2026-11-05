//! A small, strict XML reader for the package parts of an EPUB (container, OPF, NCX, encryption). It builds a flat
//! element table with byte ranges into the decoded text, so the writer can replace one text span or one attribute
//! value and leave every other byte alone. It reads no DTD, expands no entity (only the five predefined ones and
//! character references are allowed, as libxml2 without a DTD), and is iterative: deep nesting is a typed error,
//! never a stack overflow.

pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

#[derive(Debug)]
pub enum XmlError {
    /// Not well-formed; the text says where.
    Syntax(String),
    /// Nested deeper than the caller's limit.
    Depth,
}

fn syn<T>(msg: impl Into<String>) -> Result<T, XmlError> {
    Err(XmlError::Syntax(msg.into()))
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub prefix: String,
    pub local: String,
    pub ns: String,
    /// Decoded value (entities resolved, whitespace characters turned into spaces).
    pub value: String,
    /// Byte range of the value between the quotes.
    pub val_start: usize,
    pub val_end: usize,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub prefix: String,
    pub local: String,
    pub ns: String,
    pub attrs: Vec<Attr>,
    pub parent: Option<usize>,
    /// Element children, in order.
    pub children: Vec<usize>,
    /// A comment or processing instruction sits directly inside (lxml counts these as children).
    pub has_other: bool,
    /// Byte offset of `<` of the start tag.
    pub start: usize,
    /// Byte offset just past the qualified name inside the start tag.
    pub name_end: usize,
    /// Byte offset just past `>` of the start tag (also the start of the content).
    pub open_end: usize,
    /// Byte offset of `</` of the end tag; equal to `open_end` for an empty element.
    pub close_start: usize,
}

#[derive(Debug, Default)]
pub struct Dom {
    pub nodes: Vec<Node>,
    pub root: usize,
}

impl Node {
    pub fn attr(&self, local: &str) -> Option<&Attr> {
        self.attrs.iter().find(|a| a.ns.is_empty() && a.prefix.is_empty() && a.local == local)
    }
    pub fn attr_ns(&self, ns: &str, local: &str) -> Option<&Attr> {
        self.attrs.iter().find(|a| a.ns == ns && a.local == local)
    }
    pub fn is(&self, ns: &str, local: &str) -> bool {
        self.local == local && self.ns == ns
    }
}

impl Dom {
    pub fn node(&self, i: usize) -> &Node {
        &self.nodes[i]
    }
    /// Every element in document order (the table is already in start-tag order).
    pub fn all(&self) -> impl Iterator<Item = usize> {
        0..self.nodes.len()
    }
    /// First element child with this name.
    pub fn child(&self, parent: usize, ns: &str, local: &str) -> Option<usize> {
        self.nodes[parent].children.iter().copied().find(|&c| self.nodes[c].is(ns, local))
    }
    /// Descendants (not the node itself) in document order.
    pub fn descendants(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        // Nodes are stored in start-tag order, so a node's descendants are the following nodes that start before
        // its end tag.
        let end = self.nodes[i].close_start.max(self.nodes[i].open_end);
        let from = i + 1;
        let to = self.nodes[from..].iter().position(|n| n.start >= end).map_or(self.nodes.len(), |p| from + p);
        from..to
    }
}

struct Scope {
    /// (prefix, uri) pairs, innermost last.
    binds: Vec<(String, String)>,
    /// Number of bindings each open element added.
    marks: Vec<usize>,
}

impl Scope {
    fn resolve(&self, prefix: &str) -> Option<&str> {
        if prefix == "xml" {
            return Some(XML_NS);
        }
        self.binds.iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| u.as_str())
    }
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == ':' || (c as u32) >= 0xC0 && !c.is_whitespace()
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == '-' || c == '.' || (c as u32) == 0xB7
}

fn is_xml_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn valid_xml_char(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// Checks that every `&` in `s` starts a predefined entity or a valid character reference, and that `]]>` does
/// not occur (`text` only).
fn check_refs(s: &str, text: bool) -> Result<(), XmlError> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'&' => {
                let Some(semi) = s[i..].find(';').map(|p| p + i) else { return syn("a bare & in the text") };
                if semi - i > 12 {
                    return syn("a bare & in the text");
                }
                let body = &s[i + 1..semi];
                let ok = match body {
                    "amp" | "lt" | "gt" | "quot" | "apos" => true,
                    _ => match body.strip_prefix('#') {
                        Some(h) if h.starts_with('x') => u32::from_str_radix(&h[1..], 16).is_ok_and(valid_xml_char),
                        Some(d) => !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit()) && d.parse::<u32>().is_ok_and(valid_xml_char),
                        None => false,
                    },
                };
                if !ok {
                    return syn(format!("entity &{body}; is not defined"));
                }
                i = semi + 1;
            }
            b']' if text && s[i..].starts_with("]]>") => return syn("]]> in character data"),
            _ => i += 1,
        }
    }
    Ok(())
}

/// Resolves the predefined entities and character references (already checked) and turns CR LF and CR into LF.
pub fn decode(s: &str) -> String {
    if !s.contains(['&', '\r']) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        match c {
            '\r' => {
                if it.peek().map(|(_, n)| *n) == Some('\n') {
                    it.next();
                }
                out.push('\n');
            }
            '&' => {
                if let Some(semi) = s[i..].find(';').map(|p| p + i) {
                    let body = &s[i + 1..semi];
                    let ch = match body {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => body.strip_prefix('#').and_then(|h| match h.strip_prefix('x') {
                            Some(x) => u32::from_str_radix(x, 16).ok(),
                            None => h.parse::<u32>().ok(),
                        }).and_then(char::from_u32),
                    };
                    if let Some(ch) = ch {
                        out.push(ch);
                        while it.peek().is_some_and(|(j, _)| *j <= semi) {
                            it.next();
                        }
                        continue;
                    }
                }
                out.push('&');
            }
            c => out.push(c),
        }
    }
    out
}

/// Attribute-value normalisation: references resolved, tab and line breaks become spaces.
fn decode_attr(raw: &str) -> String {
    let d = decode(raw);
    if d.contains(['\t', '\n']) {
        d.replace(['\t', '\n'], " ")
    } else {
        d
    }
}

/// Escapes text for element content (`&`, `<`, `>`, and CR so it survives parsing).
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    out
}

/// Escapes an attribute value written between double quotes.
pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    out
}

fn find_from(s: &str, from: usize, pat: &str) -> Option<usize> {
    s.get(from..)?.find(pat).map(|p| p + from)
}

/// Reads a whole XML document. `max_depth` bounds element nesting.
pub fn parse(text: &str, max_depth: usize) -> Result<Dom, XmlError> {
    let b = text.as_bytes();
    let mut dom = Dom::default();
    let mut stack: Vec<usize> = Vec::new();
    let mut scope = Scope { binds: Vec::new(), marks: Vec::new() };
    let mut i = 0;
    let mut seen_root = false;
    let mut root_closed = false;
    while i < b.len() {
        if b[i] != b'<' {
            let next = text[i..].find('<').map_or(b.len(), |p| p + i);
            let chunk = &text[i..next];
            if stack.is_empty() {
                if !chunk.bytes().all(is_xml_ws) {
                    return syn(if seen_root { "content after the root element" } else { "text before the root element" });
                }
            } else {
                check_refs(chunk, true)?;
            }
            i = next;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<!--") {
            let Some(e) = find_from(text, i + 4, "-->") else { return syn("unterminated comment") };
            if text[i + 4..e].contains("--") {
                return syn("-- inside a comment");
            }
            if let Some(&top) = stack.last() {
                dom.nodes[top].has_other = true;
            }
            i = e + 3;
        } else if rest.starts_with("<![CDATA[") {
            if stack.is_empty() {
                return syn("CDATA outside the root element");
            }
            let Some(e) = find_from(text, i + 9, "]]>") else { return syn("unterminated CDATA section") };
            i = e + 3;
        } else if rest.starts_with("<?") {
            let Some(e) = find_from(text, i + 2, "?>") else { return syn("unterminated processing instruction") };
            if let Some(&top) = stack.last() {
                dom.nodes[top].has_other = true;
            } else if i > 0 && text[i + 2..].starts_with("xml") && text[i + 5..].starts_with(|c: char| c.is_whitespace() || c == '?') {
                return syn("XML declaration not at the start of the file");
            }
            i = e + 2;
        } else if rest.as_bytes().get(..9).is_some_and(|h| h.eq_ignore_ascii_case(b"<!DOCTYPE")) {
            if seen_root || !stack.is_empty() {
                return syn("DOCTYPE after the root element");
            }
            let (mut j, mut quote, mut subset) = (i + 9, 0u8, 0usize);
            while j < b.len() {
                let c = b[j];
                if quote != 0 {
                    if c == quote {
                        quote = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    quote = c;
                } else if c == b'[' {
                    subset += 1;
                } else if c == b']' {
                    subset = subset.saturating_sub(1);
                } else if c == b'>' && subset == 0 {
                    break;
                }
                j += 1;
            }
            if j >= b.len() {
                return syn("unterminated DOCTYPE");
            }
            i = j + 1;
        } else if rest.starts_with("<!") {
            return syn("unexpected declaration");
        } else if rest.starts_with("</") {
            let Some(&top) = stack.last() else { return syn("end tag with no open element") };
            let Some(gt) = find_from(text, i + 2, ">") else { return syn("unterminated end tag") };
            let name = text[i + 2..gt].trim_end_matches(|c: char| c.is_ascii_whitespace());
            let n = &dom.nodes[top];
            let want_len = if n.prefix.is_empty() { n.local.len() } else { n.prefix.len() + 1 + n.local.len() };
            if name.len() != want_len || !text[n.start + 1..n.name_end].eq(name) {
                return syn(format!("end tag </{name}> does not match <{}>", &text[n.start + 1..n.name_end]));
            }
            dom.nodes[top].close_start = i;
            stack.pop();
            let added = scope.marks.pop().unwrap_or(0);
            scope.binds.truncate(scope.binds.len() - added);
            if stack.is_empty() {
                root_closed = true;
            }
            i = gt + 1;
        } else {
            // start tag
            if root_closed {
                return syn("a second root element");
            }
            let mut j = i + 1;
            let name_start = j;
            while j < b.len() && !is_xml_ws(b[j]) && b[j] != b'>' && b[j] != b'/' {
                j += 1;
            }
            let qname = &text[name_start..j];
            let name_end = j;
            if qname.is_empty() || !qname.starts_with(is_name_start) || !qname.chars().all(is_name_char) {
                return syn(format!("bad element name {qname:?}"));
            }
            let mut raw_attrs: Vec<(usize, usize, usize, usize, usize, &str)> = Vec::new(); // start, end, name_end, vs, ve
            let self_closed;
            let open_end;
            loop {
                while j < b.len() && is_xml_ws(b[j]) {
                    j += 1;
                }
                if j >= b.len() {
                    return syn("unterminated start tag");
                }
                if b[j] == b'>' {
                    self_closed = false;
                    open_end = j + 1;
                    break;
                }
                if b[j] == b'/' {
                    if b.get(j + 1) != Some(&b'>') {
                        return syn("stray / in a start tag");
                    }
                    self_closed = true;
                    open_end = j + 2;
                    break;
                }
                let a_start = j;
                while j < b.len() && !is_xml_ws(b[j]) && b[j] != b'=' && b[j] != b'>' && b[j] != b'/' {
                    j += 1;
                }
                let aname = &text[a_start..j];
                if aname.is_empty() || !aname.starts_with(is_name_start) || !aname.chars().all(is_name_char) {
                    return syn(format!("bad attribute name {aname:?}"));
                }
                let a_name_end = j;
                while j < b.len() && is_xml_ws(b[j]) {
                    j += 1;
                }
                if b.get(j) != Some(&b'=') {
                    return syn(format!("attribute {aname} has no value"));
                }
                j += 1;
                while j < b.len() && is_xml_ws(b[j]) {
                    j += 1;
                }
                let q = match b.get(j) {
                    Some(&q) if q == b'"' || q == b'\'' => q,
                    _ => return syn(format!("attribute {aname} is not quoted")),
                };
                let vs = j + 1;
                let Some(ve) = text[vs..].find(q as char).map(|p| p + vs) else { return syn("unterminated attribute value") };
                if text[vs..ve].contains('<') {
                    return syn("< in an attribute value");
                }
                check_refs(&text[vs..ve], false)?;
                j = ve + 1;
                if !(j >= b.len() || is_xml_ws(b[j]) || b[j] == b'>' || b[j] == b'/') {
                    return syn("no space between attributes");
                }
                raw_attrs.push((a_start, j, a_name_end, vs, ve, aname));
            }
            // namespace declarations first
            let mut added = 0;
            for &(_, _, _, vs, ve, aname) in &raw_attrs {
                if aname == "xmlns" {
                    scope.binds.push((String::new(), decode_attr(&text[vs..ve])));
                    added += 1;
                } else if let Some(p) = aname.strip_prefix("xmlns:") {
                    scope.binds.push((p.to_string(), decode_attr(&text[vs..ve])));
                    added += 1;
                }
            }
            let (prefix, local) = match qname.split_once(':') {
                Some((p, l)) if !p.is_empty() && !l.is_empty() && !l.contains(':') => (p, l),
                Some(_) => return syn(format!("bad qualified name {qname:?}")),
                None => ("", qname),
            };
            let ns = if prefix.is_empty() {
                scope.resolve("").unwrap_or("").to_string()
            } else {
                match scope.resolve(prefix) {
                    Some(u) => u.to_string(),
                    None => return syn(format!("namespace prefix {prefix} is not declared")),
                }
            };
            let mut attrs: Vec<Attr> = Vec::with_capacity(raw_attrs.len());
            for &(_, _, _, vs, ve, aname) in &raw_attrs {
                let (ap, al) = match aname.split_once(':') {
                    Some((p, l)) if !p.is_empty() && !l.is_empty() => (p, l),
                    Some(_) => return syn(format!("bad attribute name {aname:?}")),
                    None => ("", aname),
                };
                let ans = if ap.is_empty() || ap == "xmlns" {
                    String::new()
                } else {
                    match scope.resolve(ap) {
                        Some(u) => u.to_string(),
                        None => return syn(format!("namespace prefix {ap} is not declared")),
                    }
                };
                if attrs.iter().any(|x| x.local == al && x.ns == ans) {
                    return syn(format!("attribute {aname} appears twice"));
                }
                attrs.push(Attr {
                    prefix: ap.to_string(),
                    local: al.to_string(),
                    ns: ans,
                    value: decode_attr(&text[vs..ve]),
                    val_start: vs,
                    val_end: ve,
                });
            }
            let idx = dom.nodes.len();
            let parent = stack.last().copied();
            if let Some(p) = parent {
                dom.nodes[p].children.push(idx);
            } else {
                dom.root = idx;
                seen_root = true;
            }
            dom.nodes.push(Node {
                prefix: prefix.to_string(),
                local: local.to_string(),
                ns,
                attrs,
                parent,
                children: Vec::new(),
                has_other: false,
                start: i,
                name_end,
                open_end,
                close_start: open_end,
            });
            if self_closed {
                scope.binds.truncate(scope.binds.len() - added);
                if parent.is_none() {
                    root_closed = true;
                }
            } else {
                if stack.len() >= max_depth {
                    return Err(XmlError::Depth);
                }
                stack.push(idx);
                scope.marks.push(added);
            }
            i = open_end;
        }
    }
    if !stack.is_empty() {
        return syn("the document ends inside an element");
    }
    if !seen_root {
        return syn("no root element");
    }
    Ok(dom)
}

/// The decoded text of an element whose content is only text and CDATA sections: `None` when it holds child
/// elements, comments or processing instructions.
pub fn text_only(dom: &Dom, text: &str, i: usize) -> Option<String> {
    let n = &dom.nodes[i];
    if !n.children.is_empty() || n.has_other {
        return None;
    }
    let raw = &text[n.open_end..n.close_start];
    if !raw.contains("<![CDATA[") {
        return Some(decode(raw));
    }
    let mut out = String::new();
    let mut rest = raw;
    while let Some(p) = rest.find("<![CDATA[") {
        out.push_str(&decode(&rest[..p]));
        let after = &rest[p + 9..];
        let e = after.find("]]>")?;
        out.push_str(&after[..e].replace("\r\n", "\n").replace('\r', "\n"));
        rest = &after[e + 3..];
    }
    out.push_str(&decode(rest));
    Some(out)
}

/// Splits text into leading whitespace, core and trailing whitespace the way Python's `str.strip` does.
pub fn split_ws(s: &str) -> (&str, &str, &str) {
    let core = s.trim();
    if core.is_empty() {
        return (s, "", "");
    }
    let lead = &s[..s.len() - s.trim_start().len()];
    let trail = &s[s.trim_end().len()..];
    (lead, core, trail)
}

/// A set of non-overlapping text replacements `(start, end, replacement)` applied back to front.
#[derive(Default)]
pub struct Edits(Vec<(usize, usize, String)>);

impl Edits {
    pub fn replace(&mut self, start: usize, end: usize, with: String) {
        self.0.push((start, end, with));
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn apply(mut self, text: &str) -> String {
        self.0.sort_by_key(|e| std::cmp::Reverse(e.0));
        let mut out = text.to_string();
        for (a, b, r) in self.0 {
            out.replace_range(a..b, &r);
        }
        out
    }
}

/// Sets (or adds, when `add`) an attribute with no namespace prefix on a parsed element. `qname` is the attribute
/// name as written (`xml:lang`, `page-progression-direction`). Returns false when nothing changed.
pub fn set_attr(edits: &mut Edits, node: &Node, prefix: &str, local: &str, ns: &str, value: &str, add: bool) -> bool {
    let found = node.attrs.iter().find(|a| a.local == local && (if ns.is_empty() { a.prefix.is_empty() } else { a.ns == ns }));
    match found {
        Some(a) if a.value == value => false,
        Some(a) => {
            edits.replace(a.val_start, a.val_end, escape_attr(value));
            true
        }
        None if add => {
            let name = if prefix.is_empty() { local.to_string() } else { format!("{prefix}:{local}") };
            edits.replace(node.name_end, node.name_end, format!(" {name}=\"{}\"", escape_attr(value)));
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_namespaces_and_spans() {
        let t = "<?xml version=\"1.0\"?>\n<a xmlns=\"u:a\" xmlns:d=\"u:d\"><d:t id=\"x\">Hi &amp; <![CDATA[<you>]]></d:t><e/></a>";
        let d = parse(t, 10).unwrap();
        let n = &d.nodes[1];
        assert_eq!((n.ns.as_str(), n.local.as_str()), ("u:d", "t"));
        assert_eq!(text_only(&d, t, 1).unwrap(), "Hi & <you>");
        assert_eq!(d.nodes[0].ns, "u:a");
        assert_eq!(d.nodes[2].local, "e");
        assert_eq!(&t[d.nodes[1].open_end..d.nodes[1].close_start], "Hi &amp; <![CDATA[<you>]]>");
    }

    #[test]
    fn refuses_what_libxml_refuses() {
        for bad in ["<a", "<a></b>", "<a>&nbsp;</a>", "<a/><b/>", "x<a/>", "<a x=1/>", "<a x='1' x='2'/>", "<p:a/>", "<a>&#0;</a>", "<a>]]></a>", ""] {
            assert!(parse(bad, 10).is_err(), "{bad:?}");
        }
        assert!(matches!(parse("<a><b><c></c></b></a>", 2), Err(XmlError::Depth)));
    }

    #[test]
    fn descendants_follow_start_order() {
        let t = "<r><a><b/><c/></a><d/></r>";
        let d = parse(t, 10).unwrap();
        assert_eq!(d.descendants(1).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(d.descendants(0).count(), 4);
    }
}
