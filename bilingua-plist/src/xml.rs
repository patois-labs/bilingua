//! A strict scanner for XML property lists. It builds the [`Tree`] with the byte span of every string value so a
//! translation can be spliced into the original text. No DTD, no custom entities, no unknown elements: a file the
//! scanner is unsure about is refused, never guessed at.

use crate::tree::*;
use bilingua_model::*;
use std::borrow::Cow;
use std::collections::HashSet;

/// Deepest element nesting (Localization OS `_MAX_DEPTH`).
pub(crate) const MAX_DEPTH: usize = 128;

pub(crate) fn perr(fmt: &str, msg: impl Into<String>) -> Error {
    Error::Parse { format: fmt.into(), msg: msg.into() }
}

pub(crate) fn too_deep() -> Error {
    LimitError::new(LimitKind::XmlDepth, "This property list is nested too deeply. It was not processed.").into()
}

pub(crate) fn too_many(max_units: usize) -> Error {
    LimitError::new(
        LimitKind::Units,
        format!("This property list has far more entries than the limit of {max_units} segments allows. Split it into smaller files."),
    )
    .into()
}

/// Python `str.isspace` (what LocOS `strip()` removes).
pub(crate) fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn is_xml_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

fn find(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > b.len() {
        return None;
    }
    b[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

/// XML end-of-line handling (XML 1.0 section 2.11), as expat applies it.
pub(crate) fn eol(s: &str) -> Cow<'_, str> {
    if s.contains('\r') {
        Cow::Owned(s.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(s)
    }
}

fn xml_char(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// Resolves `&lt; &gt; &amp; &quot; &apos;` and character references; any other entity is refused.
pub(crate) fn unescape<'a>(s: &'a str, fmt: &str) -> Result<Cow<'a, str>, Error> {
    if !s.contains('&') {
        return Ok(eol(s));
    }
    let s = eol(s);
    let mut out = String::with_capacity(s.len());
    let mut rest: &str = &s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        let semi = after.bytes().take(12).position(|c| c == b';');
        let Some(semi) = semi else { return Err(perr(fmt, "property list uses an entity other than &lt; &gt; &amp; &quot; &apos; (entity declarations are refused)")) };
        let name = &after[..semi];
        match name {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let code = if let Some(h) = name.strip_prefix("#x") {
                    (!h.is_empty() && h.len() <= 6 && h.bytes().all(|c| c.is_ascii_hexdigit())).then(|| u32::from_str_radix(h, 16).ok()).flatten()
                } else if let Some(d) = name.strip_prefix('#') {
                    (!d.is_empty() && d.len() <= 7 && d.bytes().all(|c| c.is_ascii_digit())).then(|| d.parse::<u32>().ok()).flatten()
                } else {
                    None
                };
                match code {
                    Some(c) if xml_char(c) => out.push(char::from_u32(c).unwrap_or('\u{fffd}')),
                    Some(_) => return Err(perr(fmt, "property list has an invalid character reference")),
                    None => return Err(perr(fmt, "property list uses an entity other than &lt; &gt; &amp; &quot; &apos; (entity declarations are refused)")),
                }
            }
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    Ok(Cow::Owned(out))
}

/// Escapes a string value for element content.
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

/// Keeps XML 1.0 characters, turning pasted Word breaks into newlines (LocOS `xml_target_text`).
pub(crate) fn target_text(s: &str) -> String {
    s.chars().map(|c| if matches!(c, '\u{0B}' | '\u{0C}') { '\n' } else { c }).filter(|c| !matches!(*c as u32, 0..=8 | 0x0E..=0x1F | 0xFFFE | 0xFFFF)).collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum El {
    Plist,
    Dict,
    Array,
    Str,
    Key,
    Int,
    Real,
    Date,
    Data,
    True,
    False,
}

impl El {
    fn from_name(n: &str) -> Option<El> {
        Some(match n {
            "plist" => El::Plist,
            "dict" => El::Dict,
            "array" => El::Array,
            "string" => El::Str,
            "key" => El::Key,
            "integer" => El::Int,
            "real" => El::Real,
            "date" => El::Date,
            "data" => El::Data,
            "true" => El::True,
            "false" => El::False,
            _ => return None,
        })
    }
    fn name(self) -> &'static str {
        match self {
            El::Plist => "plist",
            El::Dict => "dict",
            El::Array => "array",
            El::Str => "string",
            El::Key => "key",
            El::Int => "integer",
            El::Real => "real",
            El::Date => "date",
            El::Data => "data",
            El::True => "true",
            El::False => "false",
        }
    }
    fn is_text(self) -> bool {
        matches!(self, El::Str | El::Key)
    }
    fn is_scalar(self) -> bool {
        matches!(self, El::Int | El::Real | El::Date | El::Data | El::True | El::False)
    }
}

struct Frame {
    el: El,
    node: usize,
    cstart: usize,
    start: usize,
    pending: Option<(String, usize, usize)>,
    seen: HashSet<String>,
}

fn name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b':' | b'-') || c >= 0x80
}

/// After an element name: attributes up to `>` or `/>`. Returns the offset after the tag and whether it is empty.
fn attributes(text: &str, mut i: usize, fmt: &str) -> Result<(usize, bool), Error> {
    let b = text.as_bytes();
    let n = b.len();
    let mut seen: Vec<&str> = Vec::new();
    loop {
        let ws = i;
        while i < n && is_xml_ws(b[i]) {
            i += 1;
        }
        let had_ws = i > ws;
        if i >= n {
            return Err(perr(fmt, "not a well-formed property list (a tag is not closed)"));
        }
        match b[i] {
            b'>' => return Ok((i + 1, false)),
            b'/' => {
                return if b.get(i + 1) == Some(&b'>') { Ok((i + 2, true)) } else { Err(perr(fmt, format!("not a well-formed property list at offset {i}"))) };
            }
            _ => {}
        }
        if !had_ws {
            return Err(perr(fmt, format!("not a well-formed property list at offset {i}")));
        }
        let ns = i;
        while i < n && name_byte(b[i]) {
            i += 1;
        }
        if i == ns || b[ns].is_ascii_digit() || matches!(b[ns], b'-' | b'.') {
            return Err(perr(fmt, format!("not a well-formed property list at offset {ns}")));
        }
        let name = &text[ns..i];
        while i < n && is_xml_ws(b[i]) {
            i += 1;
        }
        if b.get(i) != Some(&b'=') {
            return Err(perr(fmt, format!("not a well-formed property list at offset {i}")));
        }
        i += 1;
        while i < n && is_xml_ws(b[i]) {
            i += 1;
        }
        let q = match b.get(i) {
            Some(&q @ (b'"' | b'\'')) => q,
            _ => return Err(perr(fmt, format!("not a well-formed property list at offset {i}"))),
        };
        i += 1;
        let Some(len) = b[i..].iter().position(|&c| c == q) else { return Err(perr(fmt, "not a well-formed property list (an attribute is not closed)")) };
        let value = &text[i..i + len];
        if value.contains('<') {
            return Err(perr(fmt, format!("not a well-formed property list at offset {i}")));
        }
        unescape(value, fmt)?;
        if seen.contains(&name) {
            return Err(perr(fmt, format!("not a well-formed property list (attribute {name} repeated)")));
        }
        seen.push(name);
        i += len + 1;
    }
}

fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400) {
                29
            } else {
                28
            }
        }
    }
}

/// `YYYY[-MM[-DD[THH[:MM[:SS]]]]]Z` as plistlib reads a date.
pub(crate) fn valid_date(s: &str) -> bool {
    let s = s.trim();
    let Some(body) = s.strip_suffix('Z') else { return false };
    let b = body.as_bytes();
    let num = |from: usize, to: usize| -> Option<u32> {
        let p = b.get(from..to)?;
        p.iter().all(u8::is_ascii_digit).then(|| p.iter().fold(0u32, |a, d| a * 10 + (d - b'0') as u32))
    };
    let Some(y) = num(0, 4) else { return false };
    if y < 1 {
        return false;
    }
    let mut at = 4;
    let mut parts = [1u32, 1, 0, 0, 0];
    let seps = [b'-', b'-', b'T', b':', b':'];
    for (k, sep) in seps.iter().enumerate() {
        if at == b.len() {
            break;
        }
        if b[at] != *sep {
            return false;
        }
        let Some(v) = num(at + 1, at + 3) else { return false };
        parts[k] = v;
        at += 3;
    }
    if at != b.len() {
        return false;
    }
    let [m, d, h, mi, se] = parts;
    (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m) && h < 24 && mi < 60 && se < 60
}

fn valid_int(s: &str) -> bool {
    let t = s.trim();
    let t = t.strip_prefix(['+', '-']).unwrap_or(t);
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit());
    }
    !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit())
}

fn validate_scalar(el: El, content: &str, fmt: &str) -> Result<(), Error> {
    let ok = match el {
        El::Int => valid_int(content),
        El::Real => {
            let t = content.trim();
            !t.is_empty() && t.parse::<f64>().is_ok()
        }
        El::Date => valid_date(content),
        El::Data => {
            let t: String = content.chars().filter(|c| !is_space(*c)).collect();
            b64::decode(&t).is_ok()
        }
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(perr(fmt, format!("<{}> holds a value that is not a valid property list {}", el.name(), el.name())))
    }
}

fn dtd_refused(fmt: &str) -> Error {
    perr(fmt, "property list declares an internal DTD or entities; refused")
}

/// Scans an XML property list (already decoded to text).
pub(crate) fn scan(text: &str, fmt: &'static str, lim: &Limits) -> Result<Tree, Error> {
    let b = text.as_bytes();
    let n = b.len();
    // XML 1.0 characters only
    for (i, &c) in b.iter().enumerate() {
        if (c < 0x20 && !matches!(c, 9 | 10 | 13)) || (c == 0xEF && b.get(i + 1) == Some(&0xBF) && b.get(i + 2).is_some_and(|x| *x >= 0xBE)) {
            return Err(perr(fmt, format!("property list holds a character XML does not allow at offset {i}")));
        }
    }
    let max_nodes = lim.max_units.saturating_mul(40);
    let mut tree = Tree { nodes: Vec::new(), root: NO_SPAN };
    let mut stack: Vec<Frame> = Vec::new();
    let mut content = String::new();
    let mut pos = 0usize;
    let mut decl_seen = false;
    let mut doctype_seen = false;
    while pos < n {
        let lt = b[pos..].iter().position(|&c| c == b'<').map(|p| p + pos);
        let chunk_end = lt.unwrap_or(n);
        if chunk_end > pos {
            let chunk = &text[pos..chunk_end];
            match stack.last().map(|f| f.el) {
                Some(el) if el.is_text() || el.is_scalar() => {
                    if chunk.contains("]]>") {
                        return Err(perr(fmt, format!("not a well-formed property list at offset {pos}")));
                    }
                    content.push_str(&unescape(chunk, fmt)?);
                }
                _ => {
                    if !chunk.chars().all(is_space) {
                        return Err(perr(fmt, format!("unexpected text at offset {pos}")));
                    }
                }
            }
        }
        let Some(lt) = lt else { break };
        let rest = &b[lt..];
        if rest.starts_with(b"<!--") {
            let Some(e) = find(b, lt + 4, b"-->") else { return Err(perr(fmt, "not a well-formed property list (a comment is not closed)")) };
            let body = &text[lt + 4..e];
            if body.contains("--") || body.ends_with('-') {
                return Err(perr(fmt, format!("not a well-formed property list at offset {lt}")));
            }
            pos = e + 3;
        } else if rest.starts_with(b"<![CDATA[") {
            let Some(e) = find(b, lt + 9, b"]]>") else { return Err(perr(fmt, "not a well-formed property list (a CDATA section is not closed)")) };
            let body = &text[lt + 9..e];
            match stack.last().map(|f| f.el) {
                Some(el) if el.is_text() => content.push_str(&eol(body)),
                Some(el) if el.is_scalar() => content.push_str(body),
                Some(_) if body.chars().all(is_space) => {}
                _ => return Err(perr(fmt, format!("unexpected CDATA at offset {lt}"))),
            }
            pos = e + 3;
        } else if rest.starts_with(b"<?") {
            let Some(e) = find(b, lt + 2, b"?>") else { return Err(perr(fmt, "not a well-formed property list (a processing instruction is not closed)")) };
            let body = &text[lt + 2..e];
            let tl = body.bytes().take_while(|&c| name_byte(c)).count();
            let target = &body[..tl];
            if target.is_empty() || b[lt + 2].is_ascii_digit() || !(tl == body.len() || is_xml_ws(body.as_bytes()[tl])) {
                return Err(perr(fmt, format!("not a well-formed property list at offset {lt}")));
            }
            if target.eq_ignore_ascii_case("xml") {
                let first = text[..lt].chars().all(is_space);
                if target != "xml" || !first || decl_seen || !body.contains("version") {
                    return Err(perr(fmt, format!("not a well-formed property list at offset {lt}")));
                }
                decl_seen = true;
            }
            pos = e + 2;
        } else if rest.starts_with(b"<!DOCTYPE") {
            if !stack.is_empty() || tree.root != NO_SPAN || doctype_seen {
                return Err(perr(fmt, format!("not a well-formed property list at offset {lt}")));
            }
            doctype_seen = true;
            let mut i = lt + 9;
            let ws = i;
            while i < n && is_xml_ws(b[i]) {
                i += 1;
            }
            if i == ws || !b[i..].starts_with(b"plist") || b.get(i + 5).is_some_and(|c| name_byte(*c)) {
                return Err(perr(fmt, "property list has a DOCTYPE other than plist"));
            }
            i += 5;
            loop {
                match b.get(i) {
                    None => return Err(perr(fmt, "not a well-formed property list (the DOCTYPE is not closed)")),
                    Some(b'[') | Some(b']') => return Err(dtd_refused(fmt)),
                    Some(b'<') => return Err(perr(fmt, format!("not a well-formed property list at offset {i}"))),
                    Some(b'>') => break,
                    Some(_) => i += 1,
                }
            }
            pos = i + 1;
        } else if rest.starts_with(b"<!") {
            return Err(perr(fmt, format!("not a well-formed property list at offset {lt}")));
        } else if rest.starts_with(b"</") {
            let mut i = lt + 2;
            let ns = i;
            while i < n && name_byte(b[i]) {
                i += 1;
            }
            let name = &text[ns..i];
            while i < n && is_xml_ws(b[i]) {
                i += 1;
            }
            if b.get(i) != Some(&b'>') {
                return Err(perr(fmt, format!("not a well-formed property list at offset {lt}")));
            }
            let end = i + 1;
            let Some(frame) = stack.pop() else { return Err(perr(fmt, format!("mismatched </{name}> at offset {lt}"))) };
            if frame.el.name() != name {
                return Err(perr(fmt, format!("mismatched </{name}> at offset {lt}")));
            }
            close(&mut tree, &mut stack, frame, &mut content, Ended { content_end: lt, end, empty: false }, fmt)?;
            pos = end;
        } else {
            // a start tag
            let mut i = lt + 1;
            if !(b.get(i).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')) {
                return Err(perr(fmt, format!("not a well-formed property list at offset {lt}")));
            }
            while i < n && name_byte(b[i]) {
                i += 1;
            }
            let name = &text[lt + 1..i];
            let (end, empty) = attributes(text, i, fmt)?;
            if let Some(top) = stack.last() {
                if top.el.is_text() || top.el.is_scalar() {
                    return Err(perr(fmt, format!("<{}> cannot contain elements", top.el.name())));
                }
            }
            let Some(el) = El::from_name(name) else { return Err(perr(fmt, format!("unknown property list element <{name}>"))) };
            if stack.len() > MAX_DEPTH {
                return Err(too_deep());
            }
            if el == El::Plist {
                if !stack.is_empty() || tree.root != NO_SPAN {
                    return Err(perr(fmt, "nested <plist>"));
                }
                if !empty {
                    stack.push(Frame { el, node: NO_SPAN, cstart: end, start: lt, pending: None, seen: HashSet::new() });
                }
                pos = end;
                continue;
            }
            let Some(top) = stack.last_mut() else { return Err(perr(fmt, "property list has content outside <plist>")) };
            if el == El::Key {
                if top.el != El::Dict {
                    return Err(perr(fmt, "unexpected <key> outside a <dict>"));
                }
                if top.pending.is_some() {
                    return Err(perr(fmt, "<dict> has a <key> with no value"));
                }
                if empty {
                    top.pending = Some((String::new(), lt, end));
                } else {
                    content.clear();
                    stack.push(Frame { el, node: NO_SPAN, cstart: end, start: lt, pending: None, seen: HashSet::new() });
                }
                pos = end;
                continue;
            }
            if tree.nodes.len() >= max_nodes {
                return Err(too_many(lim.max_units));
            }
            let kind = match el {
                El::Dict => Kind::Dict,
                El::Array => Kind::Array,
                El::Str => Kind::Str,
                _ => Kind::Other,
            };
            let mut node = Node::new(kind);
            node.start = lt;
            let idx = tree.nodes.len();
            // attach to the parent
            match top.el {
                El::Plist => {
                    if tree.root != NO_SPAN {
                        return Err(perr(fmt, "<plist> holds more than one value"));
                    }
                    tree.root = idx;
                }
                El::Array => {
                    let parent = top.node;
                    tree.nodes[parent].kids.push(Kid { key: None, kstart: NO_SPAN, kend: NO_SPAN, node: idx });
                }
                El::Dict => {
                    let Some((key, ks, ke)) = top.pending.take() else { return Err(perr(fmt, "<dict> has a value with no <key>")) };
                    if !top.seen.insert(key.clone()) {
                        return Err(perr(fmt, format!("<dict> repeats the key {key:?}; the file could not be read unambiguously")));
                    }
                    let parent = top.node;
                    tree.nodes[parent].kids.push(Kid { key: Some(key), kstart: ks, kend: ke, node: idx });
                }
                _ => {}
            }
            tree.nodes.push(node);
            let frame = Frame { el, node: idx, cstart: end, start: lt, pending: None, seen: HashSet::new() };
            if empty {
                close(&mut tree, &mut stack, frame, &mut content, Ended { content_end: lt, end, empty: true }, fmt)?;
            } else {
                if el.is_text() || el.is_scalar() {
                    content.clear();
                }
                stack.push(frame);
            }
            pos = end;
        }
    }
    if !stack.is_empty() {
        return Err(perr(fmt, "property list ends inside an element"));
    }
    if tree.root == NO_SPAN {
        return Err(perr(fmt, "property list has no value"));
    }
    Ok(tree)
}

/// An element has ended at `end`; `content_end` is where its content stopped (the `<` of the end tag, or the
/// start of an empty tag).
/// Where an element ended: the content stops at `content_end`, the element at `end`; `empty` for a `<x/>` tag.
#[derive(Clone, Copy)]
struct Ended {
    content_end: usize,
    end: usize,
    empty: bool,
}

fn close(tree: &mut Tree, stack: &mut [Frame], frame: Frame, content: &mut String, ended: Ended, fmt: &str) -> Result<(), Error> {
    let Ended { content_end, end, empty } = ended;
    match frame.el {
        El::Plist => {}
        El::Dict => {
            if frame.pending.is_some() {
                return Err(perr(fmt, "<dict> ends with a <key> that has no value"));
            }
            tree.nodes[frame.node].end = end;
        }
        El::Array => tree.nodes[frame.node].end = end,
        El::Str => {
            let node = &mut tree.nodes[frame.node];
            node.end = end;
            if !empty {
                // a non-empty element: the content sits between the tags
                node.text = std::mem::take(content);
                node.cstart = frame.cstart;
                node.cend = content_end;
            }
        }
        El::Key => {
            let key = std::mem::take(content);
            match stack.last_mut() {
                Some(p) => p.pending = Some((key, frame.start, end)),
                None => return Err(perr(fmt, "unexpected <key>")),
            }
        }
        el => {
            validate_scalar(el, content, fmt)?;
            content.clear();
            tree.nodes[frame.node].end = end;
        }
    }
    Ok(())
}
