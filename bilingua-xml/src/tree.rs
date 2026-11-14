//! A well-formed XML document as a tree of exact byte spans.
//!
//! The text is tokenised with `quick-xml` (which never expands an entity or reads a DTD); this module adds what a
//! translation tool needs on top: the span of every start tag, end tag, attribute value and text run (so a merge can
//! rewrite only what changed), XML 1.0 well-formedness checks of the kind libxml2 makes (names, characters,
//! duplicate attributes, undefined namespace prefixes, undefined entities), and namespace resolution. A DTD internal
//! subset is kept as raw bytes and never read: no entity is ever expanded or fetched. The depth and element limits
//! are counted here exactly, on the same tokens the reader sees. Nothing here allocates per character.

use bilingua_model::{Error, LimitError, LimitKind, Limits};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

pub(crate) const NONE: u32 = u32::MAX;
pub(crate) const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
pub(crate) const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Text,
    Cdata,
    Comment,
    Pi,
    Doctype,
    Elem,
}

/// One child of an element (or of the document): a run of text, a CDATA section, a comment, a processing
/// instruction, a DOCTYPE or an element (`el` is its index).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Item {
    pub kind: Kind,
    pub s: usize,
    pub e: usize,
    pub el: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Attr {
    /// Namespace id (0 = none).
    pub ns: u32,
    pub name_s: usize,
    pub name_e: usize,
    /// Start of the local part of the name (after the prefix colon).
    pub local_s: usize,
    pub val_s: usize,
    pub val_e: usize,
    pub quote: u8,
}

#[derive(Clone, Debug)]
pub(crate) struct El {
    pub s0: usize,
    pub s1: usize,
    /// End tag span (equal to `s1` for an empty-element tag).
    pub e0: usize,
    pub e1: usize,
    pub name_s: usize,
    pub name_e: usize,
    pub local_s: usize,
    pub ns: u32,
    pub empty: bool,
    pub parent: u32,
    pub attr_lo: u32,
    pub attr_hi: u32,
    pub items: Vec<Item>,
}

pub(crate) struct Tree<'a> {
    pub text: &'a str,
    pub els: Vec<El>,
    pub attrs: Vec<Attr>,
    pub uris: Vec<String>,
    pub prolog: Vec<Item>,
    pub epilog: Vec<Item>,
    pub root: u32,
    pub has_doctype: bool,
    /// The DOCTYPE has an internal subset (`[ ... ]`).
    pub has_subset: bool,
}

fn bad(msg: impl Into<String>) -> Error {
    Error::Parse { format: "xml".into(), msg: msg.into() }
}

pub(crate) fn is_name_start(c: char) -> bool {
    matches!(c, ':' | 'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}'
        | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}

pub(crate) fn is_name_char(c: char) -> bool {
    is_name_start(c) || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

pub(crate) fn valid_name(s: &str) -> bool {
    let mut it = s.chars();
    it.next().is_some_and(is_name_start) && it.all(is_name_char)
}

/// An XML 1.0 `Char`.
pub(crate) fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

pub(crate) fn is_xml_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

/// The code point of a character reference body (`#65` or `#x41`), if it names a legal XML character.
pub(crate) fn char_ref(body: &str) -> Option<char> {
    let n = if let Some(h) = body.strip_prefix("#x") {
        if h.is_empty() || h.len() > 8 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(h, 16).ok()?
    } else {
        let d = body.strip_prefix('#')?;
        if d.is_empty() || d.len() > 9 || !d.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        d.parse::<u32>().ok()?
    };
    char::from_u32(n).filter(|c| is_xml_char(*c))
}

pub(crate) fn predefined(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}

/// Decodes the predefined entities and character references of `raw`; any other `&name;` is passed to `other`,
/// which returns the text to put in its place (or `None` to keep it as written).
pub(crate) fn decode_refs(raw: &str, mut other: impl FnMut(&str) -> Option<String>) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        let Some(semi) = after.find(';') else {
            out.push_str(&rest[p..]);
            return out;
        };
        let body = &after[..semi];
        let whole = &rest[p..p + semi + 2];
        if body.starts_with('#') {
            match char_ref(body) {
                Some(c) => out.push(c),
                None => out.push_str(whole),
            }
        } else if let Some(c) = predefined(body) {
            out.push(c);
        } else {
            match other(whole) {
                Some(s) => out.push_str(&s),
                None => out.push_str(whole),
            }
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    out
}

/// Splits a start tag (`<a b="c" d='e'/>`) into its name span and attributes; offsets are relative to `base`.
struct RawTag {
    name_e: usize,
    attrs: Vec<(usize, usize, usize, usize, u8)>, // name_s, name_e, val_s, val_e, quote (absolute)
    empty: bool,
}

fn scan_start_tag(text: &str, s0: usize, s1: usize) -> Result<RawTag, Error> {
    let b = text.as_bytes();
    let mut i = s0 + 1;
    let name_s = i;
    while i < s1 && !is_xml_ws(b[i]) && b[i] != b'/' && b[i] != b'>' {
        i += 1;
    }
    let name_e = i;
    if !valid_name(&text[name_s..name_e]) {
        return Err(bad(format!("invalid element name {:?}", &text[name_s..name_e])));
    }
    let mut attrs = Vec::new();
    let mut empty = false;
    let mut need_ws = false;
    loop {
        let had_ws = i < s1 && is_xml_ws(b[i]);
        while i < s1 && is_xml_ws(b[i]) {
            i += 1;
        }
        if i >= s1 {
            return Err(bad("unterminated start tag"));
        }
        match b[i] {
            b'>' => break,
            b'/' => {
                if i + 2 != s1 || b[i + 1] != b'>' {
                    return Err(bad("stray '/' in a start tag"));
                }
                empty = true;
                break;
            }
            _ => {}
        }
        if need_ws && !had_ws {
            return Err(bad("attributes must be separated by white space"));
        }
        let a_s = i;
        while i < s1 && !is_xml_ws(b[i]) && b[i] != b'=' && b[i] != b'>' && b[i] != b'/' {
            i += 1;
        }
        let a_e = i;
        if !valid_name(&text[a_s..a_e]) {
            return Err(bad(format!("invalid attribute name {:?}", &text[a_s..a_e])));
        }
        while i < s1 && is_xml_ws(b[i]) {
            i += 1;
        }
        if i >= s1 || b[i] != b'=' {
            return Err(bad(format!("attribute {:?} has no value", &text[a_s..a_e])));
        }
        i += 1;
        while i < s1 && is_xml_ws(b[i]) {
            i += 1;
        }
        if i >= s1 || (b[i] != b'"' && b[i] != b'\'') {
            return Err(bad(format!("attribute {:?} value is not quoted", &text[a_s..a_e])));
        }
        let q = b[i];
        let v_s = i + 1;
        let Some(len) = b[v_s..s1].iter().position(|c| *c == q) else {
            return Err(bad("unterminated attribute value"));
        };
        let v_e = v_s + len;
        check_value(&text[v_s..v_e], true)?;
        attrs.push((a_s, a_e, v_s, v_e, q));
        i = v_e + 1;
        need_ws = true;
    }
    Ok(RawTag { name_e, attrs, empty })
}

/// Character and reference checks shared by text and attribute values.
fn check_value(s: &str, attr: bool) -> Result<(), Error> {
    for c in s.chars() {
        if !is_xml_char(c) {
            return Err(bad(format!("illegal character U+{:04X}", c as u32)));
        }
        if attr && c == '<' {
            return Err(bad("'<' is not allowed in an attribute value"));
        }
    }
    Ok(())
}

/// Every `&...;` in `s` must be well formed; `defined_ok` allows entity names beyond the five predefined ones.
fn check_refs(s: &str, defined_ok: bool) -> Result<(), Error> {
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        let after = &rest[p + 1..];
        let Some(semi) = after.find(';') else {
            return Err(bad("an '&' is not part of a reference (write &amp;)"));
        };
        let body = &after[..semi];
        if body.starts_with('#') {
            if char_ref(body).is_none() {
                return Err(bad(format!("invalid character reference &{body};")));
            }
        } else if !valid_name(body) {
            return Err(bad("an '&' is not part of a reference (write &amp;)"));
        } else if predefined(body).is_none() && !defined_ok {
            return Err(bad(format!("undefined entity &{body};")));
        }
        rest = &after[semi + 1..];
    }
    Ok(())
}

struct Scope<'a> {
    prefix: &'a str,
    uri: u32,
}

/// The namespace declarations in scope. A map from each prefix to its innermost binding keeps a lookup constant, so a
/// start tag with thousands of `xmlns:pN` declarations and as many `pN:a` attributes (or a root with thousands of
/// declarations above many elements) does not make the scan quadratic.
#[derive(Default)]
struct ScopeStack<'a> {
    items: Vec<Scope<'a>>,
    /// per item: the binding of its prefix before it was pushed
    prev: Vec<Option<u32>>,
    now: HashMap<&'a str, u32>,
}

impl<'a> ScopeStack<'a> {
    fn len(&self) -> usize {
        self.items.len()
    }
    fn push(&mut self, s: Scope<'a>) {
        self.prev.push(self.now.insert(s.prefix, s.uri));
        self.items.push(s);
    }
    fn truncate(&mut self, n: usize) {
        while self.items.len() > n {
            let s = self.items.pop().expect("len checked");
            match self.prev.pop().flatten() {
                Some(u) => self.now.insert(s.prefix, u),
                None => self.now.remove(s.prefix),
            };
        }
    }
    fn get(&self, prefix: &str) -> Option<u32> {
        self.now.get(prefix).copied()
    }
}

impl<'a> Tree<'a> {
    /// Parses `text` (already decoded, no BOM), refusing more than `limits.max_xml_elements` elements or a nesting
    /// deeper than `limits.max_xml_depth`.
    pub(crate) fn parse(text: &'a str, limits: &Limits) -> Result<Tree<'a>, Error> {
        if text.starts_with('\u{FEFF}') {
            return Err(bad("a second byte order mark at the start of the file"));
        }
        // where the DOCTYPE really ends; the reader must agree, or the file is refused
        let dt_span = doctype_span(text);
        let masked = dt_span.and_then(|(i, j)| mask_doctype(text, i, j));
        let mut rd = Reader::from_str(masked.as_deref().unwrap_or(text));
        {
            let c = rd.config_mut();
            c.check_end_names = true;
            c.expand_empty_elements = false;
            c.allow_dangling_amp = true; // checked by hand below
        }
        let mut t = Tree {
            text,
            els: Vec::new(),
            attrs: Vec::new(),
            uris: vec![String::new()],
            prolog: Vec::new(),
            epilog: Vec::new(),
            root: NONE,
            has_doctype: false,
            has_subset: false,
        };
        let mut uri_ids: HashMap<String, u32> = HashMap::new();
        let mut stack: Vec<u32> = Vec::new();
        let mut scopes: ScopeStack<'a> = ScopeStack::default();
        let mut scope_marks: Vec<usize> = Vec::new();
        let mut pos = 0usize;
        let mut seen_root = false;
        loop {
            let ev = rd.read_event().map_err(|e| bad(e.to_string()))?;
            let end = rd.buffer_position() as usize;
            let start = pos;
            pos = end;
            let mut text_like = false;
            match ev {
                Event::Eof => break,
                Event::Text(_) | Event::GeneralRef(_) => {
                    text_like = true;
                    let raw = &text[start..end];
                    let in_root = !stack.is_empty();
                    if !in_root {
                        if !raw.bytes().all(is_xml_ws) {
                            return Err(bad("text outside the root element"));
                        }
                    } else {
                        check_value(raw, false)?;
                        if raw.contains("]]>") {
                            return Err(bad("']]>' is not allowed in text"));
                        }
                        check_refs(raw, t.has_doctype)?;
                    }
                }
                Event::Start(_) | Event::Empty(_) => {
                    let empty = matches!(ev, Event::Empty(_));
                    if stack.is_empty() && seen_root {
                        return Err(bad("more than one root element"));
                    }
                    seen_root = true;
                    let raw = scan_start_tag(text, start, end)?;
                    let idx = t.els.len() as u32;
                    // namespaces
                    let mark = scopes.len();
                    for &(a_s, a_e, v_s, v_e, _) in &raw.attrs {
                        // references in attribute values follow the text rules (`&#0;`, a bare `&` are refused)
                        check_refs(&text[v_s..v_e], t.has_doctype)?;
                        let name = &text[a_s..a_e];
                        let decl = if name == "xmlns" {
                            Some("")
                        } else {
                            name.strip_prefix("xmlns:")
                        };
                        if let Some(prefix) = decl {
                            let uri = decode_refs(&text[v_s..v_e], |_| None);
                            if (prefix == "xml") != (uri == XML_NS) && prefix == "xml" {
                                return Err(bad("the xml prefix is reserved"));
                            }
                            if prefix == "xmlns" || uri == XMLNS_NS {
                                return Err(bad("the xmlns namespace is reserved"));
                            }
                            if !prefix.is_empty() && uri.is_empty() {
                                return Err(bad("a prefix cannot be bound to an empty namespace"));
                            }
                            let id = if uri.is_empty() {
                                0
                            } else {
                                *uri_ids.entry(uri.clone()).or_insert_with(|| {
                                    t.uris.push(uri);
                                    (t.uris.len() - 1) as u32
                                })
                            };
                            scopes.push(Scope { prefix, uri: id });
                        }
                    }
                    let lookup = |prefix: &str, scopes: &ScopeStack<'a>, uris: &mut Vec<String>, ids: &mut HashMap<String, u32>| -> Option<u32> {
                        if prefix == "xml" {
                            return Some(*ids.entry(XML_NS.to_string()).or_insert_with(|| {
                                uris.push(XML_NS.to_string());
                                (uris.len() - 1) as u32
                            }));
                        }
                        scopes.get(prefix)
                    };
                    let qname = &text[start + 1..raw.name_e];
                    let (el_prefix, local) = split_qname(qname)?;
                    let ns = if el_prefix.is_empty() {
                        lookup("", &scopes, &mut t.uris, &mut uri_ids).unwrap_or(0)
                    } else {
                        lookup(el_prefix, &scopes, &mut t.uris, &mut uri_ids)
                            .ok_or_else(|| bad(format!("namespace prefix {el_prefix} is not defined")))?
                    };
                    let attr_lo = t.attrs.len() as u32;
                    for &(a_s, a_e, v_s, v_e, q) in &raw.attrs {
                        let name = &text[a_s..a_e];
                        let (ap, al) = split_qname(name)?;
                        let (ans, local_s) = if name == "xmlns" || ap == "xmlns" {
                            (0, a_e - al.len())
                        } else if ap.is_empty() {
                            (0, a_s)
                        } else {
                            let id = lookup(ap, &scopes, &mut t.uris, &mut uri_ids)
                                .ok_or_else(|| bad(format!("namespace prefix {ap} is not defined")))?;
                            (id, a_e - al.len())
                        };
                        t.attrs.push(Attr { ns: ans, name_s: a_s, name_e: a_e, local_s, val_s: v_s, val_e: v_e, quote: q });
                    }
                    let attr_hi = t.attrs.len() as u32;
                    // duplicate attributes (same expanded name)
                    let mine = &t.attrs[attr_lo as usize..attr_hi as usize];
                    if mine.len() > 1 {
                        if mine.len() <= 12 {
                            for i in 0..mine.len() {
                                for j in i + 1..mine.len() {
                                    let (a, b) = (&mine[i], &mine[j]);
                                    if a.ns == b.ns && text[a.local_s..a.name_e] == text[b.local_s..b.name_e] && (a.ns != 0 || text[a.name_s..a.name_e] == text[b.name_s..b.name_e]) {
                                        return Err(bad(format!("duplicate attribute {}", &text[a.name_s..a.name_e])));
                                    }
                                }
                            }
                        } else {
                            let mut seen = std::collections::HashSet::new();
                            for a in mine {
                                if !seen.insert((a.ns, &text[a.local_s..a.name_e], if a.ns == 0 { &text[a.name_s..a.name_e] } else { "" })) {
                                    return Err(bad(format!("duplicate attribute {}", &text[a.name_s..a.name_e])));
                                }
                            }
                        }
                    }
                    if t.els.len() >= limits.max_xml_elements {
                        return Err(LimitError::new(
                            LimitKind::XmlElements,
                            format!("This file has more than {} XML elements in one part. It was not processed.", limits.max_xml_elements),
                        )
                        .into());
                    }
                    if !empty && stack.len() >= limits.max_xml_depth {
                        return Err(LimitError::new(
                            LimitKind::XmlDepth,
                            format!("This file nests XML elements more than {} deep. It was not processed.", limits.max_xml_depth),
                        )
                        .into());
                    }
                    let parent = stack.last().copied().unwrap_or(NONE);
                    t.els.push(El {
                        s0: start,
                        s1: end,
                        e0: if empty { end } else { usize::MAX },
                        e1: if empty { end } else { usize::MAX },
                        name_s: start + 1,
                        name_e: raw.name_e,
                        local_s: raw.name_e - local.len(),
                        ns,
                        empty: empty || raw.empty,
                        parent,
                        attr_lo,
                        attr_hi,
                        items: Vec::new(),
                    });
                    if parent == NONE {
                        t.root = idx;
                    } else {
                        t.els[parent as usize].items.push(Item { kind: Kind::Elem, s: start, e: end, el: idx });
                    }
                    if empty {
                        scopes.truncate(mark);
                    } else {
                        stack.push(idx);
                        scope_marks.push(mark);
                    }
                }
                Event::End(_) => {
                    let Some(idx) = stack.pop() else {
                        return Err(bad("unexpected end tag"));
                    };
                    let el = &mut t.els[idx as usize];
                    el.e0 = start;
                    el.e1 = end;
                    if let Some(m) = scope_marks.pop() {
                        scopes.truncate(m);
                    }
                }
                Event::CData(_) => {
                    if stack.is_empty() {
                        return Err(bad("CDATA outside the root element"));
                    }
                    let inner = &text[start + 9..end - 3];
                    check_value(inner, false)?;
                    push_item(&mut t, &stack, Kind::Cdata, start, end);
                }
                Event::Comment(_) => {
                    let inner = &text[start + 4..end - 3];
                    check_value(inner, false)?;
                    if inner.contains("--") || inner.ends_with('-') {
                        return Err(bad("'--' is not allowed inside a comment"));
                    }
                    push_item(&mut t, &stack, Kind::Comment, start, end);
                }
                Event::PI(_) | Event::Decl(_) => {
                    check_value(&text[start..end], false)?;
                    push_item(&mut t, &stack, Kind::Pi, start, end);
                }
                Event::DocType(_) => {
                    if !stack.is_empty() || seen_root || t.has_doctype {
                        return Err(bad("misplaced DOCTYPE"));
                    }
                    // the reader stops a DOCTYPE at the first '>' outside a '<' ... '>' pair; it must stop where the
                    // quotes, brackets and comments say, or the subset could hide document content (or the reverse)
                    if dt_span.map(|(i, j)| (i, j + 1)) != Some((start, end)) {
                        return Err(bad("the DOCTYPE does not end where its quotes, brackets and comments say it does; refused"));
                    }
                    let raw = &text[start..end];
                    // XML characters only: a NUL here would also be read as a skeleton marker
                    check_value(raw, false)?;
                    // An internal subset is kept as it is: no entity is ever expanded or fetched, a `&name;` stays a code.
                    t.has_subset = doctype_has_subset(raw);
                    t.has_doctype = true;
                    t.prolog.push(Item { kind: Kind::Doctype, s: start, e: end, el: NONE });
                }
            }
            if text_like {
                // merge into the previous text item when adjacent
                let items = match stack.last() {
                    Some(&p) => &mut t.els[p as usize].items,
                    None if seen_root => &mut t.epilog,
                    None => &mut t.prolog,
                };
                match items.last_mut() {
                    Some(it) if it.kind == Kind::Text && it.e == start => it.e = end,
                    _ => items.push(Item { kind: Kind::Text, s: start, e: end, el: NONE }),
                }
            }
        }
        if !stack.is_empty() {
            return Err(bad("unclosed element (the file ends inside an element)"));
        }
        if t.root == NONE {
            return Err(bad("no root element"));
        }
        if pos < text.len() && !text[pos..].bytes().all(is_xml_ws) {
            return Err(bad("trailing content after the root element"));
        }
        Ok(t)
    }

    pub(crate) fn qname(&self, el: u32) -> &str {
        let e = &self.els[el as usize];
        &self.text[e.name_s..e.name_e]
    }

    pub(crate) fn local(&self, el: u32) -> &str {
        let e = &self.els[el as usize];
        &self.text[e.local_s..e.name_e]
    }

    pub(crate) fn uri(&self, ns: u32) -> &str {
        &self.uris[ns as usize]
    }

    pub(crate) fn attrs_of(&self, el: u32) -> &[Attr] {
        let e = &self.els[el as usize];
        &self.attrs[e.attr_lo as usize..e.attr_hi as usize]
    }

    pub(crate) fn attr_local(&self, a: &Attr) -> &str {
        &self.text[a.local_s..a.name_e]
    }

    pub(crate) fn attr_qname(&self, a: &Attr) -> &str {
        &self.text[a.name_s..a.name_e]
    }

    pub(crate) fn attr_raw(&self, a: &Attr) -> &str {
        &self.text[a.val_s..a.val_e]
    }

    /// The value of the attribute with this expanded name (`ns` empty = no namespace), references decoded.
    pub(crate) fn attr_value(&self, el: u32, ns: &str, local: &str) -> Option<String> {
        self.find_attr(el, ns, local).map(|a| decode_refs(self.attr_raw(a), |_| None))
    }

    pub(crate) fn find_attr(&self, el: u32, ns: &str, local: &str) -> Option<&Attr> {
        self.attrs_of(el).iter().find(|a| self.attr_local(a) == local && self.uri(a.ns) == ns && !self.is_ns_decl(a))
    }

    pub(crate) fn is_ns_decl(&self, a: &Attr) -> bool {
        let n = self.attr_qname(a);
        n == "xmlns" || n.starts_with("xmlns:")
    }

    pub(crate) fn el_ns(&self, el: u32) -> &str {
        self.uri(self.els[el as usize].ns)
    }

    /// In-scope namespace bindings (prefix, uri) at `el`, nearest declaration winning; the default namespace is
    /// left out (XPath name tests never use it).
    pub(crate) fn nsmap(&self, el: u32) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut cur = el;
        while cur != NONE {
            for a in self.attrs_of(cur) {
                if let Some(p) = self.attr_qname(a).strip_prefix("xmlns:") {
                    if seen.insert(p) {
                        out.push((p.to_string(), decode_refs(self.attr_raw(a), |_| None)));
                    }
                }
            }
            cur = self.els[cur as usize].parent;
        }
        out
    }

    /// The decoded text content of an element and everything below it (stops once `out` holds `cap` bytes).
    pub(crate) fn string_value(&self, el: u32, out: &mut String, cap: usize) {
        // iterative: a document may nest as deep as the caller's depth limit allows
        let mut stack: Vec<(u32, usize)> = vec![(el, 0)];
        while let Some((e, i)) = stack.pop() {
            if out.len() >= cap {
                return;
            }
            let items = &self.els[e as usize].items;
            let Some(it) = items.get(i) else { continue };
            stack.push((e, i + 1));
            match it.kind {
                Kind::Text => out.push_str(&decode_refs(&self.text[it.s..it.e], |_| None)),
                Kind::Cdata => out.push_str(&self.text[it.s + 9..it.e - 3]),
                Kind::Elem => stack.push((it.el, 0)),
                _ => {}
            }
        }
    }
}

fn push_item(t: &mut Tree, stack: &[u32], kind: Kind, s: usize, e: usize) {
    let it = Item { kind, s, e, el: NONE };
    match stack.last() {
        Some(&p) => t.els[p as usize].items.push(it),
        None if t.root != NONE => t.epilog.push(it),
        None => t.prolog.push(it),
    }
}

fn split_qname(q: &str) -> Result<(&str, &str), Error> {
    match q.find(':') {
        None => Ok(("", q)),
        Some(i) => {
            let (p, l) = (&q[..i], &q[i + 1..]);
            if p.is_empty() || l.is_empty() || l.contains(':') || !valid_name(p) || !valid_name(l) {
                return Err(bad(format!("invalid qualified name {q:?}")));
            }
            Ok((p, l))
        }
    }
}

fn is_ws_char(c: char) -> bool {
    c.is_ascii() && is_xml_ws(c as u8)
}

/// The DOCTYPE of the prolog (after white space, comments and processing instructions; `<!DOCTYPE` in any case, as
/// the reader takes it): `(start, j)` where `j` is the index of its closing `>`, found with quoted strings, the
/// `[ ... ]` internal subset and the comments and processing instructions inside it skipped (the same scan as
/// `bilingua_model::guard`). `None` when there is no DOCTYPE there or it never ends.
fn doctype_span(text: &str) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    let mut i = 0usize;
    loop {
        while i < b.len() && is_xml_ws(b[i]) {
            i += 1;
        }
        if b[i..].starts_with(b"<?") {
            i += 2 + text[i + 2..].find("?>")? + 2;
        } else if b[i..].starts_with(b"<!--") {
            i += 4 + text[i + 4..].find("-->")? + 3;
        } else {
            break;
        }
    }
    if b.len() < i + 9 || !b[i..i + 9].eq_ignore_ascii_case(b"<!DOCTYPE") {
        return None;
    }
    let (mut j, mut quote, mut depth) = (i + 9, 0u8, 0usize);
    while j < b.len() {
        let c = b[j];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if depth > 0 && b[j..].starts_with(b"<!--") {
            j += 4 + text[j + 4..].find("-->")? + 2;
        } else if depth > 0 && b[j..].starts_with(b"<?") {
            j += 2 + text[j + 2..].find("?>")? + 1;
        } else if c == b'[' {
            depth += 1;
        } else if c == b']' {
            depth = depth.saturating_sub(1);
        } else if c == b'>' && depth == 0 {
            return Some((i, j));
        }
        j += 1;
    }
    None
}

/// The reader stops a DOCTYPE at the first `>` outside a `<` ... `>` pair, even one inside a quoted identifier or
/// a comment of the internal subset. For the DOCTYPE at `i..=j` (from [`doctype_span`]) this returns a copy of
/// `text` whose DOCTYPE is blanked (after its name) to the same length, so every offset stays the same and the
/// reader stops at `j`. `None` when the DOCTYPE needs no help.
fn mask_doctype(text: &str, i: usize, j: usize) -> Option<String> {
    let inner = &text[i + 9..j];
    if !inner.contains(['[', '"', '\'', '<']) {
        return None;
    }
    // keep the white space and the document type name; blank what follows
    let lead = inner.len() - inner.trim_start_matches(is_ws_char).len();
    let name = inner[lead..].find(|c: char| is_ws_char(c) || matches!(c, '[' | '"' | '\'' | '<' | '>')).unwrap_or(inner.len() - lead);
    let keep = lead + name;
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..i + 9 + keep]);
    for ch in text[i + 9 + keep..j].chars() {
        // keep line breaks and the byte length; a blank is one byte, so a wide character is several
        if ch == '\n' || ch == '\r' {
            out.push(ch);
        } else {
            for _ in 0..ch.len_utf8() {
                out.push(' ');
            }
        }
    }
    out.push_str(&text[j..]);
    Some(out)
}

/// True when a `<!DOCTYPE ...>` declares an internal subset (`[` outside the quoted identifiers).
fn doctype_has_subset(raw: &str) -> bool {
    let mut quote = 0u8;
    for &c in raw.as_bytes() {
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'[' {
            return true;
        }
    }
    false
}

