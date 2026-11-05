//! The property list tree both encodings are read into. An XML plist also records the byte span of every element
//! (and the content span of every string), so the writer can splice a translation into the original text.

pub(crate) const NO_SPAN: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Dict,
    Array,
    Str,
    /// Numbers, booleans, dates, data, null, UID: kept as they are.
    Other,
}

/// A value of the `Other` kind in a binary plist (an XML plist never rewrites them, so they are not kept).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Scalar {
    None,
    Null,
    Bool(bool),
    Int(i128),
    Real(f64),
    Date(f64),
    Data(Vec<u8>),
    Uid(u64),
}

#[derive(Debug, Clone)]
pub(crate) struct Kid {
    /// The dictionary key (None in an array).
    pub key: Option<String>,
    /// XML: span of the `<key>` element.
    pub kstart: usize,
    pub kend: usize,
    pub node: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub kind: Kind,
    /// Decoded value of a string.
    pub text: String,
    pub kids: Vec<Kid>,
    /// XML: span of the whole element.
    pub start: usize,
    pub end: usize,
    /// XML: content span of a string (`NO_SPAN` for `<string/>`).
    pub cstart: usize,
    pub cend: usize,
    pub scalar: Scalar,
}

impl Node {
    pub(crate) fn new(kind: Kind) -> Node {
        Node { kind, text: String::new(), kids: Vec::new(), start: NO_SPAN, end: NO_SPAN, cstart: NO_SPAN, cend: NO_SPAN, scalar: Scalar::None }
    }
    pub(crate) fn string(text: String) -> Node {
        let mut n = Node::new(Kind::Str);
        n.text = text;
        n
    }
}

#[derive(Debug, Default)]
pub(crate) struct Tree {
    pub nodes: Vec<Node>,
    pub root: usize,
}

impl Tree {
    pub(crate) fn find_key(&self, dict: usize, key: &str) -> Option<usize> {
        self.nodes[dict].kids.iter().position(|k| k.key.as_deref() == Some(key))
    }
}
