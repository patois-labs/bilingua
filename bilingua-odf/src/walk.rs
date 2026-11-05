//! Which paragraphs are text, how a paragraph becomes text with inline codes, and how translated text is put back.
//!
//! One walker serves every OpenDocument family: Writer, Calc, Impress and Draw all keep their text in `text:p`
//! and `text:h`. Rules follow ODF 1.3 Part 3 (6.1.2 white space, 6.1.3 `text:s`) and the Localization OS filter.

use crate::edit::{Edits, Piece};
use crate::xml::*;
use bilingua_model::{Error, Inline, LimitError, LimitKind, TagKind};

const SKIP: u8 = 1;
const HF: u8 = 2;
/// A `text:s` element asking for more spaces than this is cut short (a hostile `text:c="999999999"`).
const MAX_SPACES: usize = 1000;

fn is_skip_container(t: &Tree, n: u32) -> bool {
    let node = t.node(n);
    if node.ns == NS_TEXT {
        let l = t.local(n);
        return matches!(
            l,
            "tracked-changes"
                | "index-title-template"
                | "table-of-content-entry-template"
                | "illustration-index-entry-template"
                | "table-index-entry-template"
                | "object-index-entry-template"
                | "user-index-entry-template"
                | "alphabetical-index-entry-template"
                | "bibliography-entry-template"
        ) || l.ends_with("-source");
    }
    node.ns == NS_OFFICE && t.local(n) == "annotation"
}

fn cell_is_text(t: &Tree, cell: u32) -> bool {
    if t.attr(cell, NS_TABLE, "formula").is_some() {
        return false;
    }
    matches!(t.attr(cell, NS_OFFICE, "value-type").as_deref(), None | Some("string"))
}

/// Every translatable paragraph element of a part, in document order. A paragraph nested in another one
/// (footnote, text box) is listed after its host. In `styles.xml` only header and footer paragraphs count.
pub fn paragraphs(t: &Tree, styles_part: bool) -> Vec<u32> {
    let mut state = vec![0u8; t.nodes.len()];
    let mut out = Vec::new();
    for i in 0..t.nodes.len() as u32 {
        let node = t.node(i);
        if node.kind != Kind::Elem {
            continue;
        }
        let parent_state = if node.parent == NIL { if styles_part { 0 } else { HF } } else { state[node.parent as usize] };
        if node.ns == NS_TEXT && matches!(t.local(i), "p" | "h") && parent_state & SKIP == 0 && parent_state & HF != 0 {
            out.push(i);
        }
        let mut st = parent_state;
        if is_skip_container(t, i) {
            st |= SKIP;
        } else if node.ns == NS_TABLE && t.local(i) == "table-cell" && st & SKIP == 0 && !cell_is_text(t, i) {
            st |= SKIP;
        } else if node.ns == NS_STYLE && matches!(t.local(i), "header" | "footer" | "header-left" | "footer-left" | "header-first" | "footer-first") {
            st |= HF;
        }
        state[i as usize] = st;
    }
    out
}

/// Comment (`office:annotation`) paragraphs in document order, a comment inside a tracked deletion left out.
pub fn annotation_paragraphs(t: &Tree) -> Vec<u32> {
    let mut out = Vec::new();
    for i in 0..t.nodes.len() as u32 {
        if !t.is(i, NS_OFFICE, "annotation") || t.ancestors(i).any(|a| t.is(a, NS_TEXT, "tracked-changes")) {
            continue;
        }
        // all paragraphs below it, in document order (pre-order ids of one subtree are contiguous)
        let end = t.node(i).e1;
        let mut j = i + 1;
        while (j as usize) < t.nodes.len() && t.node(j).s0 < end {
            if t.node(j).kind == Kind::Elem && t.node(j).ns == NS_TEXT && matches!(t.local(j), "p" | "h") {
                out.push(j);
            }
            j += 1;
        }
    }
    out
}

pub fn is_paired(t: &Tree, n: u32) -> bool {
    t.node(n).kind == Kind::Elem && t.node(n).ns == NS_TEXT && matches!(t.local(n), "span" | "a" | "meta")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Text(String),
    /// Token numbers are 1-based and count every inline code in order of appearance.
    Open(usize),
    Close(usize),
    Point(usize),
}

pub struct Tok {
    pub parts: Vec<Part>,
    /// `nodes[n - 1]` is the original node of token `n`.
    pub nodes: Vec<u32>,
}

impl Tok {
    pub fn has_letter(&self) -> bool {
        self.parts.iter().any(|p| matches!(p, Part::Text(s) if s.chars().any(char::is_alphabetic)))
    }
}

struct Tz<'a> {
    t: &'a Tree,
    parts: Vec<Part>,
    nodes: Vec<u32>,
    buf: String,
    started: bool,
    space: bool,
    len: usize,
    max: usize,
}

fn too_long(max: usize) -> Error {
    Error::Limit(LimitError::new(
        LimitKind::UnitChars,
        format!("This file has a paragraph longer than {max} characters. Break it into shorter paragraphs."),
    ))
}

fn space_count(v: Option<String>) -> usize {
    let n = v.and_then(|s| s.trim().trim_start_matches('+').parse::<i64>().ok()).unwrap_or(1);
    n.clamp(1, MAX_SPACES as i64) as usize
}

impl Tz<'_> {
    fn grow(&mut self, k: usize) -> Result<(), Error> {
        self.len += k;
        if self.len > self.max {
            return Err(too_long(self.max));
        }
        Ok(())
    }

    fn chars(&mut self, s: &str) -> Result<(), Error> {
        if s.is_empty() {
            return Ok(());
        }
        // runs of space, tab, CR and LF are one space
        let mut tmp = String::with_capacity(s.len());
        let mut in_ws = false;
        for c in s.chars() {
            if matches!(c, ' ' | '\t' | '\r' | '\n') {
                if !in_ws {
                    tmp.push(' ');
                }
                in_ws = true;
            } else {
                in_ws = false;
                tmp.push(c);
            }
        }
        let mut s: &str = &tmp;
        if s.starts_with(' ') && (self.space || !self.started) {
            s = &s[1..];
        }
        if s.is_empty() {
            return Ok(());
        }
        self.grow(s.chars().count())?;
        self.buf.push_str(s);
        self.space = s.ends_with(' ');
        self.started = true;
        Ok(())
    }

    fn flush(&mut self) {
        if !self.buf.is_empty() {
            self.parts.push(Part::Text(std::mem::take(&mut self.buf)));
        }
    }

    fn token(&mut self, p: Part) {
        self.flush();
        self.parts.push(p);
        self.space = false;
    }

    fn walk(&mut self, el: u32) -> Result<(), Error> {
        let t = self.t;
        for c in t.children(el) {
            match t.node(c).kind {
                Kind::Text | Kind::Cdata => {
                    self.chars(&t.text(c))?;
                    continue;
                }
                Kind::Comment | Kind::Pi => {
                    self.nodes.push(c);
                    let n = self.nodes.len();
                    self.token(Part::Point(n));
                }
                Kind::Elem => {
                    if t.is(c, NS_TEXT, "s") {
                        let k = space_count(t.attr(c, NS_TEXT, "c"));
                        self.grow(k)?;
                        self.buf.extend(std::iter::repeat_n(' ', k));
                        self.space = false;
                        self.started = true;
                    } else if is_paired(t, c) {
                        self.nodes.push(c);
                        let n = self.nodes.len();
                        self.token(Part::Open(n));
                        self.walk(c)?;
                        self.token(Part::Close(n));
                    } else {
                        self.nodes.push(c);
                        let n = self.nodes.len();
                        self.token(Part::Point(n));
                    }
                }
            }
        }
        Ok(())
    }
}

/// The text of one paragraph with its inline codes, white space decoded per ODF 6.1.2.
pub fn tokenize(t: &Tree, p: u32, max_chars: usize) -> Result<Tok, Error> {
    let mut z = Tz { t, parts: Vec::new(), nodes: Vec::new(), buf: String::new(), started: false, space: false, len: 0, max: max_chars };
    z.walk(p)?;
    z.flush();
    Ok(Tok { parts: z.parts, nodes: z.nodes })
}

// ------------------------------------------------------------------------------------------ rebuild

struct Frame {
    node: Option<u32>,
    key: usize,
    pieces: Vec<Piece>,
}

struct Writer<'a> {
    base: Vec<Piece>,
    stack: Vec<Frame>,
    started: bool,
    /// `text:` prefix with colon, and an `xmlns` declaration to add when the document has no prefix for it.
    pfx: &'a str,
    decl: &'a str,
}

fn push_raw(v: &mut Vec<Piece>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Piece::Raw(last)) = v.last_mut() {
        last.push_str(s);
    } else {
        v.push(Piece::Raw(s.to_string()));
    }
}

impl Writer<'_> {
    fn container(&mut self) -> &mut Vec<Piece> {
        match self.stack.iter_mut().rev().find(|f| f.node.is_some()) {
            Some(f) => &mut f.pieces,
            None => &mut self.base,
        }
    }

    fn plain(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let mut esc = String::with_capacity(s.len());
        escape_text(s, &mut esc);
        push_raw(self.container(), &esc);
        self.started = true;
    }

    fn element(&mut self, local: &str, count: Option<usize>) {
        let s = match count {
            Some(c) => format!("<{p}{local}{d} {p}c=\"{c}\"/>", p = self.pfx, d = self.decl),
            None => format!("<{p}{local}{d}/>", p = self.pfx, d = self.decl),
        };
        push_raw(self.container(), &s);
    }

    /// Text between inline codes: extra spaces, tabs and line feeds become `text:s`, `text:tab`, `text:line-break`.
    fn text(&mut self, s: &str) {
        let b = s.as_bytes();
        let mut pos = 0;
        let mut i = 0;
        while i < b.len() {
            let (len, kind) = match b[i] {
                b' ' => {
                    let mut k = 1;
                    while i + k < b.len() && b[i + k] == b' ' {
                        k += 1;
                    }
                    if k >= 2 || i == 0 {
                        (k, 0)
                    } else {
                        i += 1;
                        continue;
                    }
                }
                b'\t' => (1, 1),
                b'\n' => (1, 2),
                _ => {
                    i += 1;
                    continue;
                }
            };
            if i == 0 && kind == 0 && len == 1 && !self.started {
                // a single leading space of the paragraph text: only text:s keeps it
                self.element("s", None);
                pos = i + len;
                i = pos;
                continue;
            }
            self.plain(&s[pos..i]);
            match kind {
                1 => self.element("tab", None),
                2 => self.element("line-break", None),
                _ => {
                    let lead = if i == 0 && !self.started { 0 } else { 1 };
                    if lead == 1 {
                        self.plain(" ");
                    }
                    if len > lead {
                        self.element("s", if len - lead > 1 { Some(len - lead) } else { None });
                    }
                }
            }
            self.started = true;
            pos = i + len;
            i = pos;
        }
        self.plain(&s[pos..]);
    }
}

/// Text the XML 1.0 can hold: pasted Word breaks become line feeds, illegal characters are dropped.
fn xml_target_text(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\u{b}' | '\u{c}' => Some('\n'),
            '\t' | '\n' | '\r' => Some(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => None,
            c => Some(c),
        })
        .collect()
}

/// Writes the translated paragraph `target` over `p`. `tok` is the tokenisation of the original paragraph, which
/// says which element each inline code stands for. Codes the translation lost or invented are tolerated: an
/// unknown or repeated code is dropped, and a point element (footnote, frame, bookmark) the translation left out is
/// kept at the end of the paragraph.
pub fn rebuild(t: &Tree, ed: &mut Edits, p: u32, tok: &Tok, target: &[Inline]) {
    let prefix = t.prefix(p);
    let (pfx, decl) = if prefix.is_empty() {
        match t.find_prefix(p, NS_TEXT) {
            Some(q) => (format!("{q}:"), String::new()),
            None => ("text:".to_string(), format!(" xmlns:text=\"{}\"", t.ns_uri(NS_TEXT))),
        }
    } else {
        (format!("{prefix}:"), String::new())
    };
    let mut w = Writer { base: Vec::new(), stack: Vec::new(), started: false, pfx: &pfx, decl: &decl };
    let mut used = vec![false; tok.nodes.len() + 1];
    for item in target {
        match item {
            Inline::Text(s) => w.text(&xml_target_text(s)),
            Inline::Tag { id, kind, .. } => {
                let key = id.parse::<usize>().ok().filter(|k| *k >= 1 && *k <= tok.nodes.len());
                match kind {
                    TagKind::Close => {
                        if let (Some(top), Some(k)) = (w.stack.last(), key) {
                            if top.key == k {
                                let f = w.stack.pop().expect("checked");
                                if let Some(n) = f.node {
                                    ed.set_content(t, n, f.pieces);
                                }
                            }
                        }
                    }
                    _ => {
                        let is_point = *kind == TagKind::Standalone;
                        let node = key.map(|k| tok.nodes[k - 1]);
                        let paired = node.is_some_and(|n| is_paired(t, n));
                        let usable = node.is_some() && !used[key.unwrap_or(0)] && paired != is_point;
                        if !usable {
                            if !is_point {
                                w.stack.push(Frame { node: None, key: key.unwrap_or(0), pieces: Vec::new() });
                            }
                            continue;
                        }
                        let (k, n) = (key.expect("usable"), node.expect("usable"));
                        used[k] = true;
                        w.container().push(Piece::Node(n));
                        if !is_point {
                            w.stack.push(Frame { node: Some(n), key: k, pieces: Vec::new() });
                        }
                    }
                }
            }
        }
    }
    while let Some(f) = w.stack.pop() {
        if let Some(n) = f.node {
            ed.set_content(t, n, f.pieces);
        }
    }
    for (i, n) in tok.nodes.iter().enumerate() {
        if !used[i + 1] && !is_paired(t, *n) {
            w.base.push(Piece::Node(*n));
        }
    }
    // A paired element the translation dropped is gone with its text, as its text is in the translation.
    ed.set_content(t, p, w.base);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bilingua_model::Limits;

    fn tree(body: &str) -> Tree {
        let s = format!(
            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\">{body}</office:document-content>"
        );
        Tree::parse(s, &Limits::default()).unwrap()
    }

    fn show(tok: &Tok) -> String {
        tok.parts
            .iter()
            .map(|p| match p {
                Part::Text(s) => s.clone(),
                Part::Open(n) => format!("<g{n}>"),
                Part::Close(n) => format!("</g{n}>"),
                Part::Point(n) => format!("<x{n}>"),
            })
            .collect()
    }

    #[test]
    fn white_space_follows_odf() {
        let t = tree("<text:p>  lead\n  wrapped   text <text:s text:c=\"3\"/>x<text:tab/> y</text:p>");
        let ps = paragraphs(&t, false);
        let tok = tokenize(&t, ps[0], 100_000).unwrap();
        assert_eq!(show(&tok), "lead wrapped text    x<x1> y");
    }

    #[test]
    fn text_c_is_capped_and_total_length_is_checked() {
        let t = tree("<text:p><text:s text:c=\"999999999\"/>x</text:p>");
        let ps = paragraphs(&t, false);
        let tok = tokenize(&t, ps[0], 100_000).unwrap();
        assert_eq!(show(&tok).len(), 1001);
        assert!(tokenize(&t, ps[0], 500).is_err());
    }
}
