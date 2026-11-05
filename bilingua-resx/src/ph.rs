//! .NET composite-format placeholders: `{0}`, `{0,10}`, `{0:N2}` and named `{dbName}`. `{{` and `}}` are escaped
//! braces and stay text.

pub enum Piece<'a> {
    Text(&'a str),
    Ph(&'a str),
}

fn is_ph(inner: &str) -> bool {
    let name_end = inner.find([',', ':']).unwrap_or(inner.len());
    let name = &inner[..name_end];
    !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'))
        && !inner.contains(['<', '>', '&', '"', '\'', '\n', '\r', '{'])
}

pub fn split(s: &str) -> Vec<Piece<'_>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let (mut start, mut i) = (0usize, 0usize);
    while i < b.len() {
        if b[i] != b'{' {
            i += 1;
            continue;
        }
        if b.get(i + 1) == Some(&b'{') {
            i += 2;
            continue;
        }
        let Some(rel) = s[i + 1..].find(['}', '{']) else { break };
        let close = i + 1 + rel;
        if b[close] == b'}' && is_ph(&s[i + 1..close]) {
            if i > start {
                out.push(Piece::Text(&s[start..i]));
            }
            out.push(Piece::Ph(&s[i..=close]));
            start = close + 1;
            i = close + 1;
        } else {
            i += 1;
        }
    }
    if start < s.len() {
        out.push(Piece::Text(&s[start..]));
    }
    out
}
