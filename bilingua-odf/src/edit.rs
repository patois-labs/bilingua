//! Edits over a [`Tree`]: new start tags, replaced or extended element content. Nothing is copied until the part is
//! written; an untouched subtree is emitted as one slice of the source.

use crate::xml::*;
use std::collections::HashMap;

pub enum Piece {
    /// Text or markup already in its final, escaped form.
    Raw(String),
    /// A node of the original tree, emitted with its own edits.
    Node(u32),
}

pub struct Edits {
    /// Name a namespace the document does not bind `nsN`, as lxml does, instead of a well-known prefix.
    pub lxml_prefixes: bool,
    /// The part is written again even if no byte changed (see [`Edits::mark_rewritten`]).
    forced: bool,
    content: HashMap<u32, Vec<Piece>>,
    start: HashMap<u32, String>,
    dirty: Vec<bool>,
}

impl Edits {
    pub fn new(t: &Tree) -> Edits {
        Edits { lxml_prefixes: false, forced: false, content: HashMap::new(), start: HashMap::new(), dirty: vec![false; t.nodes.len()] }
    }

    pub fn is_edited(&self) -> bool {
        self.forced || !self.content.is_empty() || !self.start.is_empty()
    }

    /// Localization OS writes a part again whenever it set a value in it, also to the value it had. Under the lxml
    /// naming (the locos profile) that is kept: the part counts as edited. Native mode leaves the bytes alone.
    pub fn mark_rewritten(&mut self) {
        if self.lxml_prefixes {
            self.forced = true;
        }
    }

    fn mark(&mut self, t: &Tree, mut n: u32) {
        while n != NIL && !self.dirty[n as usize] {
            self.dirty[n as usize] = true;
            n = t.node(n).parent;
        }
    }

    pub fn set_content(&mut self, t: &Tree, n: u32, pieces: Vec<Piece>) {
        self.mark(t, n);
        self.content.insert(n, pieces);
    }

    pub fn start_of<'a>(&'a self, t: &'a Tree, n: u32) -> &'a str {
        self.start.get(&n).map(String::as_str).unwrap_or_else(|| t.start_tag(n))
    }

    pub fn set_start(&mut self, t: &Tree, n: u32, s: String) {
        self.mark(t, n);
        self.start.insert(n, s);
    }

    fn pieces_mut(&mut self, t: &Tree, n: u32) -> &mut Vec<Piece> {
        self.mark(t, n);
        self.content.entry(n).or_insert_with(|| t.children(n).map(Piece::Node).collect())
    }

    /// Adds markup as the last child of `n`.
    pub fn append_child(&mut self, t: &Tree, n: u32, raw: String) {
        self.pieces_mut(t, n).push(Piece::Raw(raw));
    }

    /// Adds markup as child number `k` (counting every node that is not text), after the text that follows the
    /// previous child, the way lxml's `insert` places an element.
    pub fn insert_child(&mut self, t: &Tree, n: u32, k: usize, raw: String) {
        let kinds: Vec<bool> = t.children(n).map(|c| t.node(c).kind != Kind::Text && t.node(c).kind != Kind::Cdata).collect();
        let _ = self.pieces_mut(t, n);
        let is_text = |i: usize| kinds.get(i).is_some_and(|e| !e);
        let pos = if k == 0 {
            usize::from(is_text(0))
        } else {
            let mut seen = 0;
            let mut at = kinds.len();
            for (i, e) in kinds.iter().enumerate() {
                if *e {
                    seen += 1;
                    if seen == k {
                        at = i + 1;
                        break;
                    }
                }
            }
            if at < kinds.len() && is_text(at) {
                at + 1
            } else {
                at
            }
        };
        let pieces = self.content.get_mut(&n).expect("materialised");
        let pos = pos.min(pieces.len());
        pieces.insert(pos, Piece::Raw(raw));
    }

    /// Sets (`Some`) or removes (`None`) an attribute, by qualified name, in the current start tag of `n`.
    pub fn set_attr(&mut self, t: &Tree, n: u32, qname: &str, value: Option<&str>) {
        let cur = self.start_of(t, n).to_string();
        let new = edit_attr(&cur, qname, value);
        if new != cur {
            self.set_start(t, n, new);
        }
    }

    pub fn emit(&self, t: &Tree) -> String {
        let root = t.node(0);
        let mut out = String::with_capacity(t.src.len() + 256);
        out.push_str(&t.src[..root.s0 as usize]);
        self.emit_node(t, 0, &mut out);
        out.push_str(&t.src[root.e1 as usize..]);
        out
    }

    fn emit_node(&self, t: &Tree, n: u32, out: &mut String) {
        if !self.dirty[n as usize] {
            out.push_str(t.raw(n));
            return;
        }
        let start = self.start_of(t, n);
        let closed = t.self_closing(n);
        match self.content.get(&n) {
            Some(pieces) => {
                if pieces.is_empty() {
                    out.push_str(start);
                    if !closed {
                        out.push_str(t.end_tag(n));
                    }
                    return;
                }
                if closed {
                    out.push_str(&open_from_self_closing(start));
                } else {
                    out.push_str(start);
                }
                for p in pieces {
                    match p {
                        Piece::Raw(s) => out.push_str(s),
                        Piece::Node(c) => self.emit_node(t, *c, out),
                    }
                }
                if closed {
                    out.push_str("</");
                    out.push_str(t.qname(n));
                    out.push('>');
                } else {
                    out.push_str(t.end_tag(n));
                }
            }
            None => {
                out.push_str(start);
                for c in t.children(n) {
                    self.emit_node(t, c, out);
                }
                if !closed {
                    out.push_str(t.end_tag(n));
                }
            }
        }
    }
}

/// `<a b="c"/>` to `<a b="c">`.
fn open_from_self_closing(start: &str) -> String {
    let body = start.strip_suffix("/>").unwrap_or(start.strip_suffix('>').unwrap_or(start));
    format!("{}>", body.trim_end())
}

/// Sets or removes one attribute in a start tag text.
pub fn edit_attr(tag: &str, qname: &str, value: Option<&str>) -> String {
    let attrs = scan_attrs(tag);
    let hit = attrs.iter().find(|a| &tag[a.name_start..a.name_end] == qname);
    match (hit, value) {
        (Some(a), Some(v)) => {
            let mut s = String::with_capacity(tag.len() + v.len());
            s.push_str(&tag[..a.val_start]);
            s.push_str(&escape_attr(v, a.quote));
            s.push_str(&tag[a.val_end..]);
            s
        }
        (Some(a), None) => {
            let mut from = a.name_start;
            while from > 0 && tag.as_bytes()[from - 1].is_ascii_whitespace() {
                from -= 1;
            }
            format!("{}{}", &tag[..from], &tag[a.val_end + 1..])
        }
        (None, Some(v)) => {
            // before the closing `>` or `/>`, after the last attribute or the name
            let body = tag.strip_suffix("/>").map(|b| (b, "/>")).or_else(|| tag.strip_suffix('>').map(|b| (b, ">")));
            let (head, tailpart) = body.unwrap_or((tag, ""));
            let head_trim = head.trim_end();
            let ws = &head[head_trim.len()..];
            format!("{head_trim} {qname}=\"{}\"{ws}{tailpart}", escape_attr(v, b'"'))
        }
        (None, None) => tag.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_edits() {
        assert_eq!(edit_attr("<a x=\"1\" y='2'>", "x", Some("9")), "<a x=\"9\" y='2'>");
        assert_eq!(edit_attr("<a x=\"1\" y='2'>", "y", Some("a'b")), "<a x=\"1\" y='a&apos;b'>");
        assert_eq!(edit_attr("<a x=\"1\" y='2'/>", "x", None), "<a y='2'/>");
        assert_eq!(edit_attr("<a x=\"1\"/>", "z", Some("<")), "<a x=\"1\" z=\"&lt;\"/>");
        assert_eq!(edit_attr("<a>", "z", Some("1")), "<a z=\"1\">");
        assert_eq!(edit_attr("<a />", "z", Some("1")), "<a z=\"1\" />");
    }

    #[test]
    fn untouched_tree_is_one_slice_and_edits_are_local() {
        let s = "<?xml version=\"1.0\"?>\n<r><p k=\"1\">old</p><q/><s>keep</s></r>\n";
        let t = Tree::parse(s.to_string(), &bilingua_model::Limits::default()).unwrap();
        let mut e = Edits::new(&t);
        assert_eq!(e.emit(&t), s);
        let kids: Vec<u32> = t.children(0).collect();
        e.set_content(&t, kids[0], vec![Piece::Raw("new &amp;".into())]);
        e.set_content(&t, kids[1], vec![Piece::Raw("x".into())]);
        e.set_attr(&t, kids[0], "k", Some("2"));
        assert_eq!(e.emit(&t), "<?xml version=\"1.0\"?>\n<r><p k=\"2\">new &amp;</p><q>x</q><s>keep</s></r>\n");
    }

    #[test]
    fn child_insertion_follows_lxml_placement() {
        let s = "<r>\n <a/>\n <b/>\n</r>";
        let t = Tree::parse(s.to_string(), &bilingua_model::Limits::default()).unwrap();
        let mut e = Edits::new(&t);
        e.insert_child(&t, 0, 1, "<n/>".into());
        assert_eq!(e.emit(&t), "<r>\n <a/>\n <n/><b/>\n</r>");
        let mut e = Edits::new(&t);
        e.insert_child(&t, 0, 0, "<n/>".into());
        assert_eq!(e.emit(&t), "<r>\n <n/><a/>\n <b/>\n</r>");
        let mut e = Edits::new(&t);
        e.append_child(&t, 0, "<n/>".into());
        assert_eq!(e.emit(&t), "<r>\n <a/>\n <b/>\n<n/></r>");
    }
}
