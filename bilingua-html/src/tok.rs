//! A tolerant HTML tokenizer for `sanitize_html`. It never fails and never copies: every token is a byte range of
//! the input, and the ranges cover the input completely.

#[derive(Debug, Clone)]
pub struct Attr {
    /// Lowercase name.
    pub name: String,
    pub name_start: usize,
    pub name_end: usize,
    /// Inner value range (without quotes), when the attribute has a value.
    pub val: Option<(usize, usize)>,
    /// The whole value token, with quotes when present.
    pub full: Option<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub enum Tok {
    Text(usize, usize),
    Comment(usize, usize),
    /// Doctype, CDATA, processing instruction.
    Decl(usize, usize),
    /// Content of script, style and textarea.
    Raw(usize, usize),
    Start { start: usize, end: usize, name: String, attrs: Vec<Attr> },
    End { start: usize, end: usize, name: String },
}

impl Tok {
    pub fn range(&self) -> (usize, usize) {
        match self {
            Tok::Text(a, b) | Tok::Comment(a, b) | Tok::Decl(a, b) | Tok::Raw(a, b) => (*a, *b),
            Tok::Start { start, end, .. } | Tok::End { start, end, .. } => (*start, *end),
        }
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn name_byte(b: u8) -> bool {
    !(is_ws(b) || b == b'/' || b == b'>')
}

pub fn tokenize(s: &str) -> Vec<Tok> {
    let b = s.as_bytes();
    let n = b.len();
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    let mut text_start = 0;
    while i < n {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let Some(tok) = markup(s, i) else {
            i += 1;
            continue;
        };
        if text_start < i {
            out.push(Tok::Text(text_start, i));
        }
        let (_, end) = tok.range();
        let mut follow: Option<Tok> = None;
        if let Tok::Start { name, end, .. } = &tok {
            match name.as_str() {
                "script" | "style" | "textarea" => {
                    let close = find_close(b, *end, name);
                    if close > *end {
                        follow = Some(Tok::Raw(*end, close));
                    }
                }
                "title" => {
                    let close = find_close(b, *end, name);
                    if close > *end {
                        follow = Some(Tok::Text(*end, close));
                    }
                }
                _ => {}
            }
        }
        out.push(tok);
        i = end;
        if let Some(f) = follow {
            i = f.range().1;
            out.push(f);
        }
        text_start = i;
    }
    if text_start < n {
        out.push(Tok::Text(text_start, n));
    }
    out
}

/// Start of the matching close tag `</name` (case-insensitive), or the end of the input.
fn find_close(b: &[u8], from: usize, name: &str) -> usize {
    let nb = name.as_bytes();
    let mut i = from;
    while i + 2 + nb.len() <= b.len() {
        if b[i] == b'<' && b[i + 1] == b'/' && b[i + 2..i + 2 + nb.len()].eq_ignore_ascii_case(nb) {
            let after = b.get(i + 2 + nb.len()).copied();
            if after.is_none_or(|c| is_ws(c) || c == b'>' || c == b'/') {
                return i;
            }
        }
        i += 1;
    }
    b.len()
}

fn find_from(b: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if pat.is_empty() || from > b.len() {
        return None;
    }
    b[from..].windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

fn markup(s: &str, i: usize) -> Option<Tok> {
    let b = s.as_bytes();
    let n = b.len();
    let next = *b.get(i + 1)?;
    if b[i..].starts_with(b"<!--") {
        let end = find_from(b, i + 4, b"-->").map(|p| p + 3).unwrap_or(n);
        return Some(Tok::Comment(i, end));
    }
    if b[i..].starts_with(b"<![CDATA[") {
        let end = find_from(b, i, b"]]>").map(|p| p + 3).unwrap_or(n);
        return Some(Tok::Decl(i, end));
    }
    if next == b'!' || next == b'?' {
        let end = find_from(b, i, b">").map(|p| p + 1).unwrap_or(n);
        return Some(Tok::Decl(i, end));
    }
    if next == b'/' {
        let c = *b.get(i + 2)?;
        if !c.is_ascii_alphabetic() {
            return None;
        }
        let mut j = i + 2;
        while j < n && name_byte(b[j]) {
            j += 1;
        }
        let name = s[i + 2..j].to_ascii_lowercase();
        let end = find_from(b, j, b">").map(|p| p + 1).unwrap_or(n);
        return Some(Tok::End { start: i, end, name });
    }
    if !next.is_ascii_alphabetic() {
        return None;
    }
    let mut j = i + 1;
    while j < n && name_byte(b[j]) {
        j += 1;
    }
    let name = s[i + 1..j].to_ascii_lowercase();
    let mut attrs = Vec::new();
    loop {
        while j < n && (is_ws(b[j]) || b[j] == b'/') {
            j += 1;
        }
        if j >= n {
            break;
        }
        if b[j] == b'>' {
            j += 1;
            break;
        }
        let ns = j;
        // An attribute name may start with '=' in broken markup; take at least one byte.
        j += 1;
        while j < n && !(is_ws(b[j]) || b[j] == b'=' || b[j] == b'>' || b[j] == b'/') {
            j += 1;
        }
        let aname = s[ns..j].to_ascii_lowercase();
        let j0 = j;
        let mut k = j;
        while k < n && is_ws(b[k]) {
            k += 1;
        }
        if k < n && b[k] == b'=' {
            k += 1;
            while k < n && is_ws(b[k]) {
                k += 1;
            }
            if k < n && (b[k] == b'"' || b[k] == b'\'') {
                let q = b[k];
                let vs = k + 1;
                let mut e = vs;
                while e < n && b[e] != q {
                    e += 1;
                }
                let full_end = if e < n { e + 1 } else { e };
                attrs.push(Attr { name: aname, name_start: ns, name_end: j0, val: Some((vs, e)), full: Some((k, full_end)) });
                j = full_end;
            } else {
                let vs = k;
                let mut e = vs;
                while e < n && !(is_ws(b[e]) || b[e] == b'>') {
                    e += 1;
                }
                attrs.push(Attr { name: aname, name_start: ns, name_end: j0, val: Some((vs, e)), full: Some((vs, e)) });
                j = e;
            }
        } else {
            attrs.push(Attr { name: aname, name_start: ns, name_end: j0, val: None, full: None });
        }
    }
    Some(Tok::Start { start: i, end: j, name, attrs })
}


pub fn attr_value<'a>(s: &'a str, attrs: &[Attr], name: &str) -> Option<&'a str> {
    attrs.iter().find(|a| a.name == name).and_then(|a| a.val.map(|(x, y)| &s[x..y]))
}
