//! Bilingua internal: a small XML tree for the Localization OS ports (the XLIFF family, DOCX, PPTX). It holds what lxml
//! gives LocOS's filters (elements with their resolved namespace, attributes, text, comments and processing
//! instructions), plus the byte span of every element in the decoded text, which LocOS's `cat_xliff.scan` finds for
//! its byte splice. Text and attribute values are read as an XML parser reports them (line ends normalised,
//! references resolved, attribute white space normalised, CDATA as text), so the tree serialises the way lxml
//! writes it ([`Dom::serialize`]). [`parse_noblanks`] drops ignorable white space the way python-docx's parser
//! (`remove_blank_text=True`) does.
//!
//! Not a public API: it may change in any release.

use quick_xml::events::Event;

/// Where an element sits in the decoded text (LocOS `cat_xliff.Span`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Span {
    /// `<` of the start tag.
    pub start: usize,
    /// Just after the start tag's `>`.
    pub start_end: usize,
    /// `<` of the end tag (`start_end` for an empty element).
    pub end_start: usize,
    /// Just after the end tag (`start_end` for an empty element).
    pub end: usize,
    /// Written as `<x/>`.
    pub empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Element,
    Text,
    Comment,
    Pi,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    /// Element: the qualified name as written. Pi: its target.
    pub name: String,
    /// Text, comment or PI content.
    pub value: String,
    /// Element attributes as written (namespace declarations included), values as parsed.
    pub attrs: Vec<(String, String)>,
    /// Element: its namespace URI.
    pub ns: Option<String>,
    /// Index into [`Dom::scopes`]: the namespace declarations in scope.
    pub scope: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Element: where it is in the original text; `None` for a node made after parsing.
    pub span: Option<Span>,
}

impl Node {
    fn new(kind: Kind) -> Node {
        Node { kind, name: String::new(), value: String::new(), attrs: Vec::new(), ns: None, scope: 0, parent: None, children: Vec::new(), span: None }
    }
}

/// The tree (an arena: nodes refer to each other by index).
#[derive(Debug, Clone)]
pub struct Dom {
    pub nodes: Vec<Node>,
    /// Namespace scopes: each is every (prefix, uri) in scope, innermost declaration last ("" is the default).
    pub scopes: Vec<Vec<(String, String)>>,
    pub root: usize,
}

/// Deepest nesting the tree accepts (the XML guard allows 256 on extraction; this bounds the recursive walks for
/// any skeleton handed to merge).
pub const MAX_DEPTH: usize = 1024;

/// Most namespace declarations in scope at one element. Every lookup and every scope copy costs this much, so a file
/// that declares tens of thousands of prefixes (slow-input denial of service) is refused; real files declare a few.
const MAX_SCOPE: usize = 1024;

/// Most scope entries kept for the whole document. Every element that declares a namespace keeps its own copy of the
/// scope, so a root with a thousand declarations above a million children that each declare one more would hold a
/// billion entries (memory denial of service). Real files stay far below: a large Word document holds a few hundred
/// thousand.
const MAX_SCOPE_ENTRIES: usize = 1 << 22;

#[derive(Debug)]
pub struct XmlError(pub String);

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err(msg: impl Into<String>) -> XmlError {
    XmlError(msg.into())
}

/// XML line-end normalisation (`\r\n` and a lone `\r` become `\n`).
fn line_ends(s: &str) -> String {
    if !s.contains('\r') {
        return s.to_string();
    }
    s.replace("\r\n", "\n").replace('\r', "\n")
}

fn predefined(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => return None,
    })
}

/// The number of a character reference (`#30`, `#x1E`), digits only.
fn char_ref_value(name: &str) -> Option<u32> {
    let n = name.strip_prefix('#')?;
    match n.strip_prefix('x') {
        Some(h) if !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()) => u32::from_str_radix(h, 16).ok(),
        None if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => n.parse::<u32>().ok(),
        _ => None,
    }
}

/// True for an XML 1.0 `Char`: `#x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]`.
fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

fn reference(name: &str) -> Result<char, XmlError> {
    if let Some(c) = predefined(name) {
        return Ok(c);
    }
    match char_ref_value(name) {
        // a reference to a character XML 1.0 does not allow (`&#30;`, `&#0;`) is not well-formed, as lxml says
        Some(v) => char::from_u32(v).filter(|c| is_xml_char(*c)).ok_or_else(|| err(format!("xmlParseCharRef: invalid xmlChar value {v}"))),
        None => Err(err(format!("undefined entity &{name};"))),
    }
}

/// An attribute value as a parser reports it: line ends normalised, literal tab and newline as a space,
/// references resolved.
fn attr_value(raw: &str) -> Result<String, XmlError> {
    let raw = line_ends(raw);
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw.as_str();
    while let Some(i) = rest.find(['&', '\t', '\n']) {
        out.push_str(&rest[..i]);
        if rest.as_bytes()[i] == b'&' {
            let end = rest[i..].find(';').ok_or_else(|| err("unterminated reference in an attribute"))?;
            out.push(reference(&rest[i + 1..i + end])?);
            rest = &rest[i + end + 1..];
        } else {
            out.push(' ');
            rest = &rest[i + 1..];
        }
    }
    out.push_str(rest);
    Ok(out)
}

fn split_qname(q: &str) -> (&str, &str) {
    match q.split_once(':') {
        Some((p, l)) => (p, l),
        None => ("", q),
    }
}

/// Parses decoded XML text. DOCTYPEs are refused by the caller's guard first; one here is an error too.
pub fn parse(text: &str) -> Result<Dom, XmlError> {
    parse_with(text, false)
}

/// Like [`parse`], dropping ignorable white space the way libxml2 does with `XML_PARSE_NOBLANKS` (lxml's
/// `remove_blank_text=True`, which python-docx parses every part with): character data of XML white space only that
/// is followed by markup is dropped unless `xml:space="preserve"` is in effect, it is the whole content of its
/// element, or its element already holds text.
pub fn parse_noblanks(text: &str) -> Result<Dom, XmlError> {
    parse_with(text, true)
}

/// The markup that ends a run of character data.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    End,
    Other,
}

struct Pending {
    text: String,
    /// Only character data so far (no reference, no CDATA): libxml2 judges blanks on such runs only.
    plain: bool,
}

fn flush(dom: &mut Dom, stack: &[usize], preserve: &[bool], pending: &mut Pending, noblanks: bool, next: Next) {
    let plain = std::mem::replace(&mut pending.plain, true);
    if pending.text.is_empty() {
        return;
    }
    let Some(&p) = stack.last() else {
        pending.text.clear();
        return;
    };
    if noblanks && plain && !preserve.last().copied().unwrap_or(false) && pending.text.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r')) {
        let kids = &dom.nodes[p].children;
        let text_at = |c: Option<&usize>| c.is_some_and(|&c| dom.nodes[c].kind == Kind::Text);
        let keep = (kids.is_empty() && next == Next::End) || text_at(kids.first()) || text_at(kids.last());
        if !keep {
            pending.text.clear();
            return;
        }
    }
    let mut n = Node::new(Kind::Text);
    n.value = std::mem::take(&mut pending.text);
    n.parent = Some(p);
    dom.nodes.push(n);
    let id = dom.nodes.len() - 1;
    dom.nodes[p].children.push(id);
}

fn parse_with(text: &str, noblanks: bool) -> Result<Dom, XmlError> {
    let mut r = quick_xml::Reader::from_str(text);
    r.config_mut().check_end_names = true;
    let mut dom = Dom { nodes: Vec::new(), scopes: vec![vec![("xml".into(), "http://www.w3.org/XML/1998/namespace".into())]], root: usize::MAX };
    let mut stack: Vec<usize> = Vec::new();
    let mut scope_entries = 0usize;
    // xml:space="preserve" in effect, per open element
    let mut preserve: Vec<bool> = Vec::new();
    let mut pending = Pending { text: String::new(), plain: true };
    loop {
        let before = r.buffer_position() as usize;
        let ev = r.read_event().map_err(|e| err(format!("not well-formed XML: {e}")))?;
        let after = r.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(err("the file declares a DOCTYPE")),
            Event::Decl(_) => {}
            Event::Start(ref e) | Event::Empty(ref e) => {
                flush(&mut dom, &stack, &preserve, &mut pending, noblanks, Next::Other);
                if stack.is_empty() && dom.root != usize::MAX {
                    return Err(err("more than one root element"));
                }
                let empty = matches!(ev, Event::Empty(_));
                let name = std::str::from_utf8(e.name().as_ref()).map_err(|_| err("bad element name"))?.to_string();
                let mut attrs = Vec::new();
                // sets, not scans: thousands of attributes or declarations on one tag must not cost quadratic time
                let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
                for a in e.attributes().with_checks(false) {
                    let a = a.map_err(|e| err(format!("bad attribute: {e}")))?;
                    let k = std::str::from_utf8(a.key.as_ref()).map_err(|_| err("bad attribute name"))?.to_string();
                    if !seen.insert(k.clone()) {
                        return Err(err(format!("duplicate attribute {k}")));
                    }
                    let raw = std::str::from_utf8(&a.value).map_err(|_| err("bad attribute value"))?;
                    attrs.push((k, attr_value(raw)?));
                }
                let parent_scope = stack.last().map(|&p| dom.nodes[p].scope).unwrap_or(0);
                let decls: Vec<(String, String)> = attrs
                    .iter()
                    .filter_map(|(k, v)| {
                        if k == "xmlns" {
                            Some((String::new(), v.clone()))
                        } else {
                            k.strip_prefix("xmlns:").map(|p| (p.to_string(), v.clone()))
                        }
                    })
                    .collect();
                let scope = if decls.is_empty() {
                    parent_scope
                } else {
                    let size = dom.scopes[parent_scope].len() + decls.len();
                    if size > MAX_SCOPE {
                        return Err(err(format!("more than {MAX_SCOPE} namespace declarations in scope at one element; refused")));
                    }
                    scope_entries += size;
                    if scope_entries > MAX_SCOPE_ENTRIES {
                        return Err(err(format!(
                            "too many namespace declarations: the file's namespace scopes would hold more than {MAX_SCOPE_ENTRIES} entries; refused"
                        )));
                    }
                    let mut s = dom.scopes[parent_scope].clone();
                    s.extend(decls);
                    dom.scopes.push(s);
                    dom.scopes.len() - 1
                };
                let (prefix, _) = split_qname(&name);
                let ns = dom.scopes[scope].iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| u.clone()).filter(|u| !u.is_empty());
                if !prefix.is_empty() && ns.is_none() {
                    return Err(err(format!("undeclared namespace prefix {prefix}")));
                }
                // an attribute's prefix must be bound too (libxml2's namespace error, which lxml raises)
                for (k, _) in &attrs {
                    let (p, _) = split_qname(k);
                    if !p.is_empty() && p != "xmlns" && p != "xml" && !dom.scopes[scope].iter().any(|(x, u)| x == p && !u.is_empty()) {
                        return Err(err(format!("undeclared namespace prefix {p}")));
                    }
                }
                let keep_space = match attrs.iter().find(|(k, _)| k == "xml:space").map(|(_, v)| v.as_str()) {
                    Some("preserve") => true,
                    Some("default") => false,
                    _ => preserve.last().copied().unwrap_or(false),
                };
                let mut n = Node::new(Kind::Element);
                n.name = name;
                n.attrs = attrs;
                n.ns = ns;
                n.scope = scope;
                n.parent = stack.last().copied();
                n.span = Some(Span { start: before, start_end: after, end_start: after, end: after, empty });
                dom.nodes.push(n);
                let id = dom.nodes.len() - 1;
                match stack.last() {
                    Some(&p) => dom.nodes[p].children.push(id),
                    None => dom.root = id,
                }
                if !empty {
                    stack.push(id);
                    preserve.push(keep_space);
                    if stack.len() > MAX_DEPTH {
                        return Err(err("the XML is nested too deeply"));
                    }
                }
            }
            Event::End(_) => {
                flush(&mut dom, &stack, &preserve, &mut pending, noblanks, Next::End);
                let id = stack.pop().ok_or_else(|| err("unexpected end tag"))?;
                preserve.pop();
                if let Some(sp) = dom.nodes[id].span.as_mut() {
                    sp.end_start = before;
                    sp.end = after;
                }
            }
            Event::Text(t) => {
                let s = std::str::from_utf8(&t).map_err(|_| err("bad text"))?;
                if !stack.is_empty() {
                    pending.text.push_str(&line_ends(s));
                } else if !s.trim().is_empty() {
                    return Err(err("text outside the root element"));
                }
            }
            Event::CData(t) => {
                let s = std::str::from_utf8(&t).map_err(|_| err("bad CDATA"))?;
                if noblanks {
                    // libxml2 judges the character data before a CDATA section on its own
                    flush(&mut dom, &stack, &preserve, &mut pending, noblanks, Next::Other);
                }
                pending.text.push_str(&line_ends(s));
                pending.plain = false;
            }
            Event::GeneralRef(g) => {
                let name = std::str::from_utf8(&g).map_err(|_| err("bad reference"))?;
                let c = reference(name)?;
                if !stack.is_empty() {
                    pending.text.push(c);
                    pending.plain = false;
                }
            }
            // before or after the root: lxml's tostring(root) does not write these
            Event::Comment(_) | Event::PI(_) if stack.is_empty() => {}
            Event::Comment(c) => {
                flush(&mut dom, &stack, &preserve, &mut pending, noblanks, Next::Other);
                let mut n = Node::new(Kind::Comment);
                n.value = line_ends(std::str::from_utf8(&c).map_err(|_| err("bad comment"))?);
                n.parent = stack.last().copied();
                dom.nodes.push(n);
                let id = dom.nodes.len() - 1;
                dom.nodes[*stack.last().unwrap()].children.push(id);
            }
            Event::PI(p) => {
                flush(&mut dom, &stack, &preserve, &mut pending, noblanks, Next::Other);
                let raw = line_ends(std::str::from_utf8(&p).map_err(|_| err("bad processing instruction"))?);
                let (target, data) = match raw.find(|c: char| c.is_whitespace()) {
                    Some(i) => (raw[..i].to_string(), raw[i..].trim_start().to_string()),
                    None => (raw.clone(), String::new()),
                };
                let mut n = Node::new(Kind::Pi);
                n.name = target;
                n.value = data;
                n.parent = stack.last().copied();
                dom.nodes.push(n);
                let id = dom.nodes.len() - 1;
                dom.nodes[*stack.last().unwrap()].children.push(id);
            }
        }
    }
    if !stack.is_empty() || dom.root == usize::MAX {
        return Err(err("not well-formed XML: the document ends inside an element"));
    }
    Ok(dom)
}

impl Dom {
    /// The lower-case local name of an element ("" for anything else), like LocOS's `_local`.
    pub fn local(&self, id: usize) -> String {
        let n = &self.nodes[id];
        if n.kind != Kind::Element {
            return String::new();
        }
        split_qname(&n.name).1.to_lowercase()
    }

    pub fn is_el(&self, id: usize) -> bool {
        self.nodes[id].kind == Kind::Element
    }

    /// An attribute without a namespace (`el.get("id")`).
    pub fn get(&self, id: usize, name: &str) -> Option<&str> {
        self.nodes[id].attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// An attribute in a namespace (`el.get("{ns}local")`).
    pub fn get_ns(&self, id: usize, ns: &str, local: &str) -> Option<&str> {
        self.nodes[id].attrs.iter().find(|(k, _)| self.attr_ns(id, k).as_deref() == Some(ns) && split_qname(k).1 == local).map(|(_, v)| v.as_str())
    }

    /// The namespace of an attribute name on an element (none for an unprefixed one).
    pub fn attr_ns(&self, id: usize, qname: &str) -> Option<String> {
        let (p, _) = split_qname(qname);
        if p.is_empty() || p == "xmlns" {
            return None;
        }
        self.scopes[self.nodes[id].scope].iter().rev().find(|(x, _)| x == p).map(|(_, u)| u.clone())
    }

    /// Every attribute with its namespace and local name.
    pub fn attrs_ns(&self, id: usize) -> Vec<(Option<String>, String, String)> {
        self.nodes[id].attrs.iter().filter(|(k, _)| k != "xmlns" && !k.starts_with("xmlns:")).map(|(k, v)| (self.attr_ns(id, k), split_qname(k).1.to_string(), v.clone())).collect()
    }

    /// The namespace URIs in scope (`el.nsmap.values()`), innermost declaration of each prefix.
    pub fn nsmap(&self, id: usize) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for (p, u) in self.scopes[self.nodes[id].scope].iter().skip(1) {
            out.retain(|(x, _)| x != p);
            if !u.is_empty() {
                out.push((p.clone(), u.clone()));
            }
        }
        out
    }

    /// The prefix bound to `ns` in scope ("" for the default namespace), like LocOS's `_prefix_for`.
    pub fn prefix_for(&self, id: usize, ns: &str) -> Option<String> {
        self.nsmap(id).into_iter().find(|(_, u)| u == ns).map(|(p, _)| p)
    }

    pub fn element_children(&self, id: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes[id].children.iter().copied().filter(move |&c| self.is_el(c))
    }

    /// The first element child with this local name (`_child`).
    pub fn child(&self, id: usize, local: &str) -> Option<usize> {
        self.element_children(id).find(|&c| self.local(c) == local)
    }

    /// `el.iter()`: the node and every descendant element in document order (elements only).
    pub fn iter(&self, id: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if !self.is_el(n) {
                continue;
            }
            out.push(n);
            stack.extend(self.nodes[n].children.iter().rev().copied());
        }
        out
    }

    /// `"".join(el.itertext())`: the text of the element and its descendants (comments and PIs left out).
    pub fn itertext(&self, id: usize) -> String {
        let mut out = String::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            match self.nodes[n].kind {
                Kind::Text => out.push_str(&self.nodes[n].value),
                Kind::Element => stack.extend(self.nodes[n].children.iter().rev().copied()),
                _ => {}
            }
        }
        out
    }

    /// lxml's `el.text`: the text before the first non-text child.
    pub fn lead_text(&self, id: usize) -> String {
        let mut out = String::new();
        for &c in &self.nodes[id].children {
            if self.nodes[c].kind != Kind::Text {
                break;
            }
            out.push_str(&self.nodes[c].value);
        }
        out
    }

    /// An ancestor element with this local name.
    pub fn ancestor(&self, id: usize, local: &str) -> Option<usize> {
        let mut p = self.nodes[id].parent;
        while let Some(x) = p {
            if self.local(x) == local {
                return Some(x);
            }
            p = self.nodes[x].parent;
        }
        None
    }

    /// A deep copy of a node into the arena (no parent, no span).
    pub fn deep_copy(&mut self, id: usize) -> usize {
        let mut n = self.nodes[id].clone();
        n.parent = None;
        n.span = None;
        let kids = std::mem::take(&mut n.children);
        self.nodes.push(n);
        let new = self.nodes.len() - 1;
        for k in kids {
            let c = self.deep_copy(k);
            self.append(new, c);
        }
        new
    }

    /// Appends `child` as the last child of `parent`, moving it out of its old place first (lxml `append`).
    pub fn append(&mut self, parent: usize, child: usize) {
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
    }

    /// Takes a node out of its parent (lxml `remove`); it stays in the arena and may be put back elsewhere.
    pub fn detach(&mut self, id: usize) {
        if let Some(p) = self.nodes[id].parent.take() {
            self.nodes[p].children.retain(|&c| c != id);
        }
    }

    /// Inserts `child` at position `at` among the children of `parent` (lxml `insert`), moving it first.
    pub fn insert(&mut self, parent: usize, at: usize, child: usize) {
        self.detach(child);
        let at = at.min(self.nodes[parent].children.len());
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.insert(at, child);
    }

    /// Inserts `child` just before `before` (lxml `addprevious`), moving it first.
    pub fn add_previous(&mut self, before: usize, child: usize) {
        if before == child {
            return;
        }
        self.detach(child);
        let parent = self.nodes[before].parent.expect("addprevious needs a parent");
        let at = self.nodes[parent].children.iter().position(|&k| k == before).unwrap();
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.insert(at, child);
    }

    /// The local name of an element as written (case kept; "" for anything else).
    pub fn local_name(&self, id: usize) -> &str {
        let n = &self.nodes[id];
        if n.kind != Kind::Element {
            return "";
        }
        split_qname(&n.name).1
    }

    /// True for an element in namespace `ns` with this local name (lxml's Clark-name test).
    pub fn is(&self, id: usize, ns: &str, local: &str) -> bool {
        let n = &self.nodes[id];
        n.kind == Kind::Element && n.ns.as_deref() == Some(ns) && split_qname(&n.name).1 == local
    }

    /// The first element child in `ns` with this local name (`el.find("{ns}local")`).
    pub fn find(&self, id: usize, ns: &str, local: &str) -> Option<usize> {
        self.element_children(id).find(|&c| self.is(c, ns, local))
    }

    /// The tree with attributes sorted, every element written with a start and an end tag and adjacent text
    /// joined: two trees with the same canonical form are the same XML for any reader.
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        self.write_canonical(self.root, &mut out);
        out
    }

    fn write_canonical(&self, id: usize, out: &mut String) {
        let n = &self.nodes[id];
        match n.kind {
            Kind::Text => out.push_str(&esc_text(&n.value)),
            Kind::Comment | Kind::Pi => self.write(id, out),
            Kind::Element => {
                out.push('<');
                out.push_str(&n.name);
                let mut attrs: Vec<&(String, String)> = n.attrs.iter().collect();
                attrs.sort();
                for (k, v) in attrs {
                    out.push(' ');
                    out.push_str(k);
                    out.push_str("=\"");
                    out.push_str(&esc_attr(v));
                    out.push('"');
                }
                out.push('>');
                for &c in &n.children {
                    self.write_canonical(c, out);
                }
                out.push_str("</");
                out.push_str(&n.name);
                out.push('>');
            }
        }
    }

    /// Appends text at the end of an element (joined to a text node already there).
    pub fn append_text(&mut self, parent: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(&last) = self.nodes[parent].children.last() {
            if self.nodes[last].kind == Kind::Text {
                self.nodes[last].value.push_str(text);
                return;
            }
        }
        let mut n = Node::new(Kind::Text);
        n.value = text.to_string();
        self.nodes.push(n);
        let id = self.nodes.len() - 1;
        self.append(parent, id);
    }

    /// A new text node in the arena (not attached).
    pub fn new_text(&mut self, text: &str) -> usize {
        let mut n = Node::new(Kind::Text);
        n.value = text.to_string();
        self.nodes.push(n);
        self.nodes.len() - 1
    }

    /// A new element in the arena.
    pub fn new_element(&mut self, name: &str, scope: usize, ns: Option<String>) -> usize {
        let mut n = Node::new(Kind::Element);
        n.name = name.to_string();
        n.scope = scope;
        n.ns = ns;
        self.nodes.push(n);
        self.nodes.len() - 1
    }

    /// `el.set(name, value)`: replaced in place, else added at the end.
    pub fn set(&mut self, id: usize, name: &str, value: &str) {
        let attrs = &mut self.nodes[id].attrs;
        match attrs.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = value.to_string(),
            None => attrs.push((name.to_string(), value.to_string())),
        }
    }

    /// Inserts `child` as the next sibling of `after` (lxml `addnext`): lxml keeps the text that follows `after` (its
    /// tail) with it, so the new element goes after that text, before the next non-text sibling.
    pub fn add_next(&mut self, after: usize, child: usize) {
        if after == child {
            return;
        }
        self.detach(child);
        let parent = self.nodes[after].parent.expect("addnext needs a parent");
        let kids = &self.nodes[parent].children;
        let mut at = kids.iter().position(|&k| k == after).unwrap() + 1;
        while at < kids.len() && self.nodes[kids[at]].kind == Kind::Text {
            at += 1;
        }
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.insert(at, child);
    }

    /// Removes every child (`_clear`).
    pub fn clear(&mut self, id: usize) {
        for c in std::mem::take(&mut self.nodes[id].children) {
            self.nodes[c].parent = None;
        }
        if self.nodes[id].kind != Kind::Element {
            self.nodes[id].value.clear();
        }
    }

    /// The tree as lxml's `etree.tostring(root)` writes it (UTF-8, no declaration): namespace declarations first,
    /// double-quoted attributes, `<x/>` for an element with no content, `&`, `<`, `>` and CR escaped in text.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        self.write(self.root, &mut out);
        out
    }

    /// One node and its content as lxml's `etree.tostring(el)` writes it (without the declarations an ancestor
    /// makes).
    pub fn serialize_node(&self, id: usize) -> String {
        let mut out = String::new();
        self.write(id, &mut out);
        out
    }

    fn write(&self, id: usize, out: &mut String) {
        let n = &self.nodes[id];
        match n.kind {
            Kind::Text => out.push_str(&esc_text(&n.value)),
            Kind::Comment => {
                out.push_str("<!--");
                out.push_str(&n.value);
                out.push_str("-->");
            }
            Kind::Pi => {
                out.push_str("<?");
                out.push_str(&n.name);
                if !n.value.is_empty() {
                    out.push(' ');
                    out.push_str(&n.value);
                }
                out.push_str("?>");
            }
            Kind::Element => {
                out.push('<');
                out.push_str(&n.name);
                let decl = |k: &str| k == "xmlns" || k.starts_with("xmlns:");
                for (k, v) in n.attrs.iter().filter(|(k, _)| decl(k)).chain(n.attrs.iter().filter(|(k, _)| !decl(k))) {
                    out.push(' ');
                    out.push_str(k);
                    out.push_str("=\"");
                    out.push_str(&esc_attr(v));
                    out.push('"');
                }
                let content: Vec<usize> = n.children.iter().copied().filter(|&c| !(self.nodes[c].kind == Kind::Text && self.nodes[c].value.is_empty())).collect();
                if content.is_empty() {
                    out.push_str("/>");
                    return;
                }
                out.push('>');
                for c in content {
                    self.write(c, out);
                }
                out.push_str("</");
                out.push_str(&n.name);
                out.push('>');
            }
        }
    }
}

/// The PPTX port's edits (lxml's `insert`, `remove`, `addprevious` and `copy.deepcopy`, which carry an
/// element's tail text with it).
impl Dom {
    /// The local name as written (case kept).
    pub fn local_exact(&self, id: usize) -> &str {
        split_qname(&self.nodes[id].name).1
    }

    /// The prefix the innermost declaration in scope binds to `ns` (what lxml picks for a new element).
    pub fn inner_prefix(&self, id: usize, ns: &str) -> Option<String> {
        let scope = &self.scopes[self.nodes[id].scope];
        for (i, (p, u)) in scope.iter().enumerate().rev() {
            // not shadowed by a later declaration of the same prefix
            if u == ns && !scope[i + 1..].iter().any(|(q, _)| q == p) {
                return Some(p.clone());
            }
        }
        None
    }

    /// Inserts `kids` at `index` among the children of `parent`.
    pub fn insert_at(&mut self, parent: usize, index: usize, kids: &[usize]) {
        for (k, &c) in kids.iter().enumerate() {
            self.nodes[c].parent = Some(parent);
            self.nodes[parent].children.insert(index + k, c);
        }
    }

    /// lxml's child index 0: just after the text that leads the element.
    pub fn first_slot(&self, id: usize) -> usize {
        self.nodes[id].children.iter().position(|&c| self.nodes[c].kind != Kind::Text).unwrap_or(self.nodes[id].children.len())
    }

    /// lxml `parent.remove(el)`: the element goes with its tail text.
    pub fn remove_with_tail(&mut self, id: usize) {
        let Some(parent) = self.nodes[id].parent else { return };
        let kids = &self.nodes[parent].children;
        let Some(at) = kids.iter().position(|&k| k == id) else { return };
        let mut end = at + 1;
        while end < kids.len() && self.nodes[kids[end]].kind == Kind::Text {
            end += 1;
        }
        let gone: Vec<usize> = self.nodes[parent].children.drain(at..end).collect();
        for g in gone {
            self.nodes[g].parent = None;
        }
    }

    /// lxml `copy.deepcopy(el)`: a copy of the element followed by copies of its tail text, the namespace
    /// declarations it needs at `target` (a node whose scope it will live in) added to the copy.
    pub fn copy_with_tail(&mut self, id: usize, target: usize) -> Vec<usize> {
        let copy = self.deep_copy(id);
        self.declare_for(copy, id, target);
        let mut out = vec![copy];
        if let Some(parent) = self.nodes[id].parent {
            let kids = self.nodes[parent].children.clone();
            let at = kids.iter().position(|&k| k == id).unwrap_or(kids.len());
            for &k in kids.iter().skip(at + 1) {
                if self.nodes[k].kind != Kind::Text {
                    break;
                }
                let t = self.deep_copy(k);
                out.push(t);
            }
        }
        out
    }

    /// Adds to `copy` the declarations of prefixes its subtree (a copy of `orig`) uses that `target`'s scope binds
    /// differently or not at all, as libxml2 does when it copies a node into another context.
    fn declare_for(&mut self, copy: usize, orig: usize, target: usize) {
        let resolve = |d: &Dom, scope: usize, p: &str| d.scopes[scope].iter().rev().find(|(x, _)| x == p).map(|(_, u)| u.clone());
        let mut need: Vec<(String, String)> = Vec::new();
        for e in self.iter(orig) {
            let n = &self.nodes[e];
            let mut prefixes = vec![split_qname(&n.name).0.to_string()];
            for (k, _) in &n.attrs {
                let p = split_qname(k).0;
                if !p.is_empty() && p != "xmlns" && p != "xml" {
                    prefixes.push(p.to_string());
                }
            }
            for p in prefixes {
                let uri = resolve(self, n.scope, &p).unwrap_or_default();
                if !p.is_empty() && uri.is_empty() {
                    continue;
                }
                if resolve(self, self.nodes[target].scope, &p).unwrap_or_default() == uri {
                    continue;
                }
                // declared inside the copied subtree itself: carried with it
                let key = if p.is_empty() { "xmlns".to_string() } else { format!("xmlns:{p}") };
                let inside = {
                    let mut x = Some(e);
                    let mut found = false;
                    while let Some(y) = x {
                        if self.nodes[y].attrs.iter().any(|(k, _)| *k == key) {
                            found = true;
                            break;
                        }
                        if y == orig {
                            break;
                        }
                        x = self.nodes[y].parent;
                    }
                    found
                };
                if !inside && !need.iter().any(|(k, _)| *k == key) {
                    need.push((key, uri));
                }
            }
        }
        for (k, v) in need.into_iter().rev() {
            self.nodes[copy].attrs.insert(0, (k, v));
        }
    }
}

/// Text as libxml2 escapes it.
pub fn esc_text(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '\r' => o.push_str("&#13;"),
            c => o.push(c),
        }
    }
    o
}

/// An attribute value as libxml2 escapes it.
pub fn esc_attr(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\r' => o.push_str("&#13;"),
            '\n' => o.push_str("&#10;"),
            '\t' => o.push_str("&#9;"),
            c => o.push(c),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 792: many declarations / attributes on one tag are fast, and an absurd scope is refused.
    #[test]
    fn many_namespace_declarations_are_not_quadratic() {
        let mut s = String::from("<a");
        for i in 0..40_000 {
            s.push_str(&format!(" xmlns:p{i}=\"u{i}\""));
        }
        s.push_str("/>");
        let t = std::time::Instant::now();
        assert!(parse(&s).is_err());
        let mut s = String::from("<a");
        for i in 0..40_000 {
            s.push_str(&format!(" x{i}=\"v\""));
        }
        s.push_str("/>");
        assert!(parse(&s).is_ok());
        assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
        assert!(parse("<a xmlns:p='P' p:x='1' p:x='2'/>").is_err(), "repeated attribute");
        assert!(parse("<a x='1' x='2'/>").is_err());
        let e = parse(&format!("<a{}/>", (0..=MAX_SCOPE).map(|i| format!(" xmlns:p{i}='u'")).collect::<String>())).unwrap_err();
        assert!(e.0.contains("namespace declarations in scope"), "{}", e.0);
    }

    /// B-SIDE 13: a character reference outside XML 1.0's `Char` is not well-formed (lxml's message); in text and in
    /// attributes. Allowed references still read; a malformed number is an undefined entity.
    #[test]
    fn invalid_character_references_are_refused() {
        for bad in ["&#30;", "&#x1E;", "&#0;", "&#xFFFE;", "&#xD800;", "&#x110000;"] {
            let e = parse(&format!("<a>x{bad}y</a>")).unwrap_err();
            assert!(e.0.contains("xmlParseCharRef: invalid xmlChar value"), "{bad}: {}", e.0);
            assert!(parse(&format!("<a b='{bad}'/>")).is_err(), "{bad} in an attribute");
        }
        assert_eq!(parse("<a>&#9;&#xE9;&#x10000;&#13;</a>").unwrap().serialize(), "<a>\t\u{e9}\u{10000}&#13;</a>");
        assert!(parse("<a>&#+30;</a>").unwrap_err().0.contains("undefined entity"));
    }

    /// A root with a thousand declarations above many children that each declare one more: every child copies the
    /// scope, so the total is capped (memory), while the same shape with a few declarations reads fine.
    #[test]
    fn scope_copies_are_capped_for_the_whole_document() {
        let decls: String = (0..1000).map(|i| format!(" xmlns:p{i}='u{i}'")).collect();
        let kids = "<k xmlns:q='q'/>".repeat(5000);
        let t = std::time::Instant::now();
        let e = parse(&format!("<a{decls}>{kids}</a>")).unwrap_err();
        assert!(e.0.contains("too many namespace declarations"), "{}", e.0);
        assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
        let few: String = (0..40).map(|i| format!(" xmlns:p{i}='u{i}'")).collect();
        assert!(parse(&format!("<a{few}>{kids}</a>")).is_ok());
    }

    #[test]
    fn serializes_like_lxml() {
        let src = "<xliff  version = '1.2' xmlns=\"urn:x\" xmlns:q=\"u\" ><file a=\"x&gt;y&#10;z\tw\" b=\"&apos;&quot;\"><n></n><s>A &apos;b&apos; &gt; <![CDATA[<c>]]> &#xE9;\r\n</s><!-- c --><?pi  d?></file></xliff >";
        let d = parse(src).unwrap();
        assert_eq!(
            d.serialize(),
            "<xliff xmlns=\"urn:x\" xmlns:q=\"u\" version=\"1.2\"><file a=\"x&gt;y&#10;z w\" b=\"'&quot;\"><n/><s>A 'b' &gt; &lt;c&gt; \u{e9}\n</s><!-- c --><?pi d?></file></xliff>"
        );
    }

    #[test]
    fn spans_and_namespaces() {
        let src = "<a xmlns:s=\"S\"><s:b x='1'/><c>t</c></a>";
        let d = parse(src).unwrap();
        let b = d.nodes[d.root].children[0];
        let sp = d.nodes[b].span.unwrap();
        assert_eq!(&src[sp.start..sp.end], "<s:b x='1'/>");
        assert!(sp.empty);
        assert_eq!(d.nodes[b].ns.as_deref(), Some("S"));
        let c = d.nodes[d.root].children[1];
        let sp = d.nodes[c].span.unwrap();
        assert_eq!(&src[sp.start_end..sp.end_start], "t");
        assert_eq!(d.prefix_for(c, "S").as_deref(), Some("s"));
    }

    #[test]
    fn drops_blanks_like_libxml2() {
        let src = "<a>
  <b> </b>
  <c xml:space=\"preserve\">
 <d/> </c><e>x <f/> </e><g>&#32;<h/></g>
</a>";
        let d = parse_noblanks(src).unwrap();
        assert_eq!(d.serialize(), "<a><b> </b><c xml:space=\"preserve\">
 <d/> </c><e>x <f/> </e><g> <h/></g></a>");
        // the plain parser keeps every text node
        assert_eq!(parse(src).unwrap().serialize(), src.replace("&#32;", " "));
    }

    #[test]
    fn edits_and_canonical_form() {
        let mut d = parse("<a y='2' x='1'><b/><c/></a>").unwrap();
        let (b, c) = (d.nodes[d.root].children[0], d.nodes[d.root].children[1]);
        d.add_previous(b, c);
        assert_eq!(d.serialize(), "<a y=\"2\" x=\"1\"><c/><b/></a>");
        d.add_next(c, b);
        d.insert(d.root, 0, b);
        assert_eq!(d.canonical(), "<a x=\"1\" y=\"2\"><b></b><c></c></a>");
        d.detach(b);
        d.append(c, b);
        assert_eq!(d.serialize(), "<a y=\"2\" x=\"1\"><c><b/></c></a>");
        assert!(!d.is(c, "", "c") && d.local_name(c) == "c" && d.find(d.root, "u", "c").is_none());
    }

    #[test]
    fn tail_carrying_edits_like_lxml() {
        // lxml index 0 is after the text that leads the element
        let mut d = parse("<a xmlns:p='P'>lead<p:b>x</p:b> tail <c/></a>").unwrap();
        let root = d.root;
        assert_eq!(d.first_slot(root), 1);
        let b = d.find(root, "P", "b").unwrap();
        assert_eq!(d.local_exact(b), "b");
        assert_eq!(d.inner_prefix(b, "P").as_deref(), Some("p"));
        // deepcopy takes the tail (here `p` is bound where the copy goes, so nothing is declared)
        let c = d.element_children(root).last().unwrap();
        let copies = d.copy_with_tail(b, c);
        assert_eq!(copies.len(), 2);
        d.insert_at(c, 0, &copies);
        assert_eq!(d.serialize(), "<a xmlns:p=\"P\">lead<p:b>x</p:b> tail <c><p:b>x</p:b> tail </c></a>");
        // remove takes the tail with it
        d.remove_with_tail(b);
        assert_eq!(d.serialize(), "<a xmlns:p=\"P\">lead<c><p:b>x</p:b> tail </c></a>");
        // a copy put where `p` means another namespace declares its own
        let mut d = parse("<a xmlns:p='P'><p:b/><d xmlns:p='Q'/></a>").unwrap();
        let (b, dd) = (d.nodes[d.root].children[0], d.nodes[d.root].children[1]);
        let copies = d.copy_with_tail(b, dd);
        d.insert_at(dd, 0, &copies);
        assert_eq!(d.serialize(), "<a xmlns:p=\"P\"><p:b/><d xmlns:p=\"Q\"><p:b xmlns:p=\"P\"/></d></a>");
    }

    #[test]
    fn refuses_broken_xml() {
        let deep = "<a>".repeat(MAX_DEPTH + 1);
        for bad in ["<a>", "<a></b>", "<a x='1' x='2'/>", "<p:a/>", "<a p:x='1'/>", "<a xmlns:p=''><b p:x='1'/></a>", "<a>&nope;</a>", "<!DOCTYPE a><a/>", "<a/><b/>", deep.as_str()] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(parse("<a xml:space='preserve' xml:lang='de' xmlns:p='P' p:x='1'/>").is_ok());
    }
}
