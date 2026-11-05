//! A small lossless, strict XML tree. Every node keeps its original text, so writing the tree back gives the
//! input exactly. Strict means what lxml (libxml2) is for Localization OS: a part that is not well formed is
//! refused (mismatched or unclosed tags, a second root, undefined entities, `<` in an attribute value, a
//! duplicate attribute, characters XML 1.0 cannot hold). No DOCTYPE and no entity declaration is ever read.

#[derive(Debug, Clone)]
pub enum Node {
    /// Character data (entities still escaped) or a whole CDATA section.
    Text(String),
    /// A comment or processing instruction, kept as written.
    Raw(String),
    El(El),
}

#[derive(Debug, Clone, Default)]
pub struct El {
    /// Preorder number in the part (the first element is 0).
    pub id: u32,
    pub name: String,
    /// The start tag as written, e.g. `<Cell N='Label' V='x'/>`.
    pub start: String,
    /// The end tag as written; empty for a self-closing element.
    pub end: String,
    pub children: Vec<Node>,
}

impl Node {
    pub fn write_raw(&self, out: &mut String) {
        match self {
            Node::Text(s) | Node::Raw(s) => out.push_str(s),
            Node::El(e) => e.write_raw(out),
        }
    }
}

impl El {
    pub fn write_raw(&self, out: &mut String) {
        out.push_str(&self.start);
        for c in &self.children {
            c.write_raw(out);
        }
        out.push_str(&self.end);
    }
    pub fn raw(&self) -> String {
        let mut s = String::new();
        self.write_raw(&mut s);
        s
    }
    pub fn local(&self) -> &str {
        local_name(&self.name)
    }
    /// The decoded value of an attribute, matched on its full name.
    pub fn attr(&self, name: &str) -> Option<String> {
        find_attr(&self.start, name).map(|a| decode(&self.start[a.vs..a.ve], true))
    }
    pub fn has_attr(&self, name: &str) -> bool {
        find_attr(&self.start, name).is_some()
    }
    pub fn elements(&self) -> impl Iterator<Item = &El> {
        self.children.iter().filter_map(|n| if let Node::El(e) = n { Some(e) } else { None })
    }
    /// Replaces (or appends) an attribute; the quote character in use is kept.
    pub fn set_attr(&mut self, name: &str, value: &str) {
        if let Some(a) = find_attr(&self.start, name) {
            let q = a.quote as char;
            let esc = escape_attr(value, q);
            self.start.replace_range(a.vs..a.ve, &esc);
        } else {
            let q = self.quote_in_use();
            let esc = escape_attr(value, q);
            let close = if self.start.ends_with("/>") { self.start.len() - 2 } else { self.start.len() - 1 };
            let at = self.start[..close].trim_end().len();
            self.start.insert_str(at, &format!(" {name}={q}{esc}{q}"));
        }
    }
    pub fn quote_in_use(&self) -> char {
        self.start.bytes().find(|b| *b == b'"' || *b == b'\'').map(|b| b as char).unwrap_or('"')
    }
    pub fn remove_attr(&mut self, name: &str) {
        if let Some(a) = find_attr(&self.start, name) {
            // from the whitespace before the name to the closing quote
            let mut s = a.ns;
            let b = self.start.as_bytes();
            while s > 0 && b[s - 1].is_ascii_whitespace() {
                s -= 1;
            }
            self.start.replace_range(s..a.ve + 1, "");
        }
    }
}

pub fn local_name(name: &str) -> &str {
    name.rsplit_once(':').map(|(_, l)| l).unwrap_or(name)
}

pub fn prefix_of(name: &str) -> &str {
    name.split_once(':').map(|(p, _)| p).unwrap_or("")
}

// ----------------------------------------------------------------------------------------------- attributes

/// One attribute of a start tag: `ns..ne` is the name, `vs..ve` the value (without quotes), all byte offsets in
/// the start tag.
#[derive(Debug, Clone, Copy)]
pub struct Attr {
    pub ns: usize,
    pub ne: usize,
    pub vs: usize,
    pub ve: usize,
    pub quote: u8,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn name_start(c: char) -> bool {
    c == ':' || c == '_' || c.is_ascii_alphabetic() || (c as u32) >= 0xC0 && c != '\u{D7}' && c != '\u{F7}' && (c as u32) < 0x2000 || ('\u{2070}'..='\u{218F}').contains(&c) || ('\u{2C00}'..='\u{2FEF}').contains(&c) || ('\u{3001}'..='\u{D7FF}').contains(&c) || ('\u{F900}'..='\u{FDCF}').contains(&c) || ('\u{FDF0}'..='\u{FFFD}').contains(&c) || (c as u32) >= 0x10000 && (c as u32) <= 0xEFFFF
}

fn name_char(c: char) -> bool {
    name_start(c) || c == '-' || c == '.' || c.is_ascii_digit() || c == '\u{B7}' || ('\u{300}'..='\u{36F}').contains(&c) || ('\u{203F}'..='\u{2040}').contains(&c)
}

fn valid_name(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(c) if name_start(c) => it.all(name_char),
        _ => false,
    }
}

/// Walks the attributes of one start tag (`<name a="1" b='2'/>`), calling `f` for each. Errors on anything that is
/// not a well-formed attribute list.
fn scan_attrs(start: &str, mut f: impl FnMut(Attr) -> Result<(), String>) -> Result<(), String> {
    let b = start.as_bytes();
    let n = b.len();
    let mut i = 1;
    while i < n && !is_ws(b[i]) && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    loop {
        let had_ws = i < n && is_ws(b[i]);
        while i < n && is_ws(b[i]) {
            i += 1;
        }
        if i >= n || b[i] == b'>' || b[i] == b'/' {
            return Ok(());
        }
        if !had_ws {
            return Err("an attribute must be separated from the previous one by a space".into());
        }
        let ns = i;
        while i < n && b[i] != b'=' && !is_ws(b[i]) && b[i] != b'>' && b[i] != b'/' {
            i += 1;
        }
        let ne = i;
        while i < n && is_ws(b[i]) {
            i += 1;
        }
        if i >= n || b[i] != b'=' {
            return Err(format!("attribute {:?} has no value", &start[ns..ne]));
        }
        i += 1;
        while i < n && is_ws(b[i]) {
            i += 1;
        }
        if i >= n || (b[i] != b'"' && b[i] != b'\'') {
            return Err(format!("attribute {:?} value is not quoted", &start[ns..ne]));
        }
        let q = b[i];
        i += 1;
        let vs = i;
        while i < n && b[i] != q {
            i += 1;
        }
        if i >= n {
            return Err("an attribute value is not closed".into());
        }
        f(Attr { ns, ne, vs, ve: i, quote: q })?;
        i += 1;
    }
}

/// The attribute named `name` in a start tag, if any. A malformed tag finds nothing.
pub fn find_attr(start: &str, name: &str) -> Option<Attr> {
    let mut found = None;
    let _ = scan_attrs(start, |a| {
        if &start[a.ns..a.ne] == name {
            found = Some(a);
        }
        Ok(())
    });
    found
}

/// All attributes of a start tag as (name, decoded value).
pub fn attrs_of(start: &str) -> Vec<(String, String)> {
    let mut v = Vec::new();
    let _ = scan_attrs(start, |a| {
        v.push((start[a.ns..a.ne].to_string(), decode(&start[a.vs..a.ve], true)));
        Ok(())
    });
    v
}

// ----------------------------------------------------------------------------------------------- characters

fn xml_char_ok(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// Checks the references and characters of character data (`attr` = inside an attribute value).
fn check_chars(s: &str, attr: bool) -> Result<(), String> {
    let b = s.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        let c = b[i];
        match c {
            0x20..=0x7E if c != b'&' && c != b'<' && c != b']' => i += 1,
            b'\t' | b'\n' | b'\r' => i += 1,
            0..=0x1F | 0x7F if c != 0x7F => return Err(format!("the character U+{c:04X} is not allowed in XML")),
            b'<' if attr => return Err("a '<' in an attribute value".into()),
            b']' => {
                if !attr && b[i..].starts_with(b"]]>") {
                    return Err("the sequence ']]>' is not allowed in text".into());
                }
                i += 1;
            }
            b'&' => {
                let rest = &s[i + 1..];
                let Some(semi) = rest.bytes().take(12).position(|x| x == b';') else {
                    return Err("a '&' that starts no entity".into());
                };
                let ent = &rest[..semi];
                let ok = match ent {
                    "amp" | "lt" | "gt" | "quot" | "apos" => true,
                    _ => {
                        let code = if let Some(h) = ent.strip_prefix("#x") {
                            u32::from_str_radix(h, 16).ok().filter(|_| !h.is_empty() && h.bytes().all(|x| x.is_ascii_hexdigit()))
                        } else if let Some(d) = ent.strip_prefix('#') {
                            d.parse::<u32>().ok().filter(|_| !d.is_empty() && d.bytes().all(|x| x.is_ascii_digit()))
                        } else {
                            None
                        };
                        code.is_some_and(xml_char_ok)
                    }
                };
                if !ok {
                    return Err(format!("the reference &{ent}; is not defined"));
                }
                i += 1 + semi + 1;
            }
            0xEF if b[i..].starts_with(&[0xEF, 0xBF, 0xBE]) || b[i..].starts_with(&[0xEF, 0xBF, 0xBF]) => {
                return Err("the character U+FFFE/U+FFFF is not allowed in XML".into());
            }
            _ => i += 1,
        }
    }
    Ok(())
}

/// Decodes character data or an attribute value: entities and character references resolved, literal line ends
/// normalised to `\n` (in an attribute value tabs, line feeds and carriage returns become a space), exactly as an
/// XML parser hands them over. A CDATA section is returned without its wrapper.
pub fn decode(raw: &str, attr: bool) -> String {
    let cdata = raw.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>"));
    let src = cdata.unwrap_or(raw);
    if !src.bytes().any(|b| b == b'&' || b == b'\r' || (attr && (b == b'\t' || b == b'\n'))) {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut it = src.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        match c {
            '\r' => {
                if it.peek().map(|x| x.1) == Some('\n') {
                    it.next();
                }
                out.push(if attr { ' ' } else { '\n' });
            }
            '\t' | '\n' if attr => out.push(' '),
            '&' if cdata.is_none() => {
                let rest = &src[i + 1..];
                let semi = rest.bytes().take(12).position(|x| x == b';');
                let ent = semi.map(|p| &rest[..p]);
                let ch = ent.and_then(|e| match e {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => e
                        .strip_prefix("#x")
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .or_else(|| e.strip_prefix('#').and_then(|d| d.parse::<u32>().ok()))
                        .and_then(char::from_u32),
                });
                match (ch, semi) {
                    (Some(ch), Some(p)) => {
                        out.push(ch);
                        for _ in 0..=p {
                            it.next();
                        }
                    }
                    _ => out.push('&'),
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Escapes text for element content. Characters XML 1.0 cannot hold are not expected here (see `clean_target`).
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    out
}

/// Escapes a value for an attribute quoted with `q`.
pub fn escape_attr(s: &str, q: char) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            '"' if q == '"' => out.push_str("&quot;"),
            '\'' if q == '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// A translation made safe for XML 1.0, as LocOS `xml_target_text`: pasted vertical tabs and form feeds are line
/// breaks, other control characters and U+FFFE/U+FFFF are dropped.
pub fn clean_target(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\u{0B}' | '\u{0C}' => Some('\n'),
            '\u{0}'..='\u{8}' | '\u{E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}' => None,
            c => Some(c),
        })
        .collect()
}

/// Python's `str.isspace` set: Unicode white space plus the four information separators.
pub fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1C}'..='\u{1F}').contains(&c)
}

// ----------------------------------------------------------------------------------------------------- parser

pub struct Parsed {
    pub nodes: Vec<Node>,
}

/// Parses a whole part: exactly one root element, nothing but white space, comments and processing
/// instructions around it. `max_depth` is the deepest element nesting allowed.
pub fn parse(s: &str, max_depth: usize) -> Result<Parsed, String> {
    parse_inner(s, max_depth, false)
}

/// Parses a fragment (any number of top-level nodes), for markup cut out of a part.
pub fn parse_fragment(s: &str, max_depth: usize) -> Result<Parsed, String> {
    parse_inner(s, max_depth, true)
}

fn parse_inner(s: &str, max_depth: usize, fragment: bool) -> Result<Parsed, String> {
    let b = s.as_bytes();
    let n = b.len();
    let mut i = 0;
    let mut stack: Vec<El> = Vec::new();
    let mut top: Vec<Node> = Vec::new();
    let mut next_id: u32 = 0;
    let mut roots = 0usize;
    fn push(stack: &mut [El], top: &mut Vec<Node>, node: Node) {
        match stack.last_mut() {
            Some(p) => p.children.push(node),
            None => top.push(node),
        }
    }
    while i < n {
        if b[i] != b'<' {
            let j = s[i..].find('<').map(|k| i + k).unwrap_or(n);
            let text = &s[i..j];
            if stack.is_empty() && !fragment {
                if !text.bytes().all(is_ws) {
                    return Err("text outside the root element".into());
                }
            } else {
                check_chars(text, false)?;
            }
            push(&mut stack, &mut top, Node::Text(text.to_string()));
            i = j;
            continue;
        }
        let rest = &s[i..];
        if let Some(inner) = rest.strip_prefix("<!--") {
            let e = inner.find("-->").ok_or("unterminated comment")? + 4;
            let body = &rest[4..e];
            if body.contains("--") || body.ends_with('-') {
                return Err("'--' inside a comment".into());
            }
            check_chars(body, true).or_else(|m| if m.contains("'<'") { Ok(()) } else { Err(m) })?;
            push(&mut stack, &mut top, Node::Raw(rest[..e + 3].to_string()));
            i += e + 3;
        } else if rest.starts_with("<![CDATA[") {
            if stack.is_empty() {
                return Err("a CDATA section outside the root element".into());
            }
            let e = rest.find("]]>").ok_or("unterminated CDATA section")? + 3;
            let body = &rest[9..e - 3];
            for ch in body.chars() {
                if !xml_char_ok(ch as u32) {
                    return Err(format!("the character U+{:04X} is not allowed in XML", ch as u32));
                }
            }
            push(&mut stack, &mut top, Node::Text(rest[..e].to_string()));
            i += e;
        } else if let Some(inner) = rest.strip_prefix("<?") {
            let e = rest.find("?>").ok_or("unterminated processing instruction")? + 2;
            let target: String = inner.chars().take_while(|c| !c.is_whitespace() && *c != '?').collect();
            if target.eq_ignore_ascii_case("xml") && i != 0 {
                return Err("an XML declaration that is not at the very start".into());
            }
            if target.is_empty() {
                return Err("a processing instruction with no target".into());
            }
            check_chars(&rest[2..e - 2], true).or_else(|m| if m.contains("'<'") { Ok(()) } else { Err(m) })?;
            push(&mut stack, &mut top, Node::Raw(rest[..e].to_string()));
            i += e;
        } else if rest.starts_with("<!") {
            return Err("DOCTYPE and entity declarations are not allowed".into());
        } else if rest.starts_with("</") {
            let e = rest.find('>').ok_or("unterminated end tag")?;
            let name = rest[2..e].trim_end();
            let mut el = stack.pop().ok_or_else(|| format!("unexpected end tag </{name}>"))?;
            if el.name != name {
                return Err(format!("end tag </{name}> does not match <{}>", el.name));
            }
            el.end = rest[..=e].to_string();
            if stack.is_empty() {
                roots += 1;
            }
            push(&mut stack, &mut top, Node::El(el));
            i += e + 1;
        } else {
            let e = tag_end(rest)?;
            let raw = &rest[..=e];
            let self_close = raw.ends_with("/>");
            let name_end = raw[1..].find(|c: char| c.is_whitespace() || c == '/' || c == '>').map(|k| k + 1).unwrap_or(raw.len());
            let name = &raw[1..name_end];
            if !valid_name(name) {
                return Err(format!("{name:?} is not a valid element name"));
            }
            if stack.is_empty() && !fragment && roots > 0 {
                return Err("more than one root element".into());
            }
            // attributes: well formed, no duplicates, valid values
            let mut seen: Vec<&str> = Vec::new();
            scan_attrs(raw, |a| {
                let an = &raw[a.ns..a.ne];
                if !valid_name(an) {
                    return Err(format!("{an:?} is not a valid attribute name"));
                }
                if seen.contains(&an) {
                    return Err(format!("attribute {an:?} appears twice"));
                }
                seen.push(an);
                check_chars(&raw[a.vs..a.ve], true)
            })?;
            let tail = raw[name_end..raw.len() - if self_close { 2 } else { 1 }].trim_end();
            let _ = tail;
            let el = El { id: next_id, name: name.to_string(), start: raw.to_string(), ..Default::default() };
            next_id = next_id.checked_add(1).ok_or("too many elements")?;
            if self_close {
                if stack.is_empty() {
                    roots += 1;
                }
                push(&mut stack, &mut top, Node::El(el));
            } else {
                if stack.len() >= max_depth {
                    return Err(format!("elements nested deeper than {max_depth}"));
                }
                stack.push(el);
            }
            i += e + 1;
        }
    }
    if let Some(el) = stack.last() {
        return Err(format!("element <{}> is not closed", el.name));
    }
    if !fragment && roots == 0 {
        return Err("no root element".into());
    }
    Ok(Parsed { nodes: top })
}

fn tag_end(rest: &str) -> Result<usize, String> {
    let mut quote = 0u8;
    for (k, c) in rest.bytes().enumerate().skip(1) {
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Ok(k);
        }
    }
    Err("unterminated start tag".into())
}

/// The raw text of a node list.
pub fn raw_of(nodes: &[Node]) -> String {
    let mut s = String::new();
    for n in nodes {
        n.write_raw(&mut s);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> Parsed {
        parse(s, 256).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn tree_writes_back_exactly() {
        let s = "<?xml version='1.0' encoding='utf-8' ?>\r\n<a x='1' y=\"2>\"><!-- c --><b/>t &amp; u<c>\n</c><![CDATA[x<y]]></a>\n<!-- end -->";
        let p = ok(s);
        assert_eq!(raw_of(&p.nodes), s);
    }

    #[test]
    fn refuses_what_lxml_refuses() {
        for bad in [
            "<!DOCTYPE a [<!ENTITY x 'y'>]><a/>",
            "<a><b></a></b>",
            "<a>",
            "<a/><b/>",
            "<a/>text",
            "text<a/>",
            "<a>&foo;</a>",
            "<a>&</a>",
            "<a x='1' x='2'/>",
            "<a x='<'/>",
            "<a x=1/>",
            "<a x='1'y='2'/>",
            "<a>\u{1}</a>",
            "<a>&#1;</a>",
            "<a>&#xD800;</a>",
            "<a>]]></a>",
            "<a><!-- -- --></a>",
            "<1a/>",
            "",
            "<a/><?xml version='1.0'?>",
            "<a>\u{FFFE}</a>",
        ] {
            assert!(parse(bad, 256).is_err(), "{bad:?} must be refused");
        }
        let deep = "<a>".repeat(300) + &"</a>".repeat(300);
        assert!(parse(&deep, 256).is_err());
        assert!(parse(&deep, 400).is_ok());
    }

    #[test]
    fn decode_and_escape() {
        assert_eq!(decode("a &amp; b &#x65E5; &#26412; &lt;", false), "a & b 日 本 <");
        assert_eq!(decode("<![CDATA[x < y &amp;]]>", false), "x < y &amp;");
        assert_eq!(decode("a\r\nb\rc", false), "a\nb\nc");
        assert_eq!(decode("a\r\nb\tc\n", true), "a b c ");
        assert_eq!(decode("&#13;&#10;", false), "\r\n");
        assert_eq!(escape_text("a<b&c>\r"), "a&lt;b&amp;c&gt;&#13;");
        assert_eq!(escape_attr("a\"b'c\n", '"'), "a&quot;b'c&#10;");
        assert_eq!(clean_target("a\u{1}b\u{b}c\u{c}d"), "ab\nc\nd");
    }

    #[test]
    fn attributes() {
        let a = attrs_of("<w:lang w:val=\"es-ES\" w:eastAsia='ja-JP' a = 'x&amp;y'/>");
        assert_eq!(
            a,
            vec![
                ("w:val".into(), "es-ES".into()),
                ("w:eastAsia".into(), "ja-JP".into()),
                ("a".into(), "x&y".into())
            ]
        );
        let mut p = ok("<a N='1' V=\"2\"/>");
        let Node::El(e) = &mut p.nodes[0] else { panic!() };
        e.set_attr("V", "x\"y");
        e.set_attr("Z", "3");
        assert_eq!(e.start, "<a N='1' V=\"x&quot;y\" Z='3'/>");
        e.remove_attr("N");
        assert_eq!(e.start, "<a V=\"x&quot;y\" Z='3'/>");
    }

    #[test]
    fn unicode_names() {
        ok("<日本 属性='1'/>");
    }
}
