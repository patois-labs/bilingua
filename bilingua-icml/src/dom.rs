//! A small XML tree with exact source spans. Text between tags is one node per run (lxml's text/tail), so the
//! story logic ported from the LocOS filter (which works on lxml trees) maps one to one. Untouched nodes are
//! written back from their source bytes; only nodes the rebuild created or changed are regenerated.

use std::borrow::Cow;
use std::collections::HashSet;

pub type Id = u32;
pub const NIL: Id = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Elem,
    Text,
    Comment,
    Pi,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub parent: Id,
    pub children: Vec<Id>,
    /// Element qualified name, or the target of a processing instruction.
    pub name: String,
    /// Namespace uri index + 1 (0 = none).
    pub ns: u32,
    pub attrs: Vec<(String, String)>,
    pub nsdecls: Vec<(String, String)>,
    /// Decoded text of a created or changed text node.
    pub value: Option<String>,
    /// Source span of the whole node.
    pub src: Option<(u32, u32)>,
    /// End of the start tag in the source (elements).
    pub start_end: u32,
    pub self_closed: bool,
    pub dirty: bool,
    pub attrs_dirty: bool,
}

impl Node {
    pub fn blank(kind: Kind) -> Node {
        Node {
            kind,
            parent: NIL,
            children: Vec::new(),
            name: String::new(),
            ns: 0,
            attrs: Vec::new(),
            nsdecls: Vec::new(),
            value: None,
            src: None,
            start_end: 0,
            self_closed: false,
            dirty: true,
            attrs_dirty: true,
        }
    }
}

pub fn xml_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// The namespace declarations a story keeps in the lxml form, see [`Dom::story_ns`].
pub struct StoryNs {
    top: Id,
    keep: HashSet<(Id, usize)>,
    added: Vec<(String, String)>,
    pub outside: bool,
}

pub struct Dom {
    pub text: String,
    pub nodes: Vec<Node>,
    pub uris: Vec<String>,
    pub root: Id,
}

impl Dom {
    pub fn n(&self, id: Id) -> &Node {
        &self.nodes[id as usize]
    }
    pub fn nm(&mut self, id: Id) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    pub fn is_elem(&self, id: Id) -> bool {
        self.n(id).kind == Kind::Elem
    }
    pub fn is_pi(&self, id: Id) -> bool {
        self.n(id).kind == Kind::Pi
    }
    /// Local name of an element ("" for anything else), the equivalent of the LocOS `_local`.
    pub fn local(&self, id: Id) -> &str {
        let n = self.n(id);
        if n.kind != Kind::Elem {
            return "";
        }
        n.name.rsplit(':').next().unwrap_or("")
    }
    /// An element with no namespace and exactly this name (lxml's `el.tag == name`).
    pub fn is_plain(&self, id: Id, name: &str) -> bool {
        let n = self.n(id);
        n.kind == Kind::Elem && n.ns == 0 && n.name == name
    }
    pub fn attr(&self, id: Id, name: &str) -> Option<&str> {
        self.n(id).attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    pub fn ns_uri(&self, id: Id) -> &str {
        match self.n(id).ns {
            0 => "",
            k => &self.uris[k as usize - 1],
        }
    }

    /// Decoded character data of a text node.
    pub fn text_value(&self, id: Id) -> Cow<'_, str> {
        let n = self.n(id);
        if let Some(v) = &n.value {
            return Cow::Borrowed(v);
        }
        match n.src {
            Some((s, e)) => decode_chardata(&self.text[s as usize..e as usize]),
            None => Cow::Borrowed(""),
        }
    }

    /// Children that are elements, comments or processing instructions (lxml's `list(el)`).
    pub fn kids(&self, id: Id) -> Vec<Id> {
        self.n(id).children.iter().copied().filter(|c| self.n(*c).kind != Kind::Text).collect()
    }

    pub fn index_of(&self, parent: Id, child: Id) -> Option<usize> {
        self.n(parent).children.iter().position(|c| *c == child)
    }

    /// Marks `id` and its ancestors as changed (their bytes are regenerated, not copied).
    pub fn touch(&mut self, mut id: Id) {
        while id != NIL {
            let n = self.nm(id);
            n.dirty = true;
            id = n.parent;
        }
    }

    pub fn new_elem(&mut self, name: &str, ns: u32, attrs: Vec<(String, String)>, nsdecls: Vec<(String, String)>) -> Id {
        let mut n = Node::blank(Kind::Elem);
        n.name = name.to_string();
        n.ns = ns;
        n.attrs = attrs;
        n.nsdecls = nsdecls;
        self.nodes.push(n);
        (self.nodes.len() - 1) as Id
    }

    pub fn new_text(&mut self, value: &str) -> Id {
        let mut n = Node::blank(Kind::Text);
        n.value = Some(value.to_string());
        self.nodes.push(n);
        (self.nodes.len() - 1) as Id
    }

    /// Removes `id` from its parent (and nothing else).
    pub fn detach(&mut self, id: Id) {
        let p = self.n(id).parent;
        if p == NIL {
            return;
        }
        if let Some(i) = self.index_of(p, id) {
            self.nm(p).children.remove(i);
        }
        self.nm(id).parent = NIL;
        self.touch(p);
    }

    pub fn append(&mut self, parent: Id, child: Id) {
        self.detach(child);
        self.nm(child).parent = parent;
        self.nm(parent).children.push(child);
        self.touch(parent);
    }

    pub fn insert(&mut self, parent: Id, at: usize, child: Id) {
        self.detach(child);
        self.nm(child).parent = parent;
        let at = at.min(self.n(parent).children.len());
        self.nm(parent).children.insert(at, child);
        self.touch(parent);
    }

    /// Inserts `child` right before `anchor`.
    pub fn insert_before(&mut self, anchor: Id, child: Id) {
        let p = self.n(anchor).parent;
        if p == NIL {
            return;
        }
        let at = self.index_of(p, anchor).unwrap_or(0);
        self.insert(p, at, child);
    }

    /// Removes `id` from its parent together with its tail text (the text node right after it); returns that
    /// tail's text.
    pub fn remove_with_tail(&mut self, id: Id) -> Option<String> {
        let p = self.n(id).parent;
        if p == NIL {
            return None;
        }
        let i = self.index_of(p, id)?;
        let tail = match self.n(p).children.get(i + 1) {
            Some(t) if self.n(*t).kind == Kind::Text => Some(*t),
            _ => None,
        };
        let tail_text = tail.map(|t| self.text_value(t).into_owned());
        if let Some(t) = tail {
            self.nm(p).children.remove(i + 1);
            self.nm(t).parent = NIL;
        }
        self.nm(p).children.remove(i);
        self.nm(id).parent = NIL;
        self.touch(p);
        tail_text
    }

    pub fn set_attr(&mut self, id: Id, name: &str, value: &str) {
        let n = self.nm(id);
        match n.attrs.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = value.to_string(),
            None => n.attrs.push((name.to_string(), value.to_string())),
        }
        n.attrs_dirty = true;
        self.touch(id);
    }

    /// A copy of the subtree under `id` (children included); clean nodes keep their source bytes.
    pub fn deep_copy(&mut self, id: Id) -> Id {
        let mut n = self.n(id).clone();
        n.parent = NIL;
        let kids = std::mem::take(&mut n.children);
        self.nodes.push(n);
        let new = (self.nodes.len() - 1) as Id;
        for k in kids {
            let c = self.deep_copy(k);
            self.nm(c).parent = new;
            self.nm(new).children.push(c);
        }
        new
    }

    /// Appends text to a parent: extends the last child when it is text, else adds a text node.
    pub fn append_text(&mut self, parent: Id, s: &str) {
        if s.is_empty() {
            return;
        }
        if let Some(&last) = self.n(parent).children.last() {
            if self.n(last).kind == Kind::Text {
                let mut v = self.text_value(last).into_owned();
                v.push_str(s);
                let n = self.nm(last);
                n.value = Some(v);
                n.dirty = true;
                self.touch(parent);
                return;
            }
        }
        let t = self.new_text(s);
        self.append(parent, t);
    }

    // ---------------------------------------------------------------- serialization
    /// Writes `id`: untouched nodes from the source bytes, the rest regenerated. `force` regenerates every
    /// element and text node (used to compare subtrees).
    pub fn ser(&self, id: Id, out: &mut String, force: bool) {
        let n = self.n(id);
        match n.kind {
            Kind::Text => match (&n.value, n.src) {
                (Some(v), _) => escape_text(v, out),
                (None, Some((s, e))) if !force => out.push_str(&self.text[s as usize..e as usize]),
                (None, Some(_)) => escape_text(&self.text_value(id), out),
                (None, None) => {}
            },
            Kind::Comment | Kind::Pi => match n.src {
                Some((s, e)) => out.push_str(&self.text[s as usize..e as usize]),
                None => {
                    if n.kind == Kind::Pi {
                        out.push_str("<?");
                        out.push_str(&n.name);
                        out.push_str("?>");
                    }
                }
            },
            Kind::Elem => self.ser_elem(id, out, force),
        }
    }

    /// Writes `id` the way lxml does: every element's start tag is regenerated (one space between attributes, double
    /// quotes) and an element with no children is written empty (`<a/>`, also for a source `<a></a>`). Text is
    /// escaped afresh; comments keep their source bytes and a processing instruction is `<?target data?>`.
    /// `ns` says which namespace declarations to write (see [`Dom::story_ns`]); `None` writes every one, as
    /// `tostring` of a whole document does.
    pub fn ser_lxml(&self, id: Id, out: &mut String, ns: Option<&StoryNs>) {
        let n = self.n(id);
        match n.kind {
            Kind::Pi => {
                self.pi_lxml(id, out);
                return;
            }
            Kind::Elem => {}
            _ => {
                self.ser(id, out, true);
                return;
            }
        }
        let decls = self.decls_for(id, ns);
        if n.children.is_empty() {
            self.gen_start(n, &decls, out, true);
            return;
        }
        self.gen_start(n, &decls, out, false);
        for c in &n.children {
            self.ser_lxml(*c, out, ns);
        }
        out.push_str("</");
        out.push_str(&n.name);
        out.push('>');
    }

    fn decls_for(&self, id: Id, ns: Option<&StoryNs>) -> Vec<(String, String)> {
        let n = self.n(id);
        match ns {
            None => n.nsdecls.clone(),
            Some(k) => {
                let mut v: Vec<(String, String)> = n.nsdecls.iter().enumerate().filter(|(i, _)| k.keep.contains(&(id, *i))).map(|(_, d)| d.clone()).collect();
                if id == k.top {
                    v.extend(k.added.iter().cloned());
                }
                v
            }
        }
    }

    /// A processing instruction as libxml2 writes it: `<?target data?>`, the white space after the target dropped.
    pub fn pi_lxml(&self, id: Id, out: &mut String) {
        let n = self.n(id);
        let Some((s, e)) = n.src else { return };
        let body = &self.text[s as usize + 2..e as usize - 2];
        let data = body[n.name.len().min(body.len())..].trim_start_matches(xml_ws);
        out.push_str("<?");
        out.push_str(&n.name);
        if !data.is_empty() {
            out.push(' ');
            out.push_str(data);
        }
        out.push_str("?>");
    }

    /// The namespace declarations lxml leaves on a copy of `top` made with `deepcopy` and cleaned with
    /// `cleanup_namespaces`: the declarations in the subtree that some element or attribute name uses, in place, and
    /// on the copy's root those of the ancestors that the subtree uses, after the root's own, in the order they are
    /// first needed (an element's name before its attributes, in document order). `outside` is true when the
    /// subtree uses any prefix declared above it, so that it does not parse on its own.
    pub fn story_ns(&self, top: Id) -> StoryNs {
        let mut r = StoryNs { top, keep: HashSet::new(), added: Vec::new(), outside: false };
        let mut stack = vec![top];
        while let Some(e) = stack.pop() {
            let n = self.n(e);
            if n.kind != Kind::Elem {
                continue;
            }
            let mut uses: Vec<&str> = Vec::new();
            uses.push(n.name.split_once(':').map_or("", |(p, _)| p));
            for (k, _) in &n.attrs {
                if let Some((p, _)) = k.split_once(':') {
                    if p != "xml" && p != "xmlns" {
                        uses.push(p);
                    }
                }
            }
            for p in uses {
                match self.resolve_prefix(e, top, p) {
                    Some((x, i, true)) => {
                        r.keep.insert((x, i));
                    }
                    Some((x, i, false)) => {
                        let (pf, uri) = &self.n(x).nsdecls[i];
                        if !uri.is_empty() {
                            r.outside = true;
                            let own = self.n(top).nsdecls.iter().any(|(q, _)| q == pf);
                            if !own && !r.added.iter().any(|(q, _)| q == pf) {
                                r.added.push((pf.clone(), uri.clone()));
                            }
                        }
                    }
                    None => {}
                }
            }
            for c in n.children.iter().rev() {
                stack.push(*c);
            }
        }
        r
    }

    /// The nearest declaration of `prefix` seen from `from`: its node, its place in that node's list, and whether the
    /// node is `top` or below it.
    fn resolve_prefix(&self, from: Id, top: Id, prefix: &str) -> Option<(Id, usize, bool)> {
        let mut x = from;
        let mut inside = true;
        loop {
            if let Some(i) = self.n(x).nsdecls.iter().rposition(|(p, _)| p == prefix) {
                return Some((x, i, inside));
            }
            if x == top {
                inside = false;
            }
            let up = self.n(x).parent;
            if up == NIL {
                return None;
            }
            x = up;
        }
    }

    fn ser_elem(&self, id: Id, out: &mut String, force: bool) {
        let n = self.n(id);
        if let (false, false, Some((s, e))) = (n.dirty, force, n.src) {
            out.push_str(&self.text[s as usize..e as usize]);
            return;
        }
        let has_kids = !n.children.is_empty();
        let sliceable = !force && !n.attrs_dirty && n.src.is_some();
        if !has_kids && (n.src.is_none() || n.self_closed) {
            if let (true, true, Some((s, e))) = (sliceable, n.self_closed, n.src) {
                out.push_str(&self.text[s as usize..e as usize]);
            } else {
                self.gen_start(n, &n.nsdecls, out, true);
            }
            return;
        }
        match n.src {
            Some((s, _)) if sliceable && !n.self_closed => out.push_str(&self.text[s as usize..n.start_end as usize]),
            _ => self.gen_start(n, &n.nsdecls, out, false),
        }
        for c in &n.children {
            self.ser(*c, out, force);
        }
        out.push_str("</");
        out.push_str(&n.name);
        out.push('>');
    }

    fn gen_start(&self, n: &Node, decls: &[(String, String)], out: &mut String, empty: bool) {
        out.push('<');
        out.push_str(&n.name);
        for (p, u) in decls {
            out.push(' ');
            if p.is_empty() {
                out.push_str("xmlns");
            } else {
                out.push_str("xmlns:");
                out.push_str(p);
            }
            out.push_str("=\"");
            escape_attr(u, out);
            out.push('"');
        }
        for (k, v) in &n.attrs {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            escape_attr(v, out);
            out.push('"');
        }
        out.push_str(if empty { "/>" } else { ">" });
    }
}

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

pub fn escape_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
}

fn nl(s: &str, out: &mut String) {
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

/// Character data of a text run: CDATA sections, predefined and numeric references, line ends normalised.
/// The parser has already checked the run, so anything unexpected is passed through as it is.
pub fn decode_chardata(raw: &str) -> Cow<'_, str> {
    if !raw.contains(['&', '\r']) && !raw.contains("<![CDATA[") {
        return Cow::Borrowed(raw);
    }
    let b = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0usize;
    while i < b.len() {
        if raw[i..].starts_with("<![CDATA[") {
            let end = raw[i + 9..].find("]]>").map_or(raw.len(), |e| i + 9 + e);
            nl(&raw[i + 9..end], &mut out);
            i = (end + 3).min(raw.len());
        } else if b[i] == b'&' {
            match decode_ref(&raw[i..]) {
                Some((c, len)) => {
                    out.push(c);
                    i += len;
                }
                None => {
                    out.push('&');
                    i += 1;
                }
            }
        } else {
            let mut j = i;
            while j < b.len() && b[j] != b'&' && !(b[j] == b'<' && raw[j..].starts_with("<![CDATA[")) {
                j += 1;
            }
            nl(&raw[i..j], &mut out);
            i = j;
        }
    }
    Cow::Owned(out)
}

/// `&name;` or `&#N;` / `&#xH;` at the start of `s`: the character and the length of the reference.
pub fn decode_ref(s: &str) -> Option<(char, usize)> {
    let b = s.as_bytes();
    if b.first() != Some(&b'&') {
        return None;
    }
    let lim = b.len().min(14);
    let semi = b[..lim].iter().position(|c| *c == b';')?;
    let body = &s[1..semi];
    let c = match body {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let num = body.strip_prefix('#')?;
            let code = if let Some(h) = num.strip_prefix('x') {
                if h.is_empty() || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return None;
                }
                u32::from_str_radix(h, 16).ok()?
            } else {
                if num.is_empty() || !num.bytes().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                num.parse::<u32>().ok()?
            };
            let ch = char::from_u32(code)?;
            if !legal_char(ch) {
                return None;
            }
            ch
        }
    };
    Some((c, semi + 1))
}

pub fn legal_char(c: char) -> bool {
    matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}
