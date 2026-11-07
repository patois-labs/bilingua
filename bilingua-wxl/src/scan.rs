//! Reading the XML of a `.wxl` file: where each `<String>` and `<UI>` entry sits in the text.
//!
//! `quick-xml` finds the structure and checks that the file is well formed; this module adds what libxml2 checks
//! on top (no undefined entity, no characters XML 1.0 forbids, no `<` inside an attribute value, no repeated
//! attribute, one root element) and records byte offsets, so the writer can change one attribute value or one
//! element's content and leave every other byte alone.

use quick_xml::events::Event;
use quick_xml::Reader;

/// What kind of entry a child of the root is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `<String Id=".." Value=".."/>` or `<String Id="..">text</String>`.
    String,
    /// `<UI Dialog=".." Control=".." Text=".."/>`.
    Ui,
}

/// Where the text of an entry is.
#[derive(Debug, Clone)]
pub enum Place {
    /// Inside the quotes of an attribute: byte range of the raw value, and the quote character.
    Attr { start: usize, end: usize, quote: u8 },
    /// The content of the element: byte range between its start and end tags, and whether it holds CDATA.
    Content { start: usize, end: usize, cdata: bool },
    /// No place a translation can go: an empty `<String/>`, a `<UI>` without `Text`.
    None,
}

/// One `<String>` or `<UI>` child of the root.
#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: Kind,
    pub id: Option<String>,
    pub dialog: Option<String>,
    pub control: Option<String>,
    pub localizable: Option<String>,
    pub overridable: Option<String>,
    pub place: Place,
    /// The text as an XML parser reads it (references resolved, line ends normalised). Empty for `Place::None`
    /// and for an element that has child nodes.
    pub text: String,
    /// The element has a child element (LocOS skips it), or comments / processing instructions around more than one
    /// run of text.
    pub has_children: bool,
    /// Byte ranges of the comments and processing instructions directly inside the element.
    pub notes: Vec<(usize, usize)>,
}

pub struct Scanned {
    pub entries: Vec<Entry>,
}

pub fn bad(msg: impl Into<String>) -> String {
    msg.into()
}

fn local(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|c| *c == b':') {
        Some(p) => &name[p + 1..],
        None => name,
    }
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

/// One attribute of a start tag: name, byte range of the raw value, quote.
#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub quote: u8,
}

/// Parses the attributes of the start tag at `s[from..to]` (`<name a="b" c='d'/>`). Returns the offset just
/// after the element name and the attributes. The tag already passed quick-xml, so errors here are the extra
/// checks libxml2 makes.
pub fn parse_attrs(s: &[u8], from: usize, to: usize) -> Result<(usize, Vec<Attr>), String> {
    let mut i = from + 1;
    while i < to && !is_ws(s[i]) && s[i] != b'/' && s[i] != b'>' {
        i += 1;
    }
    let name_end = i;
    let mut out: Vec<Attr> = Vec::new();
    loop {
        while i < to && is_ws(s[i]) {
            i += 1;
        }
        if i >= to || s[i] == b'>' || s[i] == b'/' {
            break;
        }
        let ns = i;
        while i < to && !is_ws(s[i]) && s[i] != b'=' && s[i] != b'>' && s[i] != b'/' {
            i += 1;
        }
        let name = std::str::from_utf8(&s[ns..i]).map_err(|_| bad("attribute name is not valid UTF-8"))?.to_string();
        while i < to && is_ws(s[i]) {
            i += 1;
        }
        if i >= to || s[i] != b'=' {
            return Err(bad(format!("attribute {name} has no value")));
        }
        i += 1;
        while i < to && is_ws(s[i]) {
            i += 1;
        }
        if i >= to || (s[i] != b'"' && s[i] != b'\'') {
            return Err(bad(format!("attribute {name} is not quoted")));
        }
        let quote = s[i];
        let vs = i + 1;
        let mut j = vs;
        while j < to && s[j] != quote {
            if s[j] == b'<' {
                return Err(bad(format!("'<' is not allowed in the value of attribute {name}")));
            }
            j += 1;
        }
        if j >= to {
            return Err(bad(format!("the value of attribute {name} is not closed")));
        }
        if out.iter().any(|a| a.name == name) {
            return Err(bad(format!("attribute {name} is repeated")));
        }
        check_refs(&s[vs..j])?;
        out.push(Attr { name, start: vs, end: j, quote });
        i = j + 1;
    }
    Ok((name_end, out))
}

fn char_ok(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// Resolves the name between `&` and `;`: a predefined entity or a character reference.
pub fn resolve_ref(name: &str) -> Result<char, String> {
    match name {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "quot" => Ok('"'),
        "apos" => Ok('\''),
        _ => {
            let digits = name.strip_prefix('#').ok_or_else(|| bad(format!("undefined entity &{name};")))?;
            let n = match digits.strip_prefix('x') {
                Some(h) if !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()) => u32::from_str_radix(h, 16).ok(),
                None if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => digits.parse::<u32>().ok(),
                _ => None,
            };
            n.filter(|n| char_ok(*n)).and_then(char::from_u32).ok_or_else(|| bad(format!("invalid character reference &{name};")))
        }
    }
}

/// Checks every `&..;` in raw text.
fn check_refs(raw: &[u8]) -> Result<(), String> {
    let mut i = 0;
    while let Some(p) = raw[i..].iter().position(|c| *c == b'&') {
        let s = i + p + 1;
        let e = raw[s..].iter().position(|c| *c == b';').ok_or_else(|| bad("a '&' without its ';'"))? + s;
        let name = std::str::from_utf8(&raw[s..e]).map_err(|_| bad("entity name is not valid UTF-8"))?;
        resolve_ref(name)?;
        i = e + 1;
    }
    Ok(())
}

/// The value of an attribute as a parser reads it: references resolved, a literal tab or line end becomes a space.
pub fn unescape_attr(raw: &str) -> Result<String, String> {
    unescape(raw, true)
}

/// Element content as a parser reads it: CDATA sections inline, references resolved, `\r\n` and `\r` become `\n`.
pub fn unescape_content(raw: &str) -> Result<String, String> {
    unescape(raw, false)
}

fn unescape(raw: &str, attr: bool) -> Result<String, String> {
    let b = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let mut start = 0;
    let flush = |out: &mut String, from: usize, to: usize| out.push_str(&raw[from..to]);
    while i < b.len() {
        match b[i] {
            b'&' => {
                flush(&mut out, start, i);
                let e = raw[i + 1..].find(';').ok_or_else(|| bad("a '&' without its ';'"))? + i + 1;
                out.push(resolve_ref(&raw[i + 1..e])?);
                i = e + 1;
                start = i;
            }
            b'\r' => {
                flush(&mut out, start, i);
                out.push(if attr { ' ' } else { '\n' });
                i += if b.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                start = i;
            }
            b'\n' | b'\t' if attr => {
                flush(&mut out, start, i);
                out.push(' ');
                i += 1;
                start = i;
            }
            b'<' if !attr && raw[i..].starts_with("<![CDATA[") => {
                flush(&mut out, start, i);
                let e = raw[i + 9..].find("]]>").ok_or_else(|| bad("a CDATA section is not closed"))? + i + 9;
                let body = &raw[i + 9..e];
                // line ends inside CDATA are normalised too
                out.push_str(&body.replace("\r\n", "\n").replace('\r', "\n"));
                i = e + 3;
                start = i;
            }
            _ => i += 1,
        }
    }
    flush(&mut out, start, b.len());
    Ok(out)
}

/// Characters XML 1.0 forbids anywhere in a document: C0 controls other than tab, line feed and carriage return,
/// and U+FFFE and U+FFFF.
fn check_chars(b: &[u8]) -> Result<(), String> {
    if let Some(p) = b.iter().position(|c| *c < 0x20 && !matches!(c, 0x9 | 0xA | 0xD)) {
        return Err(bad(format!("character 0x{:02X} at byte {p} is not allowed in XML", b[p])));
    }
    if let Some(p) = b.windows(3).position(|w| w == [0xEF, 0xBF, 0xBE] || w == [0xEF, 0xBF, 0xBF]) {
        return Err(bad(format!("character U+FFFE or U+FFFF at byte {p} is not allowed in XML")));
    }
    Ok(())
}

fn find_in(hay: &str, from: usize, needle: &str) -> Option<usize> {
    hay.get(from..)?.find(needle).map(|p| p + from)
}

pub fn scan(text: &str) -> Result<Scanned, String> {
    let b = text.as_bytes();
    check_chars(b)?;
    let mut rd = Reader::from_reader(b);
    let mut depth = 0usize;
    let mut root_done = false;
    let mut root_tag = (0usize, 0usize);
    let mut entries: Vec<Entry> = Vec::new();
    let mut cur: Option<(Entry, usize)> = None; // the open entry and the end of its start tag
    let err = |at: usize, e: &dyn std::fmt::Display| bad(format!("{e} (near byte {at})"));
    loop {
        let start = rd.buffer_position() as usize;
        let ev = rd.read_event().map_err(|e| err(start, &e))?;
        let end = rd.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Start(_) | Event::Empty(_) => {
                let empty = matches!(ev, Event::Empty(_));
                let (name_end, attrs) = parse_attrs(b, start, end).map_err(|e| err(start, &e))?;
                if depth == 0 {
                    if root_done {
                        return Err(err(start, &"more than one root element"));
                    }
                    root_tag = (start, end);
                    if empty {
                        root_done = true;
                    } else {
                        depth = 1;
                    }
                    continue;
                }
                if depth == 1 {
                    let name = local(&b[start + 1..name_end]);
                    let kind = match name {
                        b"String" => Some(Kind::String),
                        b"UI" => Some(Kind::Ui),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        let get = |n: &str| -> Result<Option<String>, String> {
                            attrs.iter().find(|a| a.name == n).map(|a| unescape_attr(&text[a.start..a.end])).transpose()
                        };
                        let attr_name = if kind == Kind::String { "Value" } else { "Text" };
                        let mut e = Entry {
                            kind,
                            id: get("Id").map_err(|e| err(start, &e))?,
                            dialog: get("Dialog").map_err(|e| err(start, &e))?,
                            control: get("Control").map_err(|e| err(start, &e))?,
                            localizable: get("Localizable").map_err(|e| err(start, &e))?,
                            overridable: get("Overridable").map_err(|e| err(start, &e))?,
                            place: Place::None,
                            text: String::new(),
                            has_children: false,
                            notes: Vec::new(),
                        };
                        if let Some(a) = attrs.iter().find(|a| a.name == attr_name) {
                            e.text = unescape_attr(&text[a.start..a.end]).map_err(|m| err(start, &m))?;
                            e.place = Place::Attr { start: a.start, end: a.end, quote: a.quote };
                        }
                        if empty {
                            entries.push(e);
                        } else {
                            cur = Some((e, end));
                            depth = 2;
                        }
                        continue;
                    }
                } else if depth == 2 {
                    if let Some((e, _)) = cur.as_mut() {
                        e.has_children = true;
                    }
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                if depth == 0 {
                    return Err(err(start, &"an end tag with no start tag"));
                }
                depth -= 1;
                if depth == 0 {
                    root_done = true;
                } else if depth == 1 {
                    if let Some((mut e, content_start)) = cur.take() {
                        if !matches!(e.place, Place::Attr { .. }) {
                            let (mut from, mut to) = (content_start, start);
                            if !e.has_children && !e.notes.is_empty() {
                                // Comments (WiX loc tools write `<!-- _locID_text=.. -->` first) do not make the
                                // string a mixed one: when exactly one run of text is left between them, that run
                                // is the value and the comments stay where they are.
                                let mut runs: Vec<(usize, usize)> = Vec::new();
                                let mut at = content_start;
                                for &(ns, ne) in e.notes.iter().chain(std::iter::once(&(start, start))) {
                                    if text[at..ns].bytes().any(|c| !is_ws(c)) {
                                        runs.push((at, ns));
                                    }
                                    at = ne;
                                }
                                match runs.as_slice() {
                                    [] => {}
                                    [(a, z)] => (from, to) = (*a, *z),
                                    _ => e.has_children = true,
                                }
                            }
                            let raw = &text[from..to];
                            if !e.has_children {
                                e.text = unescape_content(raw).map_err(|m| err(from, &m))?;
                            }
                            e.place = Place::Content { start: from, end: to, cdata: raw.contains("<![CDATA[") };
                        } else if !e.has_children {
                            // the Value attribute wins; the content is not read, but must still be well formed
                            // (the parser has checked that)
                        }
                        entries.push(e);
                    }
                }
            }
            Event::Text(t) => {
                if depth == 0 && !t.iter().all(|c| is_ws(*c)) {
                    return Err(err(start, &"text outside the root element"));
                }
                if t.windows(3).any(|w| w == b"]]>") {
                    return Err(err(start, &"']]>' is not allowed in text"));
                }
            }
            Event::GeneralRef(r) => {
                if depth == 0 {
                    return Err(err(start, &"a reference outside the root element"));
                }
                let name = r.decode().map_err(|e| err(start, &e))?;
                resolve_ref(&name).map_err(|m| err(start, &m))?;
            }
            Event::CData(_) => {
                if depth == 0 {
                    return Err(err(start, &"CDATA outside the root element"));
                }
            }
            Event::Comment(_) | Event::PI(_) => {
                if let Event::PI(p) = &ev {
                    if p.target().eq_ignore_ascii_case(b"xml") {
                        return Err(err(start, &"a processing instruction may not be named xml"));
                    }
                }
                if depth == 2 {
                    if let Some((e, _)) = cur.as_mut() {
                        e.notes.push((start, end));
                    }
                }
            }
            Event::Decl(_) | Event::DocType(_) => {
                if matches!(ev, Event::Decl(_)) && start != 0 {
                    return Err(err(start, &"an XML declaration is only allowed at the start of the file"));
                }
                if depth != 0 || root_done {
                    return Err(err(start, &"a declaration in the wrong place"));
                }
            }
        }
    }
    if depth != 0 {
        return Err(bad("the file ends before the root element is closed"));
    }
    if root_tag == (0, 0) {
        return Err(bad("the file has no root element"));
    }
    Ok(Scanned { entries })
}

/// The name (without prefix) of the root element of `head`, for `sniff`: skips the declaration, comments,
/// processing instructions and the DOCTYPE.
pub fn root_name(head: &str) -> Option<String> {
    let mut i = 0;
    loop {
        let p = find_in(head, i, "<")?;
        let rest = &head[p..];
        if rest.starts_with("<!--") {
            i = find_in(head, p + 4, "-->")? + 3;
        } else if rest.starts_with("<?") {
            i = find_in(head, p + 2, "?>")? + 2;
        } else if rest.starts_with("<!") {
            i = find_in(head, p + 2, ">")? + 1;
        } else {
            let name: String = rest[1..].chars().take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/').collect();
            return Some(name.rsplit(':').next().unwrap_or("").to_string());
        }
    }
}
