//! Reads a resx document with `quick-xml` and reports, for every `<data>` that has a `<value>`, its name, comment,
//! decoded text and the byte span of the value's inner XML in the (already decoded) text.

use bilingua_model::Error;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse { format: "resx".into(), msg: msg.into() }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub comment: String,
    /// Decoded text of the value (entities and CDATA resolved, line ends normalised to LF).
    pub display: String,
    /// `type` or `mimetype` present: an object, not a string.
    pub typed: bool,
    /// The value holds child elements, comments or processing instructions.
    pub complex: bool,
    /// Byte offset of the `<data` start tag.
    pub start: usize,
    /// Byte span of the value's inner XML; `None` for `<value/>`.
    pub inner: Option<(usize, usize)>,
    /// The inner XML holds a CDATA section.
    pub cdata: bool,
}

pub struct Scan {
    pub entries: Vec<Entry>,
    /// Every line end in the file is CRLF.
    pub crlf: bool,
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

/// Appends the text a general reference stands for.
fn push_ref(out: &mut String, name: &str) -> Result<(), Error> {
    match name {
        "amp" => out.push('&'),
        "lt" => out.push('<'),
        "gt" => out.push('>'),
        "quot" => out.push('"'),
        "apos" => out.push('\''),
        _ => {
            let n = name.strip_prefix('#').and_then(|d| match d.strip_prefix(['x']) {
                Some(h) => u32::from_str_radix(h, 16).ok(),
                None => d.parse::<u32>().ok(),
            });
            match n.filter(|n| xml_char(*n)).and_then(char::from_u32) {
                Some(c) => out.push(c),
                None if name.starts_with('#') => return Err(parse_err(format!("invalid character reference &{name};"))),
                None => return Err(parse_err(format!("the file uses an undefined entity (&{name};); refused"))),
            }
        }
    }
    Ok(())
}

fn push_text(out: &mut String, raw: &str) {
    if raw.contains('\r') {
        out.push_str(&raw.replace("\r\n", "\n").replace('\r', "\n"));
    } else {
        out.push_str(raw);
    }
}

/// Checks every attribute (well formed, no duplicates, references resolve) and returns `name`, `type` and `mimetype`.
fn attrs(e: &BytesStart, want: bool) -> Result<(Option<String>, bool), Error> {
    let mut name = None;
    let mut typed = false;
    for a in e.attributes() {
        let a = a.map_err(|x| parse_err(format!("bad attribute: {x}")))?;
        let v = a.unescape_value().map_err(|x| parse_err(format!("bad attribute value: {x}")))?;
        if want {
            match a.key.as_ref() {
                b"name" => name = Some(v.into_owned()),
                b"type" | b"mimetype" => typed = true,
                _ => {}
            }
        }
    }
    Ok((name, typed))
}

struct Cur {
    name: String,
    typed: bool,
    start: usize,
    value_seen: bool,
    comment_seen: bool,
    // value state
    val_open: bool,
    val_stag_end: usize,
    display: String,
    complex: bool,
    cdata: bool,
    inner: Option<(usize, usize)>,
    // comment state
    com_open: bool,
    com_text_done: bool,
    comment: String,
}

pub fn scan(text: &str) -> Result<Scan, Error> {
    // XML 1.0 characters only.
    if text.bytes().any(|b| b < 0x20 && !matches!(b, 9 | 10 | 13)) || text.contains('\u{FFFE}') || text.contains('\u{FFFF}') {
        return Err(parse_err("the file contains a character XML does not allow"));
    }
    let mut rd = Reader::from_str(text);
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut cur: Option<Cur> = None;
    let mut entries = Vec::new();
    loop {
        let before = rd.buffer_position() as usize;
        let ev = rd.read_event().map_err(|e| parse_err(format!("not well-formed XML: {e}")))?;
        let after = rd.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                let empty = text.as_bytes()[after - 2] == b'/';
                let ln = e.local_name();
                let ln = local(ln.as_ref());
                if depth == 0 {
                    if root_seen {
                        return Err(parse_err("not well-formed XML: more than one root element"));
                    }
                    if ln != b"root" {
                        return Err(parse_err("the file has no <root> element; it is not a .resx file"));
                    }
                    root_seen = true;
                    attrs(&e, false)?;
                } else if depth == 1 && ln == b"data" {
                    let (name, typed) = attrs(&e, true)?;
                    let c = Cur {
                        name: name.unwrap_or_default(),
                        typed,
                        start: before,
                        value_seen: false,
                        comment_seen: false,
                        val_open: false,
                        val_stag_end: 0,
                        display: String::new(),
                        complex: false,
                        cdata: false,
                        inner: None,
                        com_open: false,
                        com_text_done: false,
                        comment: String::new(),
                    };
                    if empty {
                        // `<data/>` has no value: not an entry
                    } else {
                        cur = Some(c);
                    }
                } else {
                    attrs(&e, false)?;
                    if let Some(c) = cur.as_mut() {
                        if depth == 2 && ln == b"value" && !c.value_seen {
                            c.value_seen = true;
                            if !empty {
                                c.val_open = true;
                                c.val_stag_end = after;
                            }
                        } else if depth == 2 && ln == b"comment" && !c.comment_seen {
                            c.comment_seen = true;
                            c.com_open = !empty;
                        } else if depth == 3 && c.val_open {
                            c.complex = true;
                        } else if depth == 3 && c.com_open {
                            c.com_text_done = true;
                        }
                    }
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                // quick-xml has checked that it matches an open element
                let level = depth;
                depth = depth.saturating_sub(1);
                if let Some(c) = cur.as_mut() {
                    if level == 3 {
                        if c.val_open {
                            c.val_open = false;
                            c.inner = Some((c.val_stag_end, before));
                        }
                        c.com_open = false;
                    }
                }
                if level == 2 {
                    if let Some(c) = cur.take() {
                        if c.value_seen {
                            let inner = c.inner.filter(|(a, b)| b > a);
                            entries.push(Entry {
                                name: c.name,
                                comment: c.comment.trim().to_string(),
                                display: c.display,
                                typed: c.typed,
                                complex: c.complex,
                                start: c.start,
                                inner,
                                cdata: c.cdata,
                            });
                        }
                    }
                }
            }
            Event::Text(_) => {
                let raw = &text[before..after];
                if depth == 0 {
                    if !raw.trim().is_empty() {
                        return Err(parse_err("not well-formed XML: text outside the root element"));
                    }
                } else if let Some(c) = cur.as_mut() {
                    if depth == 3 && c.val_open {
                        push_text(&mut c.display, raw);
                    } else if depth == 3 && c.com_open && !c.com_text_done {
                        push_text(&mut c.comment, raw);
                    }
                }
            }
            Event::CData(_) => {
                if depth == 0 {
                    return Err(parse_err("not well-formed XML: CDATA outside the root element"));
                }
                if let Some(c) = cur.as_mut() {
                    let raw = &text[before + 9..after - 3];
                    if depth == 3 && c.val_open {
                        c.cdata = true;
                        push_text(&mut c.display, raw);
                    } else if depth == 3 && c.com_open && !c.com_text_done {
                        push_text(&mut c.comment, raw);
                    }
                }
            }
            Event::GeneralRef(_) => {
                if depth == 0 {
                    return Err(parse_err("not well-formed XML: reference outside the root element"));
                }
                let name = &text[before + 1..after - 1];
                let mut tmp = String::new();
                push_ref(&mut tmp, name)?;
                if let Some(c) = cur.as_mut() {
                    if depth == 3 && c.val_open {
                        c.display.push_str(&tmp);
                    } else if depth == 3 && c.com_open && !c.com_text_done {
                        c.comment.push_str(&tmp);
                    }
                }
            }
            Event::Comment(_) | Event::PI(_) => {
                if let Some(c) = cur.as_mut() {
                    if depth == 3 && c.val_open {
                        c.complex = true;
                    } else if depth == 3 && c.com_open {
                        c.com_text_done = true;
                    }
                }
            }
            Event::DocType(d) => {
                if d.as_ref().contains(&b'[') {
                    return Err(parse_err("the file declares a DTD internal subset; refused"));
                }
            }
            Event::Decl(_) => {}
        }
    }
    if depth != 0 {
        return Err(parse_err("not well-formed XML: the file ends inside an element"));
    }
    if !root_seen {
        return Err(parse_err("the file has no <root> element; it is not a .resx file"));
    }
    let b = text.as_bytes();
    let (mut crlf_n, mut lf_bare, mut i) = (0usize, 0usize, 0usize);
    while let Some(p) = b.get(i..).and_then(|r| r.iter().position(|c| *c == b'\n')) {
        let at = i + p;
        if at > 0 && b[at - 1] == b'\r' {
            crlf_n += 1;
        } else {
            lf_bare += 1;
        }
        i = at + 1;
    }
    Ok(Scan { entries, crlf: crlf_n > 0 && lf_bare == 0 })
}
