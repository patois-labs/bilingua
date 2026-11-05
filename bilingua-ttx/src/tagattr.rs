//! Reading and setting one attribute in the text of a (well-formed) start tag, keeping every other byte.

use crate::tokens::esc_text;

/// (name, value start, value end, quote) of each attribute of a start tag.
fn scan(tag: &str) -> Vec<(&str, usize, usize, u8)> {
    let b = tag.as_bytes();
    let n = b.len();
    let ws = |c: u8| matches!(c, b' ' | b'\t' | b'\r' | b'\n');
    let mut i = 1;
    while i < n && !ws(b[i]) && b[i] != b'/' && b[i] != b'>' {
        i += 1;
    }
    let mut out = Vec::new();
    loop {
        while i < n && ws(b[i]) {
            i += 1;
        }
        if i >= n || b[i] == b'/' || b[i] == b'>' {
            return out;
        }
        let s = i;
        while i < n && !ws(b[i]) && b[i] != b'=' {
            i += 1;
        }
        let name = &tag[s..i];
        while i < n && ws(b[i]) {
            i += 1;
        }
        if i >= n || b[i] != b'=' {
            return out;
        }
        i += 1;
        while i < n && ws(b[i]) {
            i += 1;
        }
        if i >= n || !matches!(b[i], b'"' | b'\'') {
            return out;
        }
        let q = b[i];
        let vs = i + 1;
        let Some(len) = b[vs..].iter().position(|c| *c == q) else { return out };
        out.push((name, vs, vs + len, q));
        i = vs + len + 1;
    }
}

/// The raw (still escaped) value of an attribute.
pub fn get<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    scan(tag).into_iter().find(|a| a.0 == name).map(|a| &tag[a.1..a.2])
}

fn esc_attr(v: &str, quote: u8) -> String {
    let s = esc_text(v).replace('"', "&quot;");
    if quote == b'\'' {
        s.replace('\'', "&apos;")
    } else {
        s
    }
}

pub fn set(tag: &str, name: &str, value: &str) -> String {
    if let Some((_, vs, ve, q)) = scan(tag).into_iter().find(|a| a.0 == name) {
        return format!("{}{}{}", &tag[..vs], esc_attr(value, q), &tag[ve..]);
    }
    let mut close = tag.len() - if tag.ends_with("/>") { 2 } else { 1 };
    while close > 0 && tag.as_bytes()[close - 1].is_ascii_whitespace() {
        close -= 1;
    }
    format!("{} {name}=\"{}\"{}", &tag[..close], esc_attr(value, b'"'), &tag[close..])
}
