//! A tolerant tag scanner. It follows the tokenizer of `bilingua-html` (so both agree where a tag ends) and skips
//! comments, CDATA, processing instructions, declarations and the raw content of `script`, `style`, `textarea`
//! and `title`.

#[derive(Debug, Clone)]
pub struct Attr {
    /// Lowercase name.
    pub name: String,
    /// Inner value range (without quotes); `None` when the attribute has no value.
    pub value: Option<(usize, usize)>,
    pub quoted: bool,
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub start: usize,
    pub end: usize,
    /// Where scanning goes on (past the raw content of `script` and the like).
    pub resume: usize,
    /// Lowercase name.
    pub name: String,
    pub closing: bool,
    pub self_closing: bool,
    pub attrs: Vec<Attr>,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn find_from(b: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if pat.is_empty() || from > b.len() {
        return None;
    }
    b[from..].windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

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

/// The next start or end tag at or after `i`, or `None`.
pub fn next_tag(s: &str, mut i: usize) -> Option<Tag> {
    let b = s.as_bytes();
    let n = b.len();
    while i < n {
        i += b[i..].iter().position(|&c| c == b'<')?;
        if b[i..].starts_with(b"<!--") {
            i = find_from(b, i + 4, b"-->").map_or(n, |p| p + 3);
            continue;
        }
        if b[i..].starts_with(b"<![CDATA[") {
            i = find_from(b, i, b"]]>").map_or(n, |p| p + 3);
            continue;
        }
        let next = *b.get(i + 1)?;
        if next == b'!' || next == b'?' {
            i = find_from(b, i, b">").map_or(n, |p| p + 1);
            continue;
        }
        if next == b'/' {
            let c = *b.get(i + 2)?;
            if !c.is_ascii_alphabetic() {
                i += 1;
                continue;
            }
            let mut j = i + 2;
            while j < n && !(is_ws(b[j]) || b[j] == b'/' || b[j] == b'>') {
                j += 1;
            }
            let name = s[i + 2..j].to_ascii_lowercase();
            let end = find_from(b, j, b">").map_or(n, |p| p + 1);
            return Some(Tag { start: i, end, resume: end, name, closing: true, self_closing: false, attrs: vec![] });
        }
        if !next.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < n && !(is_ws(b[j]) || b[j] == b'/' || b[j] == b'>') {
            j += 1;
        }
        let name = s[i + 1..j].to_ascii_lowercase();
        let mut attrs = Vec::new();
        let mut self_close = false;
        loop {
            while j < n && (is_ws(b[j]) || b[j] == b'/') {
                self_close = b[j] == b'/';
                j += 1;
            }
            if j >= n {
                break;
            }
            if b[j] == b'>' {
                j += 1;
                break;
            }
            self_close = false;
            let ns = j;
            j += 1;
            while j < n && !(is_ws(b[j]) || b[j] == b'=' || b[j] == b'>' || b[j] == b'/') {
                j += 1;
            }
            let aname = s.get(ns..j).unwrap_or("").to_ascii_lowercase();
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
                    attrs.push(Attr { name: aname, value: Some((vs, e)), quoted: true });
                    j = if e < n { e + 1 } else { e };
                } else {
                    let vs = k;
                    let mut e = vs;
                    while e < n && !(is_ws(b[e]) || b[e] == b'>') {
                        e += 1;
                    }
                    attrs.push(Attr { name: aname, value: Some((vs, e)), quoted: false });
                    j = e;
                }
            } else {
                attrs.push(Attr { name: aname, value: None, quoted: false });
            }
        }
        let end = j;
        let mut resume = end;
        if !self_close && matches!(name.as_str(), "script" | "style" | "textarea" | "title") {
            let close = find_close(b, end, &name);
            if close > end {
                resume = close;
            }
        }
        return Some(Tag { start: i, end, resume, name, closing: false, self_closing: self_close, attrs });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_skip_comments_and_raw_content() {
        let s = "<!-- <a> --><p class='x>y'>t</p><script>if (a<b) <i></script><title><b></title><br/>";
        let mut i = 0;
        let mut names = vec![];
        while let Some(t) = next_tag(s, i) {
            names.push(format!("{}{}", if t.closing { "/" } else { "" }, t.name));
            i = t.resume;
        }
        assert_eq!(names, ["p", "/p", "script", "/script", "title", "/title", "br"]);
    }

    #[test]
    fn attribute_values() {
        let s = "<MadCap:keyword term=\"a;b\" x=y z />";
        let t = next_tag(s, 0).unwrap();
        assert_eq!(t.name, "madcap:keyword");
        assert!(t.attrs.iter().any(|a| a.name == "term" && a.quoted));
        assert!(t.self_closing);
        let (a, b) = t.attrs[0].value.unwrap();
        assert_eq!(&s[a..b], "a;b");
    }
}
