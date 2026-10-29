//! Raw start-tag scanning: attribute names and the byte ranges of their values.
//! quick-xml gives us event boundaries; this gives us positions inside a tag.

use std::ops::Range;

#[derive(Debug, Clone)]
pub struct Attr {
    /// Start of the attribute name (absolute offset).
    pub name_start: usize,
    pub name: Range<usize>,
    /// Value bytes, excluding the quotes.
    pub value: Range<usize>,
}

impl Attr {
    /// Range covering ` name="value"` including the whitespace before the name.
    pub fn full_range(&self, buf: &[u8]) -> Range<usize> {
        let mut s = self.name_start;
        while s > 0 && matches!(buf[s - 1], b' ' | b'\t' | b'\r' | b'\n') {
            s -= 1;
        }
        s..self.value.end + 1
    }
}

#[derive(Debug, Clone)]
pub struct TagInfo {
    pub name: Range<usize>,
    pub attrs: Vec<Attr>,
    /// Where a new attribute can be inserted (just after the last attribute or the name).
    pub insert_pos: usize,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

impl TagInfo {
    /// `buf[start..end]` must be a complete start tag or empty-element tag.
    pub fn parse(buf: &[u8], start: usize, end: usize) -> TagInfo {
        let mut i = start + 1;
        let name_start = i;
        while i < end && !is_ws(buf[i]) && buf[i] != b'>' && buf[i] != b'/' {
            i += 1;
        }
        let name = name_start..i;
        let mut insert_pos = i;
        let mut attrs = Vec::new();
        loop {
            while i < end && is_ws(buf[i]) {
                i += 1;
            }
            if i >= end || buf[i] == b'>' || buf[i] == b'/' {
                break;
            }
            let ns = i;
            while i < end && buf[i] != b'=' && !is_ws(buf[i]) && buf[i] != b'>' && buf[i] != b'/' {
                i += 1;
            }
            let ne = i;
            while i < end && is_ws(buf[i]) {
                i += 1;
            }
            if i >= end || buf[i] != b'=' {
                break;
            }
            i += 1;
            while i < end && is_ws(buf[i]) {
                i += 1;
            }
            if i >= end || (buf[i] != b'"' && buf[i] != b'\'') {
                break;
            }
            let q = buf[i];
            let vs = i + 1;
            let mut j = vs;
            while j < end && buf[j] != q {
                j += 1;
            }
            if j >= end {
                break;
            }
            attrs.push(Attr { name_start: ns, name: ns..ne, value: vs..j });
            i = j + 1;
            insert_pos = i;
        }
        TagInfo { name, attrs, insert_pos }
    }

    pub fn attr(&self, buf: &[u8], name: &str) -> Option<&Attr> {
        self.attrs.iter().find(|a| &buf[a.name.clone()] == name.as_bytes())
    }

    /// Unescaped attribute value.
    pub fn value(&self, buf: &[u8], name: &str) -> Option<String> {
        self.attr(buf, name).map(|a| unescape_attr(&buf[a.value.clone()]))
    }

    /// Element name as written (with prefix).
    pub fn name_str<'a>(&self, buf: &'a [u8]) -> &'a str {
        std::str::from_utf8(&buf[self.name.clone()]).unwrap_or("")
    }
}

/// Decode the five predefined entities and numeric references in an attribute value.
pub fn unescape_attr(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    if !s.contains('&') {
        return s.into_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest: &str = &s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        match rest.find(';') {
            Some(q) => {
                match decode_entity(&rest[1..q]) {
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[..=q]),
                }
                rest = &rest[q + 1..];
            }
            None => {
                out.push_str(rest);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// `name` is the text between `&` and `;`.
pub fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let num = name.strip_prefix('#')?;
            let cp = if let Some(h) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                u32::from_str_radix(h, 16).ok()?
            } else {
                num.parse::<u32>().ok()?
            };
            char::from_u32(cp)
        }
    }
}

/// Escape for element text.
pub fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// Escape for a double-quoted attribute value.
pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}
