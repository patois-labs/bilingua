//! One strict pass over the decoded text of a stylesheet: checks it is well-formed (balanced, names, namespaces
//! declared, entities defined, XML characters only, depth and element limits), vets the DOCTYPE (only simple
//! character entities are accepted) and builds a light element tree whose nodes carry exact byte spans, so the
//! writer can splice only the units.

use crate::Limits;
use bilingua_model::{Error, LimitError, LimitKind};
use std::collections::HashSet;

pub const XSL_NS: &str = "http://www.w3.org/1999/XSL/Transform";
pub const ITS_NS: &str = "http://www.w3.org/2005/11/its";
pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Attributes of literal result elements (and `xsl:attribute` names) that hold user text.
pub const TRANSLATABLE_ATTRIBUTES: [&str; 4] = ["alt", "aria-label", "placeholder", "title"];

/// Deepest element nesting read (LocOS `_MAX_DEPTH`).
pub const MAX_DEPTH: usize = 200;

pub fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "xslt".into(), msg: msg.into() }
}

fn wf(msg: impl AsRef<str>) -> Error {
    perr(format!("This XSLT file is not well-formed XML: {}", msg.as_ref()))
}

pub fn limit_err(kind: LimitKind, msg: impl Into<String>) -> Error {
    LimitError::new(kind, msg).into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrNs {
    None,
    Xsl,
    Xml,
    Its,
    Other,
}

#[derive(Debug, Clone)]
pub struct Attr<'a> {
    pub name: &'a str,
    pub local: &'a str,
    pub prefixed: bool,
    pub ns: AttrNs,
    /// Byte span of the value between the quotes.
    pub vs: usize,
    pub ve: usize,
    pub quote: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafKind {
    Text,
    Entity,
    Comment,
    Cdata,
    Pi,
}

#[derive(Debug, Clone, Copy)]
pub struct Leaf {
    pub kind: LeafKind,
    pub s: usize,
    pub e: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum Child {
    El(usize),
    Leaf(Leaf),
}

/// How a child sits inside a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ik {
    Text,
    Point,
    Paired,
    Break,
}

#[derive(Debug)]
pub struct El<'a> {
    pub s: usize,
    pub e: usize,
    /// End of the start tag, and start of the end tag (`e` for an empty-element tag).
    pub oe: usize,
    pub cs: usize,
    pub local: &'a str,
    pub is_xsl: bool,
    pub attrs: Vec<Attr<'a>>,
    pub children: Vec<Child>,
    /// Nothing under this element is document text.
    pub skip: bool,
    pub expand_text: Option<bool>,
    pub preserve: Option<bool>,
    /// `xsl:attribute` whose `name` is one of the translatable attributes.
    pub attr_name_ok: bool,
    pub ik: Ik,
}

pub struct Lexed<'a> {
    pub ents: HashSet<&'a str>,
    pub els: Vec<El<'a>>,
    pub root: usize,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn xml_char(c: char) -> bool {
    matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn ncname(s: &str) -> bool {
    let mut it = s.chars();
    let Some(f) = it.next() else { return false };
    if !(f.is_alphabetic() || f == '_' || (f as u32) >= 0xC0) {
        return false;
    }
    it.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.') || (c as u32) >= 0xB7)
}

fn valid_qname(s: &str) -> bool {
    match s.split_once(':') {
        None => ncname(s),
        Some((p, l)) => ncname(p) && ncname(l),
    }
}


/// Entity references of a span: `Some(name)` for a named reference that is not one of the five predefined ones.
/// Validates every `&`; a numeric reference must name an XML character, a named one a predefined or declared entity.
pub fn scan_refs(
    region: &str,
    base: usize,
    ents: &HashSet<&str>,
    mut named: impl FnMut(usize, usize),
) -> Result<(), Error> {
    let b = region.as_bytes();
    let mut i = 0;
    while let Some(p) = b[i..].iter().position(|c| *c == b'&') {
        let at = i + p;
        let rest = &region[at + 1..];
        let semi = rest.bytes().take(40).position(|c| c == b';');
        let Some(len) = semi else { return Err(wf("bad entity reference")) };
        let name = &rest[..len];
        if let Some(n) = name.strip_prefix('#') {
            let v = match n.strip_prefix('x') {
                Some(h) if !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit()) => u32::from_str_radix(h, 16).ok(),
                None if !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()) => n.parse().ok(),
                _ => None,
            };
            if !v.and_then(char::from_u32).is_some_and(xml_char) {
                return Err(wf(format!("character reference &{name}; is not an XML character")));
            }
        } else if !matches!(name, "lt" | "gt" | "amp" | "quot" | "apos") {
            if !ncname(name) || !ents.contains(name) {
                return Err(wf(format!("undefined entity &{name};")));
            }
            named(base + at, base + at + len + 2);
        }
        i = at + len + 2;
    }
    Ok(())
}

/// Decodes the predefined entities and character references (already validated).
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        let Some(semi) = after.find(';') else {
            out.push_str(&rest[p..]);
            return out;
        };
        let name = &after[..semi];
        match name {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let v = match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => name.strip_prefix('#').and_then(|d| d.parse().ok()),
                };
                match v.and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[p..p + semi + 2]),
                }
            }
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------- DOCTYPE

fn skip_ws(text: &str, mut i: usize) -> usize {
    let b = text.as_bytes();
    while i < b.len() && (is_ws(b[i]) || (b[i] >= 0x80 && text[i..].chars().next().is_some_and(char::is_whitespace))) {
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    i
}

/// Position after the whitespace, comments and processing instructions that may precede the root.
fn prolog_end(text: &str) -> usize {
    let mut i = 0;
    loop {
        let j = skip_ws(text, i);
        if j > i {
            i = j;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<?") {
            match rest.find("?>") {
                Some(e) => i += e + 2,
                None => return i,
            }
        } else if let Some(after) = rest.strip_prefix("<!--") {
            match after.find("-->") {
                Some(e) => i += 4 + e + 3,
                None => return i,
            }
        } else {
            return i;
        }
    }
}

fn charref_at(b: &[u8], i: usize) -> Option<usize> {
    // `&#[0-9]{1,7};` or `&#x[0-9A-Fa-f]{1,6};`, returns the position after the `;`
    if b.get(i) != Some(&b'&') || b.get(i + 1) != Some(&b'#') {
        return None;
    }
    let (start, hex, max) = if b.get(i + 2) == Some(&b'x') { (i + 3, true, 6) } else { (i + 2, false, 7) };
    let mut j = start;
    while j < b.len() && (if hex { b[j].is_ascii_hexdigit() } else { b[j].is_ascii_digit() }) {
        j += 1;
    }
    let n = j - start;
    (n >= 1 && n <= max && b.get(j) == Some(&b';')).then_some(j + 1)
}

/// `<!ENTITY name "value">` where the value is one character reference or up to 16 plain characters.
fn simple_entity(text: &str, pos: usize) -> Option<(usize, &str)> {
    let b = text.as_bytes();
    let rest = text.get(pos..)?;
    if !rest.starts_with("<!ENTITY") {
        return None;
    }
    let mut i = pos + 8;
    let j = skip_ws(text, i);
    if j == i {
        return None;
    }
    i = j;
    let ns = i;
    let mut it = text[i..].chars();
    let f = it.next()?;
    if !(f.is_ascii_alphabetic() || f == '_') {
        return None;
    }
    i += f.len_utf8();
    for c in it {
        if c.is_alphanumeric() || matches!(c, '_' | '.' | '-') {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    let name = &text[ns..i];
    let j = skip_ws(text, i);
    if j == i {
        return None;
    }
    i = j;
    let q = *b.get(i)?;
    if q != b'"' && q != b'\'' {
        return None;
    }
    i += 1;
    let after_value = if let Some(e) = charref_at(b, i).filter(|e| b.get(*e) == Some(&q)) {
        e
    } else {
        let mut k = i;
        for (n, c) in text[i..].chars().enumerate() {
            if c as u32 == q as u32 || matches!(c, '&' | '%' | '<') || n == 16 {
                break;
            }
            k += c.len_utf8();
        }
        if b.get(k) != Some(&q) {
            return None;
        }
        k
    };
    i = after_value + 1;
    i = skip_ws(text, i);
    (b.get(i) == Some(&b'>')).then_some((i + 1, name))
}

fn bad_doctype() -> Error {
    perr("This XSLT file has a malformed DOCTYPE.")
}

/// Locates a DOCTYPE in the prolog and vets its internal subset: only simple character entities (and comments)
/// are allowed. Returns the span and the declared entity names.
type Doctype<'a> = (usize, usize, Vec<&'a str>);

fn doctype(text: &str) -> Result<Option<Doctype<'_>>, Error> {
    let i = prolog_end(text);
    let rest = &text[i..];
    if !rest.starts_with("<!DOCTYPE") {
        if rest.get(..9).is_some_and(|h| h.eq_ignore_ascii_case("<!DOCTYPE")) {
            return Err(bad_doctype());
        }
        return Ok(None);
    }
    let b = text.as_bytes();
    let mut j = i + 9;
    let k = skip_ws(text, j);
    if k == j {
        return Err(bad_doctype());
    }
    j = k;
    let ns = j;
    while j < b.len() && !is_ws(b[j]) && !matches!(b[j], b'=' | b'/' | b'<' | b'>' | b'!' | b'?' | b'"' | b'\'') {
        j += 1;
    }
    if j == ns {
        return Err(bad_doctype());
    }
    j = skip_ws(text, j);
    let lb = text[j..].find('[').map(|p| p + j);
    let Some(gt) = text[j..].find('>').map(|p| p + j) else { return Err(bad_doctype()) };
    let Some(lb) = lb.filter(|lb| *lb < gt) else { return Ok(Some((i, gt + 1, Vec::new()))) };
    let mut pos = lb + 1;
    let mut ents = Vec::new();
    loop {
        let w = skip_ws(text, pos);
        if w > pos {
            pos = w;
            continue;
        }
        if text[pos..].starts_with("<!--") {
            // a comment of at most 2000 characters
            if let Some(e) = text[pos + 4..].find("-->") {
                if text[pos + 4..pos + 4 + e].chars().count() <= 2000 {
                    pos += 4 + e + 3;
                    continue;
                }
            }
            break;
        }
        if let Some((end, name)) = simple_entity(text, pos) {
            ents.push(name);
            pos = end;
            continue;
        }
        break;
    }
    if b.get(pos) == Some(&b']') {
        let q = skip_ws(text, pos + 1);
        if b.get(q) == Some(&b'>') {
            return Ok(Some((i, q + 1, ents)));
        }
    }
    Err(perr("This XSLT file's DOCTYPE declares more than simple character entities (such as <!ENTITY nbsp \"&#160;\">); refused for safety."))
}

// ---------------------------------------------------------------- tags

struct StartTag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, usize, usize, u8)>,
    end: usize,
    empty: bool,
}

fn parse_start<'a>(text: &'a str, lt: usize, ents: &HashSet<&str>) -> Result<StartTag<'a>, Error> {
    let b = text.as_bytes();
    let n = b.len();
    let bad = || wf("malformed start tag");
    let mut i = lt + 1;
    let ns = i;
    while i < n && !is_ws(b[i]) && b[i] != b'/' && b[i] != b'>' {
        i += 1;
    }
    let name = &text[ns..i];
    if !valid_qname(name) {
        return Err(bad());
    }
    let mut attrs: Vec<(&str, usize, usize, u8)> = Vec::new();
    loop {
        let before = i;
        while i < n && is_ws(b[i]) {
            i += 1;
        }
        if i >= n {
            return Err(bad());
        }
        if b[i] == b'>' {
            return Ok(StartTag { name, attrs, end: i + 1, empty: false });
        }
        if b[i] == b'/' {
            return if b.get(i + 1) == Some(&b'>') { Ok(StartTag { name, attrs, end: i + 2, empty: true }) } else { Err(bad()) };
        }
        if i == before {
            return Err(bad());
        }
        let an = i;
        while i < n && !is_ws(b[i]) && !matches!(b[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        let aname = &text[an..i];
        if !valid_qname(aname) {
            return Err(bad());
        }
        while i < n && is_ws(b[i]) {
            i += 1;
        }
        if i >= n || b[i] != b'=' {
            return Err(bad());
        }
        i += 1;
        while i < n && is_ws(b[i]) {
            i += 1;
        }
        if i >= n || !matches!(b[i], b'"' | b'\'') {
            return Err(bad());
        }
        let q = b[i];
        let vs = i + 1;
        let Some(len) = b[vs..].iter().position(|c| *c == q) else { return Err(bad()) };
        let ve = vs + len;
        let v = &text[vs..ve];
        if v.contains('<') {
            return Err(wf("'<' in an attribute value"));
        }
        if v.contains('&') {
            scan_refs(v, vs, ents, |_, _| {})?;
        }
        i = ve + 1;
        if attrs.iter().any(|a| a.0 == aname) {
            return Err(wf("duplicate attribute"));
        }
        attrs.push((aname, vs, ve, q));
    }
}

/// The start tag at `pos` of a text that is exactly one start tag (used to stamp `<html lang>`).
pub type TagAttrs = Vec<(String, usize, usize, u8)>;

pub fn single_start_tag(tag: &str) -> Option<(String, TagAttrs)> {
    if !tag.starts_with('<') {
        return None;
    }
    let st = parse_start(tag, 0, &HashSet::new()).ok()?;
    (st.end == tag.len()).then(|| (st.name.to_string(), st.attrs.iter().map(|(n, s, e, q)| (n.to_string(), *s, *e, *q)).collect()))
}

fn is_inline_name(local: &str) -> bool {
    if local.len() > 10 || !local.is_ascii() {
        return false;
    }
    let mut buf = [0u8; 10];
    for (i, c) in local.bytes().enumerate() {
        buf[i] = c.to_ascii_lowercase();
    }
    matches!(
        std::str::from_utf8(&buf[..local.len()]).unwrap_or(""),
        "a" | "abbr"
            | "acronym"
            | "b"
            | "basic-link"
            | "bdi"
            | "bdo"
            | "big"
            | "br"
            | "cite"
            | "code"
            | "data"
            | "del"
            | "dfn"
            | "em"
            | "font"
            | "i"
            | "img"
            | "inline"
            | "ins"
            | "kbd"
            | "label"
            | "mark"
            | "q"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strike"
            | "strong"
            | "sub"
            | "sup"
            | "time"
            | "tt"
            | "u"
            | "var"
            | "wbr"
    )
}

/// xsl:* elements whose whole subtree never holds document text.
fn skip_subtree(local: &str) -> bool {
    matches!(
        local,
        "accumulator"
            | "assert"
            | "character-map"
            | "comment"
            | "context-item"
            | "decimal-format"
            | "global-context-item"
            | "import"
            | "import-schema"
            | "include"
            | "key"
            | "message"
            | "mode"
            | "namespace"
            | "namespace-alias"
            | "output"
            | "preserve-space"
            | "processing-instruction"
            | "sort"
            | "strip-space"
            | "use-package"
    )
}

/// xsl:* instructions that stand inside a sentence as one protected point.
fn point_xsl(local: &str) -> bool {
    matches!(local, "apply-imports" | "apply-templates" | "call-template" | "copy-of" | "next-match" | "number" | "sequence" | "value-of")
}

fn yes_no(raw: &str) -> Option<bool> {
    match raw.trim().to_lowercase().as_str() {
        "yes" | "true" | "1" => Some(true),
        "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

fn ik_of(els: &[El], leaf_or_el: Child) -> Ik {
    match leaf_or_el {
        Child::Leaf(l) => {
            if l.kind == LeafKind::Text {
                Ik::Text
            } else {
                Ik::Point
            }
        }
        Child::El(i) => els[i].ik,
    }
}

fn compute_ik(els: &[El], i: usize) -> Ik {
    let el = &els[i];
    let has_el = el.children.iter().any(|c| matches!(c, Child::El(_)));
    if el.is_xsl {
        if point_xsl(el.local) {
            return if has_el && matches!(el.local, "apply-templates" | "call-template") { Ik::Break } else { Ik::Point };
        }
        if el.local == "text" && !has_el {
            return if !el.children.is_empty() && !el.skip { Ik::Paired } else { Ik::Point };
        }
        return Ik::Break;
    }
    if !is_inline_name(el.local) {
        return Ik::Break;
    }
    if el.skip || el.children.is_empty() {
        return Ik::Point;
    }
    if el.children.iter().all(|c| ik_of(els, *c) != Ik::Break) {
        Ik::Paired
    } else {
        Ik::Break
    }
}

// ---------------------------------------------------------------- the pass

struct Scopes<'a> {
    map: std::collections::HashMap<&'a str, Vec<String>>,
    log: Vec<&'a str>,
}

impl<'a> Scopes<'a> {
    fn push(&mut self, prefix: &'a str, uri: String) {
        self.map.entry(prefix).or_default().push(uri);
        self.log.push(prefix);
    }
    fn unwind(&mut self, mark: usize) {
        while self.log.len() > mark {
            if let Some(p) = self.log.pop() {
                if let Some(v) = self.map.get_mut(p) {
                    v.pop();
                }
            }
        }
    }
    fn get(&self, prefix: &str) -> Option<&str> {
        if prefix == "xml" {
            return Some(XML_NS);
        }
        self.map.get(prefix).and_then(|v| v.last()).map(String::as_str)
    }
}

pub fn lex<'a>(text: &'a str, l: &Limits) -> Result<Lexed<'a>, Error> {
    if let Some(c) = text.chars().find(|c| !xml_char(*c)) {
        return Err(wf(format!("character U+{:04X} is not allowed", c as u32)));
    }
    let dt = doctype(text)?;
    let ents: HashSet<&str> = dt.as_ref().map(|d| d.2.iter().copied().collect()).unwrap_or_default();
    let dspan = dt.as_ref().map(|d| (d.0, d.1));
    let max_depth = MAX_DEPTH.min(l.max_xml_depth);
    let b = text.as_bytes();
    let n = b.len();
    let mut els: Vec<El> = Vec::new();
    let mut stack: Vec<(usize, &str, usize)> = Vec::new(); // (element index, qname, scope mark)
    let mut scopes = Scopes { map: Default::default(), log: Vec::new() };
    let mut root: Option<usize> = None;
    let mut root_closed = false;
    let mut pos = 0usize;
    while pos < n {
        let lt = b[pos..].iter().position(|c| *c == b'<').map_or(n, |p| p + pos);
        if lt > pos {
            let region = &text[pos..lt];
            if let Some(&(top, _, _)) = stack.last() {
                if region.contains("]]>") {
                    return Err(wf("']]>' in text"));
                }
                let mut leaves: Vec<Leaf> = Vec::new();
                let mut at = pos;
                let mut ent_spans: Vec<(usize, usize)> = Vec::new();
                if region.contains('&') {
                    scan_refs(region, pos, &ents, |s, e| ent_spans.push((s, e)))?;
                }
                for (s, e) in ent_spans {
                    if s > at {
                        leaves.push(Leaf { kind: LeafKind::Text, s: at, e: s });
                    }
                    leaves.push(Leaf { kind: LeafKind::Entity, s, e });
                    at = e;
                }
                if lt > at {
                    leaves.push(Leaf { kind: LeafKind::Text, s: at, e: lt });
                }
                els[top].children.extend(leaves.into_iter().map(Child::Leaf));
            } else if !region.bytes().all(is_ws) {
                return Err(wf("text outside the root element"));
            }
        }
        if lt >= n {
            break;
        }
        if let Some((ds, de)) = dspan {
            if lt == ds {
                pos = de;
                continue;
            }
        }
        let rest = &text[lt..];
        let (end, kind) = if let Some(after) = rest.strip_prefix("<!--") {
            let Some(e) = after.find("-->") else { return Err(wf("unterminated comment")) };
            let body = &after[..e];
            if body.contains("--") || body.ends_with('-') {
                return Err(wf("'--' inside a comment"));
            }
            (lt + 4 + e + 3, LeafKind::Comment)
        } else if let Some(after) = rest.strip_prefix("<![CDATA[") {
            if stack.is_empty() {
                return Err(wf("CDATA outside the root element"));
            }
            let Some(e) = after.find("]]>") else { return Err(wf("unterminated CDATA section")) };
            (lt + 9 + e + 3, LeafKind::Cdata)
        } else if let Some(after) = rest.strip_prefix("<?") {
            let Some(e) = after.find("?>") else { return Err(wf("unterminated processing instruction")) };
            let body = &after[..e];
            let target = body.split(|c: char| c.is_ascii_whitespace()).next().unwrap_or("");
            if !ncname(target) {
                return Err(wf("bad processing instruction"));
            }
            if target.eq_ignore_ascii_case("xml") && lt != 0 {
                return Err(wf("the XML declaration is not at the start"));
            }
            (lt + 2 + e + 2, LeafKind::Pi)
        } else if rest.starts_with("</") {
            let Some(g) = rest.find('>') else { return Err(wf("unterminated end tag")) };
            let name = rest[2..g].trim_end_matches([' ', '\t', '\r', '\n']);
            let Some((idx, qn, mark)) = stack.pop() else { return Err(wf("unbalanced end tag")) };
            if name != qn {
                return Err(wf(format!("end tag </{name}> does not match <{qn}>")));
            }
            els[idx].cs = lt;
            els[idx].e = lt + g + 1;
            scopes.unwind(mark);
            if stack.is_empty() {
                root_closed = true;
            }
            pos = lt + g + 1;
            continue;
        } else if rest.starts_with("<!") {
            return Err(wf("unexpected declaration"));
        } else {
            if stack.is_empty() && (root_closed || root.is_some()) {
                return Err(wf("more than one root element"));
            }
            let st = parse_start(text, lt, &ents)?;
            if els.len() >= l.max_xml_elements {
                return Err(limit_err(LimitKind::XmlElements, "This XSLT file has more elements than the limit allows. Split it into smaller files."));
            }
            let mark = scopes.log.len();
            for (an, vs, ve, _) in &st.attrs {
                if *an == "xmlns" {
                    scopes.push("", unescape(&text[*vs..*ve]));
                } else if let Some(p) = an.strip_prefix("xmlns:") {
                    scopes.push(p, unescape(&text[*vs..*ve]));
                }
            }
            let (prefix, local) = match st.name.split_once(':') {
                Some((p, l)) => (p, l),
                None => ("", st.name),
            };
            let uri: Option<&str> = match scopes.get(prefix) {
                Some(u) => Some(u).filter(|u| !u.is_empty()),
                None if prefix.is_empty() => None,
                None => return Err(wf(format!("namespace prefix {prefix} is not defined"))),
            };
            let is_xsl = uri == Some(XSL_NS);
            let code_ns = matches!(uri, None | Some(XHTML_NS) | Some(SVG_NS));
            let mut attrs = Vec::with_capacity(st.attrs.len());
            for (an, vs, ve, q) in &st.attrs {
                let (ap, al) = match an.split_once(':') {
                    Some((p, l)) => (p, l),
                    None => ("", *an),
                };
                let ns = if ap.is_empty() {
                    AttrNs::None
                } else if ap == "xmlns" {
                    AttrNs::Other
                } else {
                    match scopes.get(ap) {
                        None => return Err(wf(format!("namespace prefix {ap} is not defined"))),
                        Some(XSL_NS) => AttrNs::Xsl,
                        Some(XML_NS) => AttrNs::Xml,
                        Some(ITS_NS) => AttrNs::Its,
                        Some(_) => AttrNs::Other,
                    }
                };
                attrs.push(Attr { name: an, local: al, prefixed: !ap.is_empty(), ns, vs: *vs, ve: *ve, quote: *q });
            }
            // flags the planner needs
            let raw = |a: &Attr| text[a.vs..a.ve].to_string();
            let find = |name: &str, ns: AttrNs| attrs.iter().find(|a| if ns == AttrNs::None { a.name == name } else { a.local == name && a.ns == ns });
            let expand_text = if is_xsl { find("expand-text", AttrNs::None) } else { find("expand-text", AttrNs::Xsl) }.and_then(|a| yes_no(&raw(a)));
            let preserve = find("space", AttrNs::Xml).map(|a| raw(a).trim() == "preserve");
            let doe = is_xsl && local == "text" && find("disable-output-escaping", AttrNs::None).is_some_and(|a| yes_no(&raw(a)) == Some(true));
            let attr_name_ok = is_xsl && local == "attribute" && find("name", AttrNs::None).is_some_and(|a| TRANSLATABLE_ATTRIBUTES.contains(&raw(a).trim()));
            let skip = if is_xsl {
                skip_subtree(local) || doe
            } else {
                (code_ns && (local.eq_ignore_ascii_case("script") || local.eq_ignore_ascii_case("style")))
                    || [find("translate", AttrNs::None), find("translate", AttrNs::Its)].into_iter().flatten().any(|a| raw(a).trim().to_lowercase() == "no")
            };
            let idx = els.len();
            els.push(El {
                s: lt,
                e: st.end,
                oe: st.end,
                cs: st.end,
                local,
                is_xsl,

                attrs,
                children: Vec::new(),
                skip,
                expand_text,
                preserve,
                attr_name_ok,
                ik: Ik::Break,
            });
            if let Some(&(top, _, _)) = stack.last() {
                els[top].children.push(Child::El(idx));
            } else {
                root = Some(idx);
            }
            if st.empty {
                scopes.unwind(mark);
                if stack.is_empty() {
                    root_closed = true;
                }
            } else {
                stack.push((idx, st.name, mark));
                if stack.len() > max_depth {
                    return Err(limit_err(LimitKind::XmlDepth, format!("This XSLT file nests elements more than {max_depth} deep; refused.")));
                }
            }
            pos = st.end;
            continue;
        };
        if let Some(&(top, _, _)) = stack.last() {
            els[top].children.push(Child::Leaf(Leaf { kind, s: lt, e: end }));
        }
        pos = end;
    }
    let Some(root) = root else { return Err(wf("no root element")) };
    if !stack.is_empty() {
        return Err(wf("unclosed element"));
    }
    for i in (0..els.len()).rev() {
        let ik = compute_ik(&els, i);
        els[i].ik = ik;
    }
    Ok(Lexed { els, root, ents })
}

/// Span of the (vetted) DOCTYPE, so the generic XML guard can be run on the text without it.
pub fn doctype_span(text: &str) -> Result<Option<(usize, usize)>, Error> {
    Ok(doctype(text)?.map(|d| (d.0, d.1)))
}
