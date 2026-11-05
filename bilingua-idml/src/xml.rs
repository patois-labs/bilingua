//! A small, lossless XML tree in an arena, with the element model of lxml (text before the first child, a tail
//! after each node) because the InDesign story logic is written against it.
//!
//! * Every element keeps its start tag as written, so a tree that was not changed writes back exactly.
//! * Text is kept raw (entities and CDATA as in the file) and decoded on demand; text added by the story code is
//!   escaped when it is appended.
//! * No DOCTYPE, no entity declarations, no undefined entities, no invalid characters: those are errors.
//! * Parsing is iterative; the nesting depth and the element count are capped by the caller.

use std::collections::HashSet;

pub type Id = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    El,
    Pi,
    Comment,
}

#[derive(Debug, Clone)]
struct Node {
    kind: Kind,
    /// Element name with its prefix; the target of a processing instruction.
    name: String,
    /// Element: the start tag between `<` and `>` (or `/>`), as written. PI and comment: the whole raw text.
    body: String,
    /// The start tag was written `<a/>`.
    empty_src: bool,
    /// `body` still matches `attrs`.
    raw_ok: bool,
    attrs: Vec<(String, String)>,
    /// Raw character data before the first child.
    text: String,
    /// Raw character data after this node (lxml's tail).
    tail: String,
    parent: Option<Id>,
    children: Vec<Id>,
}

#[derive(Debug, Clone)]
pub struct Tree {
    nodes: Vec<Node>,
    root: Id,
    prolog: String,
    epilog: String,
}

/// Limits for one parse.
#[derive(Debug, Clone, Copy)]
pub struct ParseLimits {
    pub max_depth: usize,
    pub max_elements: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        ParseLimits { max_depth: 256, max_elements: 4_000_000 }
    }
}

pub fn local_name(name: &str) -> &str {
    name.rsplit_once(':').map(|(_, l)| l).unwrap_or(name)
}

fn is_name_char(c: char) -> bool {
    !(c.is_whitespace() || matches!(c, '<' | '>' | '/' | '=' | '"' | '\'' | '&'))
}

/// Characters XML 1.0 allows.
pub fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{FFFE}' && c != '\u{FFFF}')
}

/// Checks character data (or an attribute value): valid characters, only predefined entities and valid
/// character references.
fn check_chars(s: &str, what: &str) -> Result<(), String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c < 0x20 && !matches!(c, 9 | 10 | 13) {
            return Err(format!("{what} holds the control character U+{c:04X}"));
        }
        if c == b'&' {
            let rest = &s[i + 1..];
            let semi = rest.find(';').filter(|k| *k <= 12).ok_or_else(|| format!("{what} has a bare ampersand"))?;
            let ent = &rest[..semi];
            let ok = match ent {
                "amp" | "lt" | "gt" | "quot" | "apos" => true,
                _ => char_ref(ent).is_some_and(is_xml_char),
            };
            if !ok {
                return Err(format!("{what} uses the entity &{ent}; which is not defined"));
            }
            i += semi + 2;
            continue;
        }
        i += 1;
    }
    if s.contains(['\u{FFFE}', '\u{FFFF}']) {
        return Err(format!("{what} holds a character XML does not allow"));
    }
    Ok(())
}

fn char_ref(ent: &str) -> Option<char> {
    let n = if let Some(h) = ent.strip_prefix("#x") {
        u32::from_str_radix(h, 16).ok()?
    } else if let Some(d) = ent.strip_prefix('#') {
        if d.is_empty() || !d.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        d.parse::<u32>().ok()?
    } else {
        return None;
    };
    char::from_u32(n)
}

/// Decodes raw character data: CDATA sections, the five entities and character references; literal CR LF and CR
/// become LF as an XML parser reads them.
pub fn decode_text(raw: &str) -> String {
    if !raw.contains(['&', '\r']) && !raw.contains("<![CDATA[") {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("<![CDATA[") {
            let end = tail.find("]]>").unwrap_or(tail.len());
            push_lf(&mut out, &tail[..end]);
            rest = tail.get(end + 3..).unwrap_or("");
            continue;
        }
        let next = rest.find(['&', '<']).unwrap_or(rest.len());
        if next > 0 {
            push_lf(&mut out, &rest[..next]);
            rest = &rest[next..];
            continue;
        }
        if rest.starts_with('&') {
            if let Some(semi) = rest[1..].find(';').filter(|k| *k <= 12) {
                let ent = &rest[1..1 + semi];
                let ch = match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => char_ref(ent),
                };
                if let Some(c) = ch {
                    out.push(c);
                    rest = &rest[semi + 2..];
                    continue;
                }
            }
        }
        // a lone `<` or `&` that is not markup: keep it
        let c = rest.chars().next().unwrap_or('&');
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn push_lf(out: &mut String, s: &str) {
    if !s.contains('\r') {
        out.push_str(s);
        return;
    }
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' {
            if it.peek() == Some(&'\n') {
                it.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
}

/// Escapes text for element content (`&`, `<`, `>`). Characters XML 1.0 cannot hold are dropped.
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c if is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c if is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
    out
}

fn decode_attr(raw: &str) -> String {
    // literal whitespace in an attribute value is a space; character references keep their value
    if raw.contains(['\t', '\n', '\r']) {
        let norm: String = raw.chars().map(|c| if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c }).collect();
        return decode_text(&norm);
    }
    decode_text(raw)
}

impl Tree {
    /// Parses a document. Errors are short messages safe to show.
    pub fn parse(src: &str, lim: &ParseLimits) -> Result<Tree, String> {
        let b = src.as_bytes();
        let n = b.len();
        let mut nodes: Vec<Node> = Vec::with_capacity(n / 40 + 8);
        let mut stack: Vec<Id> = Vec::new();
        let mut root: Option<Id> = None;
        let mut i = 0usize;
        let mut prolog_end = 0usize; // byte offset where the root element starts
        let mut epilog_start: Option<usize> = None;
        let mut elements = 0usize;
        if src.contains(['\u{FFFE}', '\u{FFFF}']) {
            return Err("the part holds a character XML does not allow".into());
        }
        let blank = |s: &str| s.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{feff}'));
        while i < n {
            if b[i] != b'<' {
                let j = src[i..].find('<').map(|k| i + k).unwrap_or(n);
                let t = &src[i..j];
                match stack.last().copied() {
                    Some(top) => {
                        check_chars(t, "character data")?;
                        push_text(&mut nodes, top, t);
                    }
                    None => {
                        if !blank(t) {
                            return Err("text outside the root element".into());
                        }
                    }
                }
                i = j;
                continue;
            }
            let rest = &src[i..];
            if rest.starts_with("<!--") {
                let e = rest[4..].find("-->").ok_or("unterminated comment")? + 4 + 3;
                if let Some(top) = stack.last().copied() {
                    let id = push_node(&mut nodes, Kind::Comment, String::new(), rest[..e].to_string(), Some(top));
                    nodes[top].children.push(id);
                }
                i += e;
            } else if rest.starts_with("<![CDATA[") {
                let e = rest.find("]]>").ok_or("unterminated CDATA section")? + 3;
                let top = *stack.last().ok_or("CDATA outside the root element")?;
                check_chars(&rest[9..e - 3], "a CDATA section").or_else(|m| if m.contains("ampersand") || m.contains("entity") { Ok(()) } else { Err(m) })?;
                push_text(&mut nodes, top, &rest[..e]);
                i += e;
            } else if rest.starts_with("<?") {
                let e = rest[2..].find("?>").ok_or("unterminated processing instruction")? + 2 + 2;
                let inner = &rest[2..e - 2];
                let target: String = inner.chars().take_while(|c| is_name_char(*c) && *c != '?').collect();
                if target.is_empty() {
                    return Err("a processing instruction has no target".into());
                }
                if let Some(top) = stack.last().copied() {
                    check_chars(inner, "a processing instruction").or_else(|m| if m.contains("ampersand") || m.contains("entity") { Ok(()) } else { Err(m) })?;
                    let id = push_node(&mut nodes, Kind::Pi, target, rest[..e].to_string(), Some(top));
                    nodes[top].children.push(id);
                }
                i += e;
            } else if rest.starts_with("<!") {
                return Err("a DOCTYPE or entity declaration is not allowed".into());
            } else if rest.starts_with("</") {
                let e = rest.find('>').ok_or("unterminated end tag")?;
                let name = rest[2..e].trim();
                let top = stack.pop().ok_or_else(|| format!("unexpected end tag </{name}>"))?;
                if nodes[top].name != name {
                    return Err(format!("end tag </{name}> does not match <{}>", nodes[top].name));
                }
                if stack.is_empty() {
                    epilog_start = Some(i + e + 1);
                }
                i += e + 1;
            } else {
                if stack.is_empty() && root.is_some() {
                    return Err("more than one root element".into());
                }
                let e = tag_end(rest)?;
                let inner = &rest[1..e];
                let (body, empty) = match inner.strip_suffix('/') {
                    Some(b) => (b, true),
                    None => (inner, false),
                };
                let name: String = body.chars().take_while(|c| is_name_char(*c)).collect();
                if name.is_empty() {
                    return Err("an element has no name".into());
                }
                let attrs = parse_attrs(&body[name.len()..])?;
                elements += 1;
                if elements > lim.max_elements {
                    return Err("too many elements".into());
                }
                let parent = stack.last().copied();
                let id = push_node(&mut nodes, Kind::El, name, body.to_string(), parent);
                nodes[id].empty_src = empty;
                nodes[id].raw_ok = true;
                nodes[id].attrs = attrs;
                match parent {
                    Some(p) => nodes[p].children.push(id),
                    None => {
                        root = Some(id);
                        prolog_end = i;
                    }
                }
                if !empty {
                    if stack.len() >= lim.max_depth {
                        return Err(format!("elements nested deeper than {}", lim.max_depth));
                    }
                    stack.push(id);
                } else if parent.is_none() {
                    epilog_start = Some(i + e + 1);
                }
                i += e + 1;
            }
        }
        if let Some(top) = stack.last() {
            return Err(format!("element <{}> is not closed", nodes[*top].name));
        }
        let root = root.ok_or("no root element")?;
        let epilog = src[epilog_start.unwrap_or(n)..].to_string();
        // the prolog must not hold anything but a declaration, comments, instructions and whitespace
        let pro = &src[..prolog_end];
        check_misc(pro)?;
        check_misc(&epilog)?;
        Ok(Tree { nodes, root, prolog: pro.to_string(), epilog })
    }

    pub fn root(&self) -> Id {
        self.root
    }

    pub fn kind(&self, id: Id) -> Kind {
        self.nodes[id].kind
    }
    pub fn is_element(&self, id: Id) -> bool {
        self.nodes[id].kind == Kind::El
    }
    pub fn is_pi(&self, id: Id) -> bool {
        self.nodes[id].kind == Kind::Pi
    }
    pub fn name(&self, id: Id) -> &str {
        &self.nodes[id].name
    }
    /// The name without its prefix; empty for a comment or processing instruction.
    pub fn local(&self, id: Id) -> &str {
        if self.nodes[id].kind == Kind::El {
            local_name(&self.nodes[id].name)
        } else {
            ""
        }
    }
    pub fn attr(&self, id: Id, name: &str) -> Option<&str> {
        self.nodes[id].attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    pub fn attrs(&self, id: Id) -> &[(String, String)] {
        &self.nodes[id].attrs
    }
    pub fn set_attr(&mut self, id: Id, name: &str, value: &str) {
        let n = &mut self.nodes[id];
        match n.attrs.iter_mut().find(|(k, _)| k == name) {
            Some(a) => a.1 = value.to_string(),
            None => n.attrs.push((name.to_string(), value.to_string())),
        }
        n.raw_ok = false;
    }
    pub fn parent(&self, id: Id) -> Option<Id> {
        self.nodes[id].parent
    }
    pub fn children(&self, id: Id) -> &[Id] {
        &self.nodes[id].children
    }
    pub fn index_of(&self, parent: Id, child: Id) -> Option<usize> {
        self.nodes[parent].children.iter().position(|c| *c == child)
    }

    /// Decoded character data before the first child.
    pub fn text(&self, id: Id) -> String {
        decode_text(&self.nodes[id].text)
    }
    /// Decoded character data after the node.
    pub fn tail(&self, id: Id) -> String {
        decode_text(&self.nodes[id].tail)
    }
    pub fn set_tail_raw(&mut self, id: Id, raw: &str) {
        self.nodes[id].tail = raw.to_string();
    }
    pub fn tail_raw(&self, id: Id) -> &str {
        &self.nodes[id].tail
    }
    pub fn clear_tail(&mut self, id: Id) {
        self.nodes[id].tail.clear();
    }
    /// Appends text (escaped) to the element's own leading text.
    pub fn push_text(&mut self, id: Id, s: &str) {
        let e = escape_text(s);
        self.nodes[id].text.push_str(&e);
    }
    /// Appends text (escaped) to the tail of a node.
    pub fn push_tail(&mut self, id: Id, s: &str) {
        let e = escape_text(s);
        self.nodes[id].tail.push_str(&e);
    }

    /// A new, detached element.
    pub fn new_element(&mut self, name: &str, attrs: &[(String, String)]) -> Id {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind: Kind::El,
            name: name.to_string(),
            body: String::new(),
            empty_src: false,
            raw_ok: false,
            attrs: attrs.to_vec(),
            text: String::new(),
            tail: String::new(),
            parent: None,
            children: vec![],
        });
        id
    }

    /// A detached copy of a node and everything under it (its tail is not copied).
    pub fn deep_copy(&mut self, id: Id) -> Id {
        let mut n = self.nodes[id].clone();
        let kids = std::mem::take(&mut n.children);
        n.parent = None;
        n.tail.clear();
        let new = self.nodes.len();
        self.nodes.push(n);
        for k in kids {
            let c = self.deep_copy_keep_tail(k);
            self.nodes[c].parent = Some(new);
            self.nodes[new].children.push(c);
        }
        new
    }

    fn deep_copy_keep_tail(&mut self, id: Id) -> Id {
        let tail = self.nodes[id].tail.clone();
        let c = self.deep_copy(id);
        self.nodes[c].tail = tail;
        c
    }

    /// Removes a node (with its tail) from its parent.
    pub fn detach(&mut self, id: Id) {
        if let Some(p) = self.nodes[id].parent.take() {
            self.nodes[p].children.retain(|c| *c != id);
        }
    }
    /// Moves `child` (with its tail) to the end of `parent`.
    pub fn append(&mut self, parent: Id, child: Id) {
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
    }
    /// Moves `child` to position `index` of `parent`.
    pub fn insert(&mut self, parent: Id, index: usize, child: Id) {
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        let at = index.min(self.nodes[parent].children.len());
        self.nodes[parent].children.insert(at, child);
    }
    /// Moves `new` to directly after `anchor`.
    pub fn add_next(&mut self, anchor: Id, new: Id) {
        let Some(p) = self.nodes[anchor].parent else { return };
        self.detach(new);
        let at = self.index_of(p, anchor).map(|i| i + 1).unwrap_or(self.nodes[p].children.len());
        self.nodes[new].parent = Some(p);
        self.nodes[p].children.insert(at, new);
    }

    /// The node and all its descendants in document order.
    pub fn iter(&self, id: Id) -> Vec<Id> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(x) = stack.pop() {
            out.push(x);
            for c in self.nodes[x].children.iter().rev() {
                stack.push(*c);
            }
        }
        out
    }

    /// All text under a node, decoded (like lxml's `itertext`): own text, then each child's text and tail.
    pub fn itertext(&self, id: Id) -> String {
        let mut out = String::new();
        self.itertext_into(id, &mut out, true);
        out
    }
    fn itertext_into(&self, id: Id, out: &mut String, top: bool) {
        let n = &self.nodes[id];
        if n.kind == Kind::El {
            out.push_str(&decode_text(&n.text));
            for c in &n.children {
                self.itertext_into(*c, out, false);
            }
        }
        if !top {
            out.push_str(&decode_text(&n.tail));
        }
    }

    /// The node and its subtree as XML, without the tail.
    pub fn serialize_node(&self, id: Id) -> String {
        let mut s = String::new();
        self.write_node(id, &mut s);
        s
    }

    /// The start tag of an element as `<name ...>`, or `<name .../>` when `closed`.
    pub fn start_tag(&self, id: Id, closed: bool) -> String {
        let n = &self.nodes[id];
        if n.kind != Kind::El {
            return n.body.clone();
        }
        let body = self.open_body(id);
        if closed {
            format!("<{}/>", body.trim_end())
        } else {
            format!("<{body}>")
        }
    }

    fn open_body(&self, id: Id) -> String {
        let n = &self.nodes[id];
        if n.raw_ok {
            return n.body.clone();
        }
        let mut s = n.name.clone();
        for (k, v) in &n.attrs {
            s.push(' ');
            s.push_str(k);
            s.push_str("=\"");
            s.push_str(&escape_attr(v));
            s.push('"');
        }
        s
    }

    fn write_node(&self, id: Id, out: &mut String) {
        let n = &self.nodes[id];
        if n.kind != Kind::El {
            out.push_str(&n.body);
            return;
        }
        let body = self.open_body(id);
        if n.children.is_empty() && n.text.is_empty() && (n.empty_src || !n.raw_ok) {
            if n.empty_src && n.raw_ok {
                out.push('<');
                out.push_str(&body);
                out.push_str("/>");
            } else {
                // built by the story code with no content: still an empty element
                out.push('<');
                out.push_str(&body);
                out.push_str("/>");
            }
            return;
        }
        out.push('<');
        out.push_str(&body);
        out.push('>');
        out.push_str(&n.text);
        for c in &n.children {
            self.write_node(*c, out);
            out.push_str(&self.nodes[*c].tail);
        }
        out.push_str("</");
        out.push_str(&n.name);
        out.push('>');
    }

    /// The whole document.
    pub fn to_xml(&self) -> String {
        let mut s = String::with_capacity(self.prolog.len() + self.epilog.len() + 1024);
        s.push_str(&self.prolog);
        self.write_node(self.root, &mut s);
        s.push_str(&self.epilog);
        s
    }

    /// The text before the root element (declaration, comments, instructions).
    pub fn prolog(&self) -> &str {
        &self.prolog
    }
}

fn push_node(nodes: &mut Vec<Node>, kind: Kind, name: String, body: String, parent: Option<Id>) -> Id {
    let id = nodes.len();
    nodes.push(Node {
        kind,
        name,
        body,
        empty_src: false,
        raw_ok: false,
        attrs: vec![],
        text: String::new(),
        tail: String::new(),
        parent,
        children: vec![],
    });
    id
}

fn push_text(nodes: &mut [Node], parent: Id, t: &str) {
    match nodes[parent].children.last().copied() {
        Some(last) => nodes[last].tail.push_str(t),
        None => nodes[parent].text.push_str(t),
    }
}

/// Outside the root element only a declaration, comments, instructions and white space may appear.
fn check_misc(s: &str) -> Result<(), String> {
    let mut rest = s;
    loop {
        rest = rest.trim_start_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{feff}'));
        if rest.is_empty() {
            return Ok(());
        }
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = &r[r.find("-->").ok_or("unterminated comment")? + 3..];
        } else if let Some(r) = rest.strip_prefix("<?") {
            rest = &r[r.find("?>").ok_or("unterminated processing instruction")? + 2..];
        } else {
            return Err("unexpected content outside the root element".into());
        }
    }
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
        } else if c == b'<' {
            return Err("a start tag holds a bare <".into());
        }
    }
    Err("unterminated start tag".into())
}

fn parse_attrs(s: &str) -> Result<Vec<(String, String)>, String> {
    let b = s.as_bytes();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut i = 0;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let ks = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let key = &s[ks..i];
        if key.is_empty() || !key.chars().all(is_name_char) {
            return Err("an attribute has a bad name".into());
        }
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            return Err(format!("attribute {key} has no value"));
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
            return Err(format!("attribute {key} is not quoted"));
        }
        let q = b[i];
        i += 1;
        let vs = i;
        while i < b.len() && b[i] != q {
            i += 1;
        }
        if i >= b.len() {
            return Err("unterminated attribute value".into());
        }
        let raw = &s[vs..i];
        if raw.contains('<') {
            return Err(format!("attribute {key} holds a bare <"));
        }
        check_chars(raw, "an attribute value")?;
        if !seen.insert(key) {
            return Err(format!("attribute {key} appears twice"));
        }
        out.push((key.to_string(), decode_attr(raw)));
        i += 1;
        // a value must be followed by white space or the end
        if i < b.len() && !b[i].is_ascii_whitespace() {
            return Err("attributes need white space between them".into());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Tree, String> {
        Tree::parse(s, &ParseLimits::default())
    }

    #[test]
    fn writes_back_exactly() {
        let s = "\u{feff}<?xml version=\"1.0\"?>\n<a x='1' y=\"2>\"><!-- c --><b /> t &amp; u<c>\n</c><![CDATA[x<y]]></a>\n";
        assert_eq!(p(s).unwrap().to_xml(), s);
    }

    #[test]
    fn refuses_bad_input() {
        for bad in [
            "<!DOCTYPE a [<!ENTITY x 'y'>]><a/>",
            "<a><b></a></b>",
            "<a>",
            "<a>&nope;</a>",
            "<a x='1' x='2'/>",
            "<a/><b/>",
            "text<a/>",
            "<a>\u{1}</a>",
            "<a b=c/>",
            "<a>&#0;</a>",
            "",
        ] {
            assert!(p(bad).is_err(), "{bad:?}");
        }
        let deep = "<a>".repeat(300) + &"</a>".repeat(300);
        assert!(p(&deep).is_err());
    }

    #[test]
    fn decodes() {
        assert_eq!(decode_text("a &amp; b &#x65E5; &#26412;"), "a & b \u{65e5} \u{672c}");
        assert_eq!(decode_text("<![CDATA[x < y]]>z"), "x < yz");
        assert_eq!(decode_text("a\r\nb\rc"), "a\nb\nc");
        assert_eq!(escape_text("a<b&c\u{1}"), "a&lt;b&amp;c");
    }

    #[test]
    fn edits() {
        let mut t = p("<r><a k='1'/>x<b/></r>").unwrap();
        let r = t.root();
        let a = t.children(r)[0];
        t.set_attr(a, "k", "2\"");
        let c = t.new_element("c", &[]);
        t.append(r, c);
        assert_eq!(t.to_xml(), "<r><a k=\"2&quot;\"/>x<b/><c/></r>");
        let d = t.deep_copy(r);
        assert_eq!(t.serialize_node(d), "<r><a k=\"2&quot;\"/>x<b/><c/></r>");
    }
}
