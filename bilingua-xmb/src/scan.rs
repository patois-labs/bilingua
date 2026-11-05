//! A small, strict XML reader for message bundles: checks the text is well formed (no custom entities, balanced,
//! valid characters, unique attributes, one root) and records the exact character span of every element, so the
//! writer can splice only what changed. Nothing is resolved: a DOCTYPE is skipped, never read.
//!
//! Iterative (no recursion), so nesting depth cannot overflow the stack; the engine's XML guard has already
//! bounded depth and element count before this runs.

use bilingua_model::Error;

pub fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "xmb".into(), msg: msg.into() }
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    /// Decoded value (entities resolved, tabs and newlines as spaces).
    pub value: String,
    /// Span of the raw value, inside the quotes.
    pub vstart: usize,
    pub vend: usize,
    pub quote: u8,
}

#[derive(Debug, Clone, Copy)]
pub enum Child {
    /// Character data between `start` and `end` (raw, entities not resolved). `cdata` content has no entities.
    Text { start: usize, end: usize, cdata: bool },
    /// A comment or processing instruction.
    Other { start: usize, end: usize },
    Elem(usize),
}

#[derive(Debug, Clone)]
pub struct Elem {
    pub name: String,
    pub attrs: Vec<Attr>,
    /// `<` of the start tag.
    pub start: usize,
    /// Just after the start tag's `>`: where the content begins.
    pub open_end: usize,
    /// `<` of the end tag (equals `open_end` for an empty element).
    pub close_start: usize,
    /// Just after the end tag.
    pub end: usize,
    pub children: Vec<Child>,
}

impl Elem {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.name == name).map(|a| a.value.as_str())
    }
}

pub struct Tree {
    pub elems: Vec<Elem>,
    pub root: usize,
}

fn xml_char(c: char) -> bool {
    matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == ':'
}

fn name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '\u{B7}') || ('\u{300}'..='\u{36F}').contains(&c)
}

/// Every `&` starts a predefined entity or a character reference to an XML character.
fn check_refs(s: &str) -> Result<(), Error> {
    let b = s.as_bytes();
    let mut i = 0;
    while let Some(p) = b[i..].iter().position(|c| *c == b'&') {
        let at = i + p;
        let Some(len) = b[at + 1..b.len().min(at + 14)].iter().position(|c| *c == b';') else {
            return Err(perr("not well-formed XML: bad entity reference"));
        };
        let name = &s[at + 1..at + 1 + len];
        let ok = match name {
            "lt" | "gt" | "amp" | "quot" | "apos" => true,
            _ => name.strip_prefix('#').is_some_and(|n| {
                let v = match n.strip_prefix('x') {
                    Some(h) if !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit()) => u32::from_str_radix(h, 16).ok(),
                    None if !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()) => n.parse().ok(),
                    _ => None,
                };
                v.and_then(char::from_u32).is_some_and(xml_char)
            }),
        };
        if !ok {
            return Err(perr(format!("not well-formed XML: undefined entity &{name};")));
        }
        i = at + len + 2;
    }
    Ok(())
}

/// Resolves the references `check_refs` accepted. A CRLF or lone CR becomes LF, as an XML processor reads it.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find(['&', '\r']) {
        out.push_str(&rest[..p]);
        let after = &rest[p..];
        if after.starts_with('\r') {
            out.push('\n');
            rest = after[1..].strip_prefix('\n').unwrap_or(&after[1..]);
            continue;
        }
        let Some(semi) = after.find(';') else {
            out.push_str(after);
            return out;
        };
        let name = &after[1..semi];
        match name {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let v = match name.strip_prefix("#x") {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => name.strip_prefix('#').and_then(|d| d.parse().ok()),
                };
                match v.and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => out.push_str(&after[..semi + 1]),
                }
            }
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    out
}

/// An attribute value as a parser normalises it: tab, newline and CR in the source are spaces; character
/// references stay what they say.
fn attr_value(raw: &str) -> String {
    if !raw.contains(['\t', '\n', '\r', '&']) {
        return raw.to_string();
    }
    let mut spaced = String::with_capacity(raw.len());
    let mut it = raw.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                spaced.push(' ');
            }
            '\n' | '\t' => spaced.push(' '),
            c => spaced.push(c),
        }
    }
    unescape(&spaced)
}

fn find(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > b.len() {
        return None;
    }
    b[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

pub struct StartTag {
    pub name: String,
    pub name_end: usize,
    pub attrs: Vec<Attr>,
    /// Just after `>`.
    pub end: usize,
    pub self_closing: bool,
}

/// Reads a start tag at `pos` (which is a `<`).
pub fn start_tag(text: &str, pos: usize) -> Result<StartTag, Error> {
    let b = text.as_bytes();
    let mut i = pos + 1;
    let ns = i;
    let first = text[i..].chars().next();
    if !first.is_some_and(name_start) {
        return Err(perr("not well-formed XML: bad element name"));
    }
    while i < b.len() {
        let c = text[i..].chars().next().unwrap_or(' ');
        if name_char(c) {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    let name = text[ns..i].to_string();
    let name_end = i;
    let mut attrs: Vec<Attr> = Vec::new();
    loop {
        let before = i;
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        match b.get(i) {
            None => return Err(perr("not well-formed XML: unterminated start tag")),
            Some(b'>') => return Ok(StartTag { name, name_end, attrs, end: i + 1, self_closing: false }),
            Some(b'/') => {
                return if b.get(i + 1) == Some(&b'>') {
                    Ok(StartTag { name, name_end, attrs, end: i + 2, self_closing: true })
                } else {
                    Err(perr("not well-formed XML: stray / in a start tag"))
                };
            }
            Some(_) => {}
        }
        if i == before {
            return Err(perr("not well-formed XML: attributes must be separated by white space"));
        }
        let an = i;
        let first = text[i..].chars().next();
        if !first.is_some_and(name_start) {
            return Err(perr("not well-formed XML: bad attribute name"));
        }
        while i < b.len() {
            let c = text[i..].chars().next().unwrap_or(' ');
            if name_char(c) {
                i += c.len_utf8();
            } else {
                break;
            }
        }
        let aname = text[an..i].to_string();
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        if b.get(i) != Some(&b'=') {
            return Err(perr("not well-formed XML: attribute without a value"));
        }
        i += 1;
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        let q = match b.get(i) {
            Some(q @ (b'"' | b'\'')) => *q,
            _ => return Err(perr("not well-formed XML: attribute value is not quoted")),
        };
        i += 1;
        let vs = i;
        let Some(len) = b[i..].iter().position(|c| *c == q) else {
            return Err(perr("not well-formed XML: unterminated attribute value"));
        };
        let vend = i + len;
        let raw = &text[vs..vend];
        if raw.contains('<') {
            return Err(perr("not well-formed XML: < in an attribute value"));
        }
        check_refs(raw)?;
        if attrs.iter().any(|a| a.name == aname) {
            return Err(perr(format!("not well-formed XML: attribute {aname} repeated")));
        }
        attrs.push(Attr { name: aname, value: attr_value(raw), vstart: vs, vend, quote: q });
        i = vend + 1;
    }
}

/// Skips the prolog (declaration, comments, processing instructions, DOCTYPE) and returns the position of the
/// root element's `<`. A DOCTYPE with an external identifier is refused; its internal subset is never read.
fn skip_prolog(text: &str) -> Result<usize, Error> {
    let b = text.as_bytes();
    let mut i = 0;
    loop {
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        let rest = &b[i..];
        if rest.starts_with(b"<?") {
            i = find(b, i + 2, b"?>").ok_or_else(|| perr("not well-formed XML: unterminated processing instruction"))? + 2;
        } else if rest.starts_with(b"<!--") {
            i = find(b, i + 4, b"-->").ok_or_else(|| perr("not well-formed XML: unterminated comment"))? + 3;
        } else if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
            let (mut j, mut sq, mut subset) = (i + 9, 0u8, 0usize);
            let head_start = j;
            let mut head_end = None;
            while j < b.len() {
                let c = b[j];
                if sq != 0 {
                    if c == sq {
                        sq = 0;
                    }
                } else if subset > 0 && rest_starts(b, j, b"<!--") {
                    j = find(b, j + 4, b"-->").ok_or_else(|| perr("not well-formed XML: unterminated comment"))? + 2;
                } else if c == b'"' || c == b'\'' {
                    sq = c;
                } else if c == b'[' {
                    if subset == 0 {
                        head_end = Some(j);
                    }
                    subset += 1;
                } else if c == b']' {
                    subset = subset.saturating_sub(1);
                } else if c == b'>' && subset == 0 {
                    break;
                }
                j += 1;
            }
            if j >= b.len() {
                return Err(perr("not well-formed XML: unterminated DOCTYPE"));
            }
            let head = &b[head_start..head_end.unwrap_or(j)];
            if head.windows(6).any(|w| w.eq_ignore_ascii_case(b"SYSTEM")) || head.windows(6).any(|w| w.eq_ignore_ascii_case(b"PUBLIC")) {
                return Err(perr("the file points at an external DTD; refused."));
            }
            i = j + 1;
        } else {
            break;
        }
    }
    if b.get(i) == Some(&b'<') && text[i + 1..].chars().next().is_some_and(name_start) {
        Ok(i)
    } else {
        Err(perr("not well-formed XML: no root element"))
    }
}

fn rest_starts(b: &[u8], at: usize, needle: &[u8]) -> bool {
    b[at..].starts_with(needle)
}

/// Where the root element's start tag lies: `(start, end)`. Used to switch `lang` without a full parse.
pub fn root_tag(text: &str) -> Result<(usize, usize, StartTag), Error> {
    let at = skip_prolog(text)?;
    let st = start_tag(text, at)?;
    Ok((at, st.end, st))
}

pub fn parse(text: &str) -> Result<Tree, Error> {
    // XML 1.0 characters only. ASCII is checked a byte at a time.
    let b = text.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c < 0x20 && !matches!(c, b'\t' | b'\n' | b'\r') {
            return Err(perr(format!("not well-formed XML: control character at offset {i}")));
        }
        if c >= 0xC0 && !text.is_char_boundary(i) {
            continue;
        }
    }
    if !text.is_ascii() && text.chars().any(|c| !xml_char(c)) {
        return Err(perr("not well-formed XML: a character that XML does not allow"));
    }
    let root_at = skip_prolog(text)?;
    let mut elems: Vec<Elem> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut i = root_at;
    let mut root: Option<usize> = None;
    while i < b.len() {
        if stack.is_empty() && root.is_some() {
            // after the root: white space, comments and processing instructions only
            while i < b.len() && is_ws(b[i]) {
                i += 1;
            }
            if i >= b.len() {
                break;
            }
            if b[i..].starts_with(b"<!--") {
                let e = find(b, i + 4, b"-->").ok_or_else(|| perr("not well-formed XML: unterminated comment"))?;
                i = e + 3;
            } else if b[i..].starts_with(b"<?") {
                let e = find(b, i + 2, b"?>").ok_or_else(|| perr("not well-formed XML: unterminated processing instruction"))?;
                i = e + 2;
            } else {
                return Err(perr("not well-formed XML: content after the root element"));
            }
            continue;
        }
        if b[i] != b'<' {
            let end = b[i..].iter().position(|c| *c == b'<').map_or(b.len(), |p| i + p);
            let raw = &text[i..end];
            check_refs(raw)?;
            if raw.contains("]]>") {
                return Err(perr("not well-formed XML: ]]> in character data"));
            }
            let top = *stack.last().ok_or_else(|| perr("not well-formed XML: text outside the root"))?;
            elems[top].children.push(Child::Text { start: i, end, cdata: false });
            i = end;
            continue;
        }
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            let e = find(b, i + 4, b"-->").ok_or_else(|| perr("not well-formed XML: unterminated comment"))?;
            let body = &b[i + 4..e];
            if body.windows(2).any(|w| w == b"--") || body.last() == Some(&b'-') {
                return Err(perr("not well-formed XML: -- inside a comment"));
            }
            let top = *stack.last().ok_or_else(|| perr("not well-formed XML: comment outside the root"))?;
            elems[top].children.push(Child::Other { start: i, end: e + 3 });
            i = e + 3;
        } else if rest.starts_with(b"<![CDATA[") {
            let e = find(b, i + 9, b"]]>").ok_or_else(|| perr("not well-formed XML: unterminated CDATA section"))?;
            let top = *stack.last().ok_or_else(|| perr("not well-formed XML: CDATA outside the root"))?;
            elems[top].children.push(Child::Text { start: i + 9, end: e, cdata: true });
            i = e + 3;
        } else if rest.starts_with(b"<?") {
            let e = find(b, i + 2, b"?>").ok_or_else(|| perr("not well-formed XML: unterminated processing instruction"))?;
            let top = *stack.last().ok_or_else(|| perr("not well-formed XML: processing instruction outside the root"))?;
            elems[top].children.push(Child::Other { start: i, end: e + 2 });
            i = e + 2;
        } else if rest.starts_with(b"<!") {
            return Err(perr("not well-formed XML: a declaration inside the document"));
        } else if rest.starts_with(b"</") {
            let top = stack.pop().ok_or_else(|| perr("not well-formed XML: unexpected end tag"))?;
            let name = &elems[top].name;
            let ne = i + 2 + name.len();
            if !text[i + 2..].starts_with(name.as_str()) {
                return Err(perr(format!("not well-formed XML: end tag does not match <{name}>")));
            }
            let mut j = ne;
            while j < b.len() && is_ws(b[j]) {
                j += 1;
            }
            if b.get(j) != Some(&b'>') {
                return Err(perr(format!("not well-formed XML: end tag does not match <{name}>")));
            }
            elems[top].close_start = i;
            elems[top].end = j + 1;
            i = j + 1;
        } else {
            let st = start_tag(text, i)?;
            let idx = elems.len();
            let elem = Elem {
                name: st.name,
                attrs: st.attrs,
                start: i,
                open_end: st.end,
                close_start: st.end,
                end: st.end,
                children: Vec::new(),
            };
            match stack.last() {
                Some(&top) => elems[top].children.push(Child::Elem(idx)),
                None => root = Some(idx),
            }
            elems.push(elem);
            if !st.self_closing {
                stack.push(idx);
            }
            i = st.end;
        }
    }
    if !stack.is_empty() {
        return Err(perr("not well-formed XML: the file ends inside an element"));
    }
    let root = root.ok_or_else(|| perr("not well-formed XML: no root element"))?;
    Ok(Tree { elems, root })
}
