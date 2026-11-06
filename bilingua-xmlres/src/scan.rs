//! Reads an Android `strings.xml` or a Java XML properties file with `quick-xml` and reports every resource slot
//! (in file order), the byte spans of its element and inner XML in the decoded text, and its content as text and
//! inline markup pieces. Android `<plurals>` groups are reported with their items for the plural fill on merge.

use bilingua_model::Error;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// Which vocabulary is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Android,
    JavaProps,
}

impl Dialect {
    pub fn id(self) -> &'static str {
        match self {
            Dialect::Android => "android",
            Dialect::JavaProps => "javaxml",
        }
    }
    fn root(self) -> &'static [u8] {
        match self {
            Dialect::Android => b"resources",
            Dialect::JavaProps => b"properties",
        }
    }
    fn what(self) -> &'static str {
        match self {
            Dialect::Android => "This Android strings file",
            Dialect::JavaProps => "This Java XML properties file",
        }
    }
}

fn parse_err(d: Dialect, msg: impl Into<String>) -> Error {
    Error::Parse { format: d.id().into(), msg: msg.into() }
}

/// One piece of a slot's content: decoded text, or the exact markup of an inline element (start tag, end tag) or of
/// a protected point (Android `<xliff:g>` with its content, a comment, a processing instruction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Open(String),
    Close(String),
    Point(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    String,
    Plural(String),
    Array(usize),
    Entry,
}

#[derive(Debug, Clone)]
pub struct Slot {
    pub name: String,
    pub kind: Kind,
    /// translatable="false", a resource reference, or (Java) an entry with child elements, comments or PIs.
    pub skip: bool,
    /// Byte span of the inner XML; `None` for an empty element.
    pub inner: Option<(usize, usize)>,
    pub pieces: Vec<Piece>,
    pub cdata: bool,
    /// The comment right before the resource (Android), for the translator.
    pub comment: String,
    /// Byte offset of the start tag (for `line N`).
    pub start: usize,
}

/// An `<item>` of a `<plurals>`: element span, quantity, inner span.
#[derive(Debug, Clone)]
pub struct Item {
    pub start: usize,
    pub end: usize,
    pub quantity: String,
    pub inner: Option<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct Group {
    /// translatable="false" on the `<plurals>`: never changed.
    pub off: bool,
    pub items: Vec<Item>,
}

pub struct Scan {
    pub slots: Vec<Slot>,
    pub groups: Vec<Group>,
}

fn local(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|c| *c == b':') {
        Some(p) => &name[p + 1..],
        None => name,
    }
}

fn xml_char(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn push_ref(d: Dialect, out: &mut String, name: &str) -> Result<(), Error> {
    match name {
        "amp" => out.push('&'),
        "lt" => out.push('<'),
        "gt" => out.push('>'),
        "quot" => out.push('"'),
        "apos" => out.push('\''),
        _ => {
            let n = name.strip_prefix('#').and_then(|v| match v.strip_prefix('x') {
                Some(h) => u32::from_str_radix(h, 16).ok(),
                None => v.parse::<u32>().ok(),
            });
            match n.filter(|n| xml_char(*n)).and_then(char::from_u32) {
                Some(c) => out.push(c),
                None if name.starts_with('#') => return Err(parse_err(d, format!("invalid character reference &{name};"))),
                None => return Err(parse_err(d, format!("{} uses an undefined entity (&{name};); refused.", d.what()))),
            }
        }
    }
    Ok(())
}

/// Line ends as an XML parser reports them (CRLF and CR read as LF).
fn norm_nl(raw: &str) -> String {
    if raw.contains('\r') {
        raw.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        raw.to_string()
    }
}

#[derive(Default)]
struct Attrs {
    name: Option<String>,
    key: Option<String>,
    translatable_false: bool,
    quantity: Option<String>,
}

fn attrs(d: Dialect, e: &BytesStart) -> Result<Attrs, Error> {
    let mut a = Attrs::default();
    for at in e.attributes() {
        let at = at.map_err(|x| parse_err(d, format!("{} is not well-formed XML: bad attribute: {x}", d.what())))?;
        let raw = std::str::from_utf8(&at.value).map_err(|_| parse_err(d, "an attribute value is not UTF-8"))?;
        // undefined entities in attributes are refused too (LocOS refuses any entity left in the tree)
        let mut v = String::new();
        let mut rest = raw;
        while let Some(p) = rest.find('&') {
            v.push_str(&rest[..p]);
            let Some(end) = rest[p..].find(';') else {
                return Err(parse_err(d, format!("{} is not well-formed XML: a bare & in an attribute", d.what())));
            };
            push_ref(d, &mut v, &rest[p + 1..p + end])?;
            rest = &rest[p + end + 1..];
        }
        v.push_str(rest);
        // unprefixed attributes only, as lxml's el.get("name")
        match at.key.as_ref() {
            b"name" => a.name = Some(v),
            b"key" => a.key = Some(v),
            b"translatable" => a.translatable_false = bilingua_model::py_strip(&v).to_lowercase() == "false",
            b"quantity" => a.quantity = Some(v),
            _ => {}
        }
    }
    Ok(a)
}

/// A slot being read: its depth (the depth its start tag was seen at), the end of its start tag, and an open
/// point (start offset, depth) whose content is copied as it is.
struct Cap {
    slot: Slot,
    depth: usize,
    stag_end: usize,
    point: Option<(usize, usize)>,
    /// index into `groups` for a plural item
    group: Option<usize>,
    item_start: usize,
}

struct Parent {
    kind: u8, // 1 plurals, 2 string-array
    name: String,
    off: bool,
    count: usize,
    group: Option<usize>,
    /// the comment before the `<plurals>` or `<string-array>` (LocOS gives it to every item's record)
    comment: String,
}

fn is_point(d: Dialect, qname: &[u8]) -> bool {
    // LocOS: the XLIFF 1.2 `g` element, or any element whose local name is `g` (Android `<xliff:g>`)
    d == Dialect::Android && local(qname) == b"g"
}

fn push_text(pieces: &mut Vec<Piece>, s: &str) {
    if let Some(Piece::Text(t)) = pieces.last_mut() {
        t.push_str(s);
    } else {
        pieces.push(Piece::Text(s.to_string()));
    }
}

pub fn scan(text: &str, d: Dialect) -> Result<Scan, Error> {
    if text.bytes().any(|b| b < 0x20 && !matches!(b, 9 | 10 | 13)) || text.contains('\u{FFFE}') || text.contains('\u{FFFF}') {
        return Err(parse_err(d, format!("{} contains a character XML does not allow", d.what())));
    }
    let mut rd = Reader::from_str(text);
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut cap: Option<Cap> = None;
    let mut parent: Option<Parent> = None;
    let mut comment = String::new();
    let mut slots = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    let bad = |e: quick_xml::Error| parse_err(d, format!("{} is not well-formed XML: {e}", d.what()));
    loop {
        let before = rd.buffer_position() as usize;
        let ev = rd.read_event().map_err(bad)?;
        let after = rd.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                let empty = text.as_bytes()[after - 2] == b'/';
                let qname = e.name();
                let qname = qname.as_ref();
                let ln = local(qname);
                if let Some(c) = cap.as_mut() {
                    attrs(d, &e)?;
                    if c.point.is_none() {
                        let raw = text[before..after].to_string();
                        if is_point(d, qname) {
                            if empty {
                                c.slot.pieces.push(Piece::Point(raw));
                            } else {
                                c.point = Some((before, depth));
                            }
                        } else {
                            if d == Dialect::JavaProps {
                                c.slot.skip = true; // an entry with child elements is left as it is
                            }
                            c.slot.pieces.push(Piece::Open(raw));
                            if empty {
                                c.slot.pieces.push(Piece::Close(String::new()));
                            }
                        }
                    }
                } else if depth == 0 {
                    if root_seen {
                        return Err(parse_err(d, format!("{} is not well-formed XML: more than one root element", d.what())));
                    }
                    if ln != d.root() {
                        return Err(parse_err(d, format!("{} has no <{}> root element", d.what(), String::from_utf8_lossy(d.root()))));
                    }
                    root_seen = true;
                    attrs(d, &e)?;
                } else {
                    let a = attrs(d, &e)?;
                    let start_slot = |name: String, kind: Kind, skip: bool, comment: String| Slot {
                        name,
                        kind,
                        skip,
                        inner: None,
                        pieces: Vec::new(),
                        cdata: false,
                        comment,
                        start: before,
                    };
                    let mut new_cap = None;
                    if depth == 1 {
                        match (d, ln) {
                            (Dialect::Android, b"string") => {
                                new_cap = Some((start_slot(a.name.clone().unwrap_or_default(), Kind::String, a.translatable_false, comment.clone()), None));
                            }
                            (Dialect::Android, b"plurals") | (Dialect::Android, b"string-array") if !empty => {
                                let plural = ln == b"plurals";
                                let group = plural.then(|| {
                                    groups.push(Group { off: a.translatable_false, items: Vec::new() });
                                    groups.len() - 1
                                });
                                parent = Some(Parent {
                                    kind: if plural { 1 } else { 2 },
                                    name: a.name.clone().unwrap_or_default(),
                                    off: a.translatable_false,
                                    count: 0,
                                    group,
                                    comment: comment.clone(),
                                });
                            }
                            (Dialect::JavaProps, b"entry") => {
                                new_cap = Some((start_slot(a.key.clone().unwrap_or_default(), Kind::Entry, false, String::new()), None));
                            }
                            _ => {}
                        }
                        comment.clear();
                    } else if depth == 2 && ln == b"item" {
                        if let Some(p) = parent.as_mut() {
                            let kind = if p.kind == 1 {
                                Kind::Plural(a.quantity.clone().unwrap_or_default())
                            } else {
                                p.count += 1;
                                Kind::Array(p.count - 1)
                            };
                            new_cap = Some((start_slot(p.name.clone(), kind, p.off, p.comment.clone()), p.group));
                        }
                    }
                    if let Some((slot, group)) = new_cap {
                        if empty {
                            if let (Some(g), Kind::Plural(q)) = (group, &slot.kind) {
                                groups[g].items.push(Item { start: before, end: after, quantity: q.clone(), inner: None });
                            }
                            slots.push(slot);
                        } else {
                            cap = Some(Cap { slot, depth, stag_end: after, point: None, group, item_start: before });
                        }
                    }
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                let mut done = false;
                if let Some(c) = cap.as_mut() {
                    if let Some((ps, pd)) = c.point {
                        if depth == pd {
                            c.slot.pieces.push(Piece::Point(text[ps..after].to_string()));
                            c.point = None;
                        }
                    } else if depth == c.depth {
                        c.slot.inner = Some((c.stag_end, before));
                        done = true;
                    } else {
                        c.slot.pieces.push(Piece::Close(text[before..after].to_string()));
                    }
                }
                if done {
                    let c = cap.take().expect("open slot");
                    if let (Some(g), Kind::Plural(q)) = (c.group, &c.slot.kind) {
                        groups[g].items.push(Item { start: c.item_start, end: after, quantity: q.clone(), inner: c.slot.inner });
                    }
                    slots.push(c.slot);
                } else if cap.is_none() && depth == 1 {
                    parent = None;
                }
            }
            Event::Text(_) => {
                let raw = &text[before..after];
                if depth == 0 {
                    if !raw.trim().is_empty() {
                        return Err(parse_err(d, format!("{} is not well-formed XML: text outside the root element", d.what())));
                    }
                } else if let Some(c) = cap.as_mut() {
                    if c.point.is_none() {
                        push_text(&mut c.slot.pieces, &norm_nl(raw));
                    }
                }
            }
            Event::CData(_) => {
                if depth == 0 {
                    return Err(parse_err(d, format!("{} is not well-formed XML: CDATA outside the root element", d.what())));
                }
                if let Some(c) = cap.as_mut() {
                    if c.point.is_none() {
                        c.slot.cdata = true;
                        push_text(&mut c.slot.pieces, &norm_nl(&text[before + 9..after - 3]));
                    }
                }
            }
            Event::GeneralRef(_) => {
                if depth == 0 {
                    return Err(parse_err(d, format!("{} is not well-formed XML: a reference outside the root element", d.what())));
                }
                let mut tmp = String::new();
                push_ref(d, &mut tmp, &text[before + 1..after - 1])?;
                if let Some(c) = cap.as_mut() {
                    if c.point.is_none() {
                        push_text(&mut c.slot.pieces, &tmp);
                    }
                }
            }
            Event::Comment(t) => {
                if let Some(c) = cap.as_mut() {
                    if d == Dialect::JavaProps {
                        c.slot.skip = true; // lxml counts a comment as a child: LocOS leaves the entry alone
                    }
                    if c.point.is_none() {
                        c.slot.pieces.push(Piece::Point(text[before..after].to_string()));
                    }
                } else if depth == 1 {
                    comment = String::from_utf8_lossy(&t).trim().to_string();
                }
            }
            Event::PI(_) => {
                if let Some(c) = cap.as_mut() {
                    if d == Dialect::JavaProps {
                        c.slot.skip = true; // as a comment
                    }
                    if c.point.is_none() {
                        c.slot.pieces.push(Piece::Point(text[before..after].to_string()));
                    }
                }
            }
            Event::DocType(t) => {
                if t.as_ref().contains(&b'[') {
                    return Err(parse_err(
                        d,
                        format!("{} declares a DTD internal subset; refused (resource files never need one, and it is an entity-expansion / XXE vector).", d.what()),
                    ));
                }
            }
            Event::Decl(_) => {}
        }
    }
    if depth != 0 {
        return Err(parse_err(d, format!("{} is not well-formed XML: the file ends inside an element", d.what())));
    }
    if !root_seen {
        return Err(parse_err(d, format!("{} has no <{}> root element", d.what(), String::from_utf8_lossy(d.root()))));
    }
    Ok(Scan { slots, groups })
}
