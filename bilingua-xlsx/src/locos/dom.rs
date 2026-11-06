//! Not `bilingua-xmltree` on purpose (Track D group B): this tree's edit calls treat an element and its tail as
//! one group (remove, insert, copy), a new element declares the lxml default prefix (`dc`, `ns0`) when its
//! namespace has none in scope, and element spans are not kept; the XLSX port's 30-odd call sites rely on those
//! meanings (the shared tree's `detach` and `insert` move one node), so moving would be a rewrite, not a dedupe.
//!
//! A small XML tree for the Localization OS port: what lxml gives LocOS's `xlsx_package` (elements with their
//! resolved namespace, attributes, text, comments and processing instructions) and the edits it makes. Text and
//! attribute values are read as an XML parser reports them (line ends normalised, references resolved, attribute
//! white space normalised, CDATA as text), so the tree serialises the way lxml writes it ([`Dom::serialize`]).
//!
//! lxml keeps the text after an element (its `tail`) with the element: removing, moving or copying an element
//! takes its tail along. Here text nodes are siblings, so the "tail" of an element is the run of text nodes right
//! after it, and [`Dom::remove`], [`Dom::copy_with_tail`] and the insert calls treat an element and that run as
//! one group. A new element takes the prefix its namespace already has where it is put, else declares one (lxml's
//! default prefixes: `dc` for Dublin Core, `ns0` otherwise).

use quick_xml::events::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

impl Node {
    fn new(kind: Kind) -> Node {
        Node { kind, name: String::new(), value: String::new(), attrs: Vec::new(), ns: None, scope: 0, parent: None, children: Vec::new() }
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

/// Deepest nesting the tree accepts (the XML guard allows less on extraction; this bounds the recursive walks).
pub const MAX_DEPTH: usize = 1024;

pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

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

fn char_ref(name: &str) -> Option<char> {
    let n = name.strip_prefix('#')?;
    let v = match n.strip_prefix('x') {
        Some(h) => u32::from_str_radix(h, 16).ok()?,
        None => n.parse::<u32>().ok()?,
    };
    char::from_u32(v)
}

fn reference(name: &str) -> Result<char, XmlError> {
    predefined(name).or_else(|| char_ref(name)).ok_or_else(|| err(format!("undefined entity &{name};")))
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

/// Parses decoded XML text. A DOCTYPE is an error (the caller's guard refuses one first).
pub fn parse(text: &str) -> Result<Dom, XmlError> {
    let mut r = quick_xml::Reader::from_str(text);
    r.config_mut().check_end_names = true;
    let mut dom = Dom::empty();
    let mut stack: Vec<usize> = Vec::new();
    let mut pending = String::new();
    let flush = |dom: &mut Dom, stack: &[usize], pending: &mut String| {
        if pending.is_empty() {
            return;
        }
        if let Some(&p) = stack.last() {
            let mut n = Node::new(Kind::Text);
            n.value = std::mem::take(pending);
            dom.push_child(p, n);
        } else {
            pending.clear();
        }
    };
    loop {
        let ev = r.read_event().map_err(|e| err(format!("not well-formed XML: {e}")))?;
        match ev {
            Event::Eof => break,
            Event::DocType(_) => return Err(err("the part declares a DOCTYPE")),
            Event::Decl(_) => {}
            Event::Start(ref e) | Event::Empty(ref e) => {
                flush(&mut dom, &stack, &mut pending);
                if stack.is_empty() && dom.root != usize::MAX {
                    return Err(err("more than one root element"));
                }
                let empty = matches!(ev, Event::Empty(_));
                let name = std::str::from_utf8(e.name().as_ref()).map_err(|_| err("bad element name"))?.to_string();
                let mut attrs = Vec::new();
                for a in e.attributes() {
                    let a = a.map_err(|e| err(format!("bad attribute: {e}")))?;
                    let k = std::str::from_utf8(a.key.as_ref()).map_err(|_| err("bad attribute name"))?.to_string();
                    if attrs.iter().any(|(x, _): &(String, String)| *x == k) {
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
                for (k, _) in &attrs {
                    let (p, _) = split_qname(k);
                    if !p.is_empty() && p != "xmlns" && !dom.scopes[scope].iter().any(|(x, u)| x == p && !u.is_empty()) {
                        return Err(err(format!("undeclared namespace prefix {p}")));
                    }
                }
                let mut n = Node::new(Kind::Element);
                n.name = name;
                n.attrs = attrs;
                n.ns = ns;
                n.scope = scope;
                let id = match stack.last() {
                    Some(&p) => dom.push_child(p, n),
                    None => {
                        dom.nodes.push(n);
                        dom.root = dom.nodes.len() - 1;
                        dom.root
                    }
                };
                if !empty {
                    stack.push(id);
                    if stack.len() > MAX_DEPTH {
                        return Err(err("the XML is nested too deeply"));
                    }
                }
            }
            Event::End(_) => {
                flush(&mut dom, &stack, &mut pending);
                stack.pop().ok_or_else(|| err("unexpected end tag"))?;
            }
            Event::Text(t) => {
                let s = std::str::from_utf8(&t).map_err(|_| err("bad text"))?;
                if !stack.is_empty() {
                    pending.push_str(&line_ends(s));
                } else if !s.trim().is_empty() {
                    return Err(err("text outside the root element"));
                }
            }
            Event::CData(t) => {
                let s = std::str::from_utf8(&t).map_err(|_| err("bad CDATA"))?;
                if stack.is_empty() {
                    return Err(err("CDATA outside the root element"));
                }
                pending.push_str(&line_ends(s));
            }
            Event::GeneralRef(g) => {
                let name = std::str::from_utf8(&g).map_err(|_| err("bad reference"))?;
                let c = reference(name)?;
                if stack.is_empty() {
                    return Err(err("a reference outside the root element"));
                }
                pending.push(c);
            }
            // before or after the root: lxml's tostring(root) does not write these
            Event::Comment(_) | Event::PI(_) if stack.is_empty() => {}
            Event::Comment(c) => {
                flush(&mut dom, &stack, &mut pending);
                let mut n = Node::new(Kind::Comment);
                n.value = line_ends(std::str::from_utf8(&c).map_err(|_| err("bad comment"))?);
                dom.push_child(*stack.last().unwrap(), n);
            }
            Event::PI(p) => {
                flush(&mut dom, &stack, &mut pending);
                let raw = line_ends(std::str::from_utf8(&p).map_err(|_| err("bad processing instruction"))?);
                let (target, data) = match raw.find(|c: char| c.is_whitespace()) {
                    Some(i) => (raw[..i].to_string(), raw[i..].trim_start().to_string()),
                    None => (raw.clone(), String::new()),
                };
                let mut n = Node::new(Kind::Pi);
                n.name = target;
                n.value = data;
                dom.push_child(*stack.last().unwrap(), n);
            }
        }
    }
    if !stack.is_empty() || dom.root == usize::MAX {
        return Err(err("not well-formed XML: the document ends inside an element"));
    }
    Ok(dom)
}

impl Dom {
    fn empty() -> Dom {
        Dom { nodes: Vec::new(), scopes: vec![vec![("xml".into(), XML_NS.into())]], root: usize::MAX }
    }

    /// A document holding one new root element with these namespace declarations (`etree.Element(tag, nsmap=...)`).
    pub fn with_root(ns: &str, local: &str, nsmap: &[(&str, &str)]) -> Dom {
        let mut dom = Dom::empty();
        let mut scope = dom.scopes[0].clone();
        let mut n = Node::new(Kind::Element);
        for (p, u) in nsmap {
            scope.push((p.to_string(), u.to_string()));
            n.attrs.push((if p.is_empty() { "xmlns".to_string() } else { format!("xmlns:{p}") }, u.to_string()));
        }
        dom.scopes.push(scope);
        n.scope = 1;
        let prefix = nsmap.iter().rev().find(|(_, u)| *u == ns).map(|(p, _)| p.to_string()).unwrap_or_default();
        n.name = if prefix.is_empty() { local.to_string() } else { format!("{prefix}:{local}") };
        n.ns = Some(ns.to_string());
        dom.nodes.push(n);
        dom.root = 0;
        dom
    }

    fn push_child(&mut self, parent: usize, mut n: Node) -> usize {
        n.parent = Some(parent);
        self.nodes.push(n);
        let id = self.nodes.len() - 1;
        self.nodes[parent].children.push(id);
        id
    }

    pub fn is_el(&self, id: usize) -> bool {
        self.nodes[id].kind == Kind::Element
    }

    /// The element's local name ("" for anything else).
    pub fn local(&self, id: usize) -> &str {
        let n = &self.nodes[id];
        if n.kind != Kind::Element {
            return "";
        }
        split_qname(&n.name).1
    }

    /// True for an element `{ns}local`.
    pub fn is(&self, id: usize, ns: &str, local: &str) -> bool {
        self.is_el(id) && self.nodes[id].ns.as_deref() == Some(ns) && self.local(id) == local
    }

    /// An attribute without a namespace (`el.get("t")`).
    pub fn get(&self, id: usize, name: &str) -> Option<&str> {
        self.nodes[id].attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// An attribute in a namespace (`el.get("{ns}local")`).
    pub fn get_ns(&self, id: usize, ns: &str, local: &str) -> Option<&str> {
        let scope = &self.scopes[self.nodes[id].scope];
        self.nodes[id]
            .attrs
            .iter()
            .find(|(k, _)| {
                let (p, l) = split_qname(k);
                !p.is_empty() && p != "xmlns" && l == local && scope.iter().rev().find(|(x, _)| x == p).map(|(_, u)| u.as_str()) == Some(ns)
            })
            .map(|(_, v)| v.as_str())
    }

    /// `el.set(name, value)` for an attribute without a namespace or with the `xml` prefix: replaced in place,
    /// else added at the end.
    pub fn set(&mut self, id: usize, name: &str, value: &str) {
        let attrs = &mut self.nodes[id].attrs;
        match attrs.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = value.to_string(),
            None => attrs.push((name.to_string(), value.to_string())),
        }
    }

    /// `el.attrib.pop(name, None)`.
    pub fn remove_attr(&mut self, id: usize, name: &str) {
        self.nodes[id].attrs.retain(|(k, _)| k != name);
    }

    pub fn element_children(&self, id: usize) -> Vec<usize> {
        self.nodes[id].children.iter().copied().filter(|&c| self.is_el(c)).collect()
    }

    /// `el.find("{ns}local")`: the first element child with that name.
    pub fn find(&self, id: usize, ns: &str, local: &str) -> Option<usize> {
        self.nodes[id].children.iter().copied().find(|&c| self.is(c, ns, local))
    }

    /// `el.findall("{ns}local")`.
    pub fn findall(&self, id: usize, ns: &str, local: &str) -> Vec<usize> {
        self.nodes[id].children.iter().copied().filter(|&c| self.is(c, ns, local)).collect()
    }

    /// `el.findtext("{ns}local")`: `None` without such a child, else its text ("" when it has none).
    pub fn findtext(&self, id: usize, ns: &str, local: &str) -> Option<String> {
        self.find(id, ns, local).map(|c| self.text(c).unwrap_or_default())
    }

    /// `el.iter()`: the node and every descendant element in document order.
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

    /// `el.iter("{ns}local")`.
    pub fn iter_tag(&self, id: usize, ns: &str, local: &str) -> Vec<usize> {
        self.iter(id).into_iter().filter(|&e| self.is(e, ns, local)).collect()
    }

    /// lxml's `el.text`: the text before the first non-text child, `None` when there is none.
    pub fn text(&self, id: usize) -> Option<String> {
        let mut out: Option<String> = None;
        for &c in &self.nodes[id].children {
            if self.nodes[c].kind != Kind::Text {
                break;
            }
            out.get_or_insert_with(String::new).push_str(&self.nodes[c].value);
        }
        out
    }

    /// `el.text = value`: the leading text replaced.
    pub fn set_text(&mut self, id: usize, value: &str) {
        let lead = self.nodes[id].children.iter().take_while(|&&c| self.nodes[c].kind == Kind::Text).count();
        let gone: Vec<usize> = self.nodes[id].children.drain(..lead).collect();
        for g in gone {
            self.nodes[g].parent = None;
        }
        let mut n = Node::new(Kind::Text);
        n.value = value.to_string();
        n.parent = Some(id);
        self.nodes.push(n);
        let t = self.nodes.len() - 1;
        self.nodes[id].children.insert(0, t);
    }

    /// Whether `ancestor` is a proper ancestor of `id` satisfying `pred`.
    pub fn has_ancestor(&self, id: usize, pred: impl Fn(usize) -> bool) -> bool {
        let mut p = self.nodes[id].parent;
        while let Some(x) = p {
            if pred(x) {
                return true;
            }
            p = self.nodes[x].parent;
        }
        false
    }

    /// The element and the text nodes right after it (its lxml tail).
    fn group(&self, id: usize) -> Vec<usize> {
        let mut out = vec![id];
        if let Some(parent) = self.nodes[id].parent {
            let kids = &self.nodes[parent].children;
            if let Some(at) = kids.iter().position(|&k| k == id) {
                out.extend(kids[at + 1..].iter().copied().take_while(|&k| self.nodes[k].kind == Kind::Text));
            }
        }
        out
    }

    /// `parent.remove(el)`: the element goes, and its tail with it.
    pub fn remove(&mut self, id: usize) {
        let group = self.group(id);
        if let Some(parent) = self.nodes[id].parent {
            self.nodes[parent].children.retain(|k| !group.contains(k));
        }
        for g in group {
            self.nodes[g].parent = None;
        }
    }

    /// Detaches every child after the leading text (`el[:] = []`), tails included.
    pub fn clear_elements(&mut self, id: usize) {
        let lead = self.nodes[id].children.iter().take_while(|&&c| self.nodes[c].kind == Kind::Text).count();
        let gone: Vec<usize> = self.nodes[id].children.drain(lead..).collect();
        for g in gone {
            self.nodes[g].parent = None;
        }
    }

    fn copy_node(&mut self, id: usize) -> usize {
        let mut n = self.nodes[id].clone();
        n.parent = None;
        let kids = std::mem::take(&mut n.children);
        self.nodes.push(n);
        let new = self.nodes.len() - 1;
        for k in kids {
            let c = self.copy_node(k);
            self.nodes[c].parent = Some(new);
            self.nodes[new].children.push(c);
        }
        new
    }

    /// `copy.deepcopy(el)`: a detached copy of the element with its tail.
    pub fn copy_with_tail(&mut self, id: usize) -> Vec<usize> {
        self.group(id).into_iter().map(|g| self.copy_node(g)).collect()
    }

    /// `parent.append(el)` for detached nodes.
    pub fn append(&mut self, parent: usize, group: &[usize]) {
        for &g in group {
            self.nodes[g].parent = Some(parent);
            self.nodes[parent].children.push(g);
        }
    }

    /// `el.addprevious(new)`: the new nodes go right before `el` (after the text in front of it).
    pub fn add_previous(&mut self, el: usize, group: &[usize]) {
        let parent = self.nodes[el].parent.expect("addprevious needs a parent");
        let at = self.nodes[parent].children.iter().position(|&k| k == el).unwrap();
        for (i, &g) in group.iter().enumerate() {
            self.nodes[g].parent = Some(parent);
            self.nodes[parent].children.insert(at + i, g);
        }
    }

    /// `el.addnext(new)`: lxml keeps the text after `el` with it, so the new nodes go after that text.
    pub fn add_next(&mut self, el: usize, group: &[usize]) {
        let parent = self.nodes[el].parent.expect("addnext needs a parent");
        let at = self.nodes[parent].children.iter().position(|&k| k == el).unwrap();
        let mut at = at + 1;
        while at < self.nodes[parent].children.len() && self.nodes[self.nodes[parent].children[at]].kind == Kind::Text {
            at += 1;
        }
        for (i, &g) in group.iter().enumerate() {
            self.nodes[g].parent = Some(parent);
            self.nodes[parent].children.insert(at + i, g);
        }
    }

    /// `parent.insert(0, new)`: before the first non-text child (the leading text stays first), else at the end.
    pub fn insert_first(&mut self, parent: usize, group: &[usize]) {
        let at = self.nodes[parent].children.iter().position(|&k| self.nodes[k].kind != Kind::Text);
        match at {
            Some(at) => {
                for (i, &g) in group.iter().enumerate() {
                    self.nodes[g].parent = Some(parent);
                    self.nodes[parent].children.insert(at + i, g);
                }
            }
            None => self.append(parent, group),
        }
    }

    /// A new, detached element `{ns}local` for a place under `parent`: it takes the prefix `ns` has there, else
    /// declares one of its own (`dc` for Dublin Core, `ns0` for anything else, as lxml does).
    pub fn new_element(&mut self, parent: usize, ns: &str, local: &str) -> usize {
        let pscope = self.nodes[parent].scope;
        let scope = &self.scopes[pscope];
        // innermost binding of `ns` whose prefix is not rebound further in
        let found = scope.iter().enumerate().rev().find(|(i, (p, u))| u == ns && !scope[i + 1..].iter().any(|(q, _)| q == p)).map(|(_, (p, _))| p.clone());
        let mut n = Node::new(Kind::Element);
        n.ns = Some(ns.to_string());
        match found {
            Some(p) => {
                n.name = if p.is_empty() { local.to_string() } else { format!("{p}:{local}") };
                n.scope = pscope;
            }
            None => {
                let base = if ns == "http://purl.org/dc/elements/1.1/" { "dc" } else { "ns" };
                let mut prefix = if base == "dc" { "dc".to_string() } else { "ns0".to_string() };
                let mut k = 0;
                while scope.iter().any(|(q, _)| *q == prefix) {
                    k += 1;
                    prefix = format!("{base}{k}");
                }
                let mut s = scope.clone();
                s.push((prefix.clone(), ns.to_string()));
                self.scopes.push(s);
                n.scope = self.scopes.len() - 1;
                n.attrs.push((format!("xmlns:{prefix}"), ns.to_string()));
                n.name = format!("{prefix}:{local}");
            }
        }
        self.nodes.push(n);
        self.nodes.len() - 1
    }

    /// The tree as lxml's `etree.tostring(root)` writes it (no declaration).
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        self.write(self.root, &mut out);
        out
    }

    /// One element and its tail as `etree.tostring(el)` writes them (namespace declarations of the ancestors left
    /// out: this is only compared between siblings).
    pub fn serialize_with_tail(&self, id: usize) -> String {
        let mut out = String::new();
        for g in self.group(id) {
            self.write(g, &mut out);
        }
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

    #[test]
    fn serializes_like_lxml() {
        let src = "<a  v = '1' xmlns=\"urn:x\" xmlns:q=\"u\" ><b c=\"x&gt;y&#10;z\tw\"><n></n><s>A &apos;b&apos; &gt; <![CDATA[<c>]]> &#xE9;\r\n</s><!-- c --><?pi  d?></b></a >";
        let d = parse(src).unwrap();
        assert_eq!(d.serialize(), "<a xmlns=\"urn:x\" xmlns:q=\"u\" v=\"1\"><b c=\"x&gt;y&#10;z w\"><n/><s>A 'b' &gt; &lt;c&gt; \u{e9}\n</s><!-- c --><?pi d?></b></a>");
    }

    #[test]
    fn tails_move_with_their_element() {
        let mut d = parse("<a xmlns=\"urn:x\">lead<b/> tail-b <c/> tail-c </a>").unwrap();
        let b = d.find(d.root, "urn:x", "b").unwrap();
        let copy = d.copy_with_tail(b);
        d.remove(b);
        assert_eq!(d.serialize(), "<a xmlns=\"urn:x\">lead<c/> tail-c </a>");
        let c = d.find(d.root, "urn:x", "c").unwrap();
        d.add_previous(c, &copy);
        assert_eq!(d.serialize(), "<a xmlns=\"urn:x\">lead<b/> tail-b <c/> tail-c </a>");
        let root = d.root;
        d.clear_elements(root);
        assert_eq!(d.serialize(), "<a xmlns=\"urn:x\">lead</a>");
    }

    #[test]
    fn new_elements_use_the_prefix_in_scope() {
        let mut d = parse("<p:a xmlns:p=\"urn:p\"><b xmlns=\"urn:d\"/></p:a>").unwrap();
        let root = d.root;
        let x = d.new_element(root, "urn:p", "x");
        let y = d.new_element(root, "http://purl.org/dc/elements/1.1/", "language");
        let z = d.new_element(root, "urn:other", "z");
        d.append(root, &[x, y, z]);
        assert_eq!(
            d.serialize(),
            "<p:a xmlns:p=\"urn:p\"><b xmlns=\"urn:d\"/><p:x/><dc:language xmlns:dc=\"http://purl.org/dc/elements/1.1/\"/><ns0:z xmlns:ns0=\"urn:other\"/></p:a>"
        );
    }

    #[test]
    fn refuses_broken_xml() {
        let deep = "<a>".repeat(MAX_DEPTH + 1);
        for bad in ["<a>", "<a></b>", "<a x='1' x='2'/>", "<p:a/>", "<a p:x='1'/>", "<a>&nope;</a>", "<!DOCTYPE a><a/>", "<a/><b/>", deep.as_str()] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
