//! The MIME walker: finds the byte range of every header block and body, without recursion (a message nested
//! thousands of levels deep costs a stack of ranges, not call frames) and without copying the data.
//!
//! The rules are LocOS's: headers end at the first blank line, a multipart is split at its `--boundary` lines
//! (the line break before a delimiter belongs to the delimiter), a missing close delimiter lets the last part run
//! to the end, and signed or encrypted messages are refused because a translation would break them.

use std::collections::BTreeMap;

pub const MAX_PARTS: usize = 500;
pub const MAX_DEPTH: usize = 16;
pub const MAX_HEADER_BLOCK: usize = 1024 * 1024;
const MAX_BOUNDARY: usize = 200;

/// Why a walk stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkError {
    /// Not an email at all, or one we will not translate (signed, encrypted, unreadable headers).
    Refused(String),
    /// Over a structural cap: parts, nesting depth, header block size.
    Limit(Cap, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    Parts,
    Depth,
    Header,
}

#[derive(Debug, Clone)]
pub struct Part {
    pub start: usize,
    /// End of the header fields (the blank line starts here).
    pub hdr_end: usize,
    pub body_start: usize,
    pub end: usize,
    pub ctype: String,
    pub children: Vec<usize>,
    /// The Content-Type header value, unfolded (`text/plain; charset=utf-8`).
    pub content_type: Option<String>,
    /// Content-Transfer-Encoding, lower case and trimmed (`7bit` when absent).
    pub cte: String,
    pub disposition: Option<String>,
}

impl Part {
    pub fn param(&self, name: &str) -> Option<String> {
        get_param(self.content_type.as_deref()?, name)
    }
    pub fn maintype(&self) -> &str {
        self.ctype.split('/').next().unwrap_or("")
    }
}

const REFUSED: &[(&str, &str)] = &[
    ("multipart/signed", "signed"),
    ("multipart/encrypted", "encrypted"),
    ("application/pkcs7-mime", "S/MIME"),
    ("application/x-pkcs7-mime", "S/MIME"),
    ("application/pkcs7-signature", "signed"),
    ("application/x-pkcs7-signature", "signed"),
    ("application/pgp-encrypted", "encrypted"),
    ("application/pgp-signature", "signed"),
    ("application/pkcs7-encrypted", "encrypted"),
];

pub fn signed_message(kind: &str) -> String {
    format!(
        "This email is {kind} (S/MIME or PGP). Translating it would break the signature or cannot read the encrypted text; export the message without signing or encryption and upload that instead."
    )
}

fn is_field_start(first: &[u8]) -> bool {
    // ^[!-9;-~]+[ \t]*:
    let mut i = 0;
    while i < first.len() && ((b'!'..=b'9').contains(&first[i]) || (b';'..=b'~').contains(&first[i])) {
        i += 1;
    }
    if i == 0 {
        return false;
    }
    while i < first.len() && (first[i] == b' ' || first[i] == b'\t') {
        i += 1;
    }
    first.get(i) == Some(&b':')
}

fn find_from(hay: &[u8], from: usize, to: usize, needle: u8) -> Option<usize> {
    if from >= to {
        return None;
    }
    hay[from..to].iter().position(|b| *b == needle).map(|p| p + from)
}

/// (end of header fields, start of body) for the entity in `data[start..end]`.
fn header_end(data: &[u8], start: usize, end: usize) -> Result<(usize, usize), WalkError> {
    for lead in [&b"\r\n"[..], &b"\n"[..]] {
        if data[start..end].starts_with(lead) {
            return Ok((start, start + lead.len()));
        }
    }
    let limit = end.min(start.saturating_add(MAX_HEADER_BLOCK + 4));
    // \n\r?\n
    let mut i = start;
    while let Some(p) = find_from(data, i, limit, b'\n') {
        let q = p + 1;
        if q < limit && data[q] == b'\n' {
            return Ok((p + 1, q + 1));
        }
        if q + 1 < limit && data[q] == b'\r' && data[q + 1] == b'\n' {
            return Ok((p + 1, q + 2));
        }
        i = q;
    }
    if end - start > MAX_HEADER_BLOCK {
        return Err(WalkError::Limit(Cap::Header, "This email has a header block over 1 MB; refused.".into()));
    }
    Ok((end, end))
}

struct Fields {
    content_type: Option<String>,
    cte: Option<String>,
    disposition: Option<String>,
}

/// Reads the fields the walker needs from one header block, the way Python's header parser would: fields end at
/// the first line that is not a field or a continuation.
fn read_fields(block: &[u8]) -> Result<Fields, WalkError> {
    let mut f = Fields { content_type: None, cte: None, disposition: None };
    let lines = crate::pyish::byte_lines_keep(block);
    let mut cur: Option<(String, Vec<u8>)> = None;
    let mut done: Vec<(String, Vec<u8>)> = Vec::new();
    for (k, line) in lines.iter().enumerate() {
        let body = line.strip_suffix(b"\n").unwrap_or(line);
        let body = body.strip_suffix(b"\r").unwrap_or(body);
        if matches!(line.first(), Some(b' ' | b'\t')) {
            if let Some((_, v)) = cur.as_mut() {
                v.extend_from_slice(body);
            }
            continue;
        }
        if body.is_empty() {
            break;
        }
        if k == 0 && body.starts_with(b"From ") {
            continue; // a Unix "From " line
        }
        let Some(colon) = body.iter().position(|c| *c == b':') else {
            if body.starts_with(b"From ") {
                return Err(WalkError::Refused("This email's MIME headers could not be read.".into()));
            }
            break; // the body starts here
        };
        if !body[..colon].iter().all(|c| (b'!'..=b'9').contains(c) || (b';'..=b'~').contains(c)) {
            break;
        }
        if let Some(prev) = cur.take() {
            done.push(prev);
        }
        if colon == 0 {
            continue;
        }
        let name = String::from_utf8_lossy(&body[..colon]).trim().to_ascii_lowercase();
        cur = Some((name, body[colon + 1..].to_vec()));
    }
    if let Some(prev) = cur.take() {
        done.push(prev);
    }
    for (name, v) in done {
        let val = String::from_utf8_lossy(&v).trim().to_string();
        match name.as_str() {
            "content-type" if f.content_type.is_none() => f.content_type = Some(val),
            "content-transfer-encoding" if f.cte.is_none() => f.cte = Some(val),
            "content-disposition" if f.disposition.is_none() => f.disposition = Some(val),
            _ => {}
        }
    }
    Ok(f)
}

fn content_type_of(v: Option<&str>) -> String {
    match v {
        None => "text/plain".into(),
        Some(v) => {
            let t = v.split(';').next().unwrap_or("").trim().to_lowercase();
            if t.matches('/').count() != 1 {
                "text/plain".into()
            } else {
                t
            }
        }
    }
}

/// Byte ranges of the body parts between the delimiters (RFC 2046 section 5.1.1).
fn split_multipart(data: &[u8], start: usize, end: usize, boundary: &[u8]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open_at: Option<usize> = None;
    let next_line = |from: usize| find_from(data, from, end, b'\n').map(|n| n + 1);
    let mut line = if start == 0 || data[start - 1] == b'\n' { Some(start) } else { next_line(start) };
    while let Some(p) = line {
        if p >= end {
            break;
        }
        let Some((mend, close)) = delimiter_at(data, p, end, boundary) else {
            line = next_line(p);
            continue;
        };
        if let Some(o) = open_at {
            let mut stop = p;
            if stop >= 2 && &data[stop - 2..stop] == b"\r\n" {
                stop -= 2;
            } else if stop >= 1 && data[stop - 1] == b'\n' {
                stop -= 1;
            }
            ranges.push((o, o.max(stop)));
        }
        if close {
            return ranges;
        }
        let nl = find_from(data, mend, end, b'\n');
        open_at = Some(nl.map_or(end, |n| n + 1));
        line = nl.map(|n| n + 1);
    }
    if let Some(o) = open_at {
        ranges.push((o, end));
    }
    ranges
}

/// `^--boundary(--)?[ \t]*\r?$` at line start `p`: (end of match, is the close delimiter).
fn delimiter_at(data: &[u8], p: usize, end: usize, boundary: &[u8]) -> Option<(usize, bool)> {
    let need = 2 + boundary.len();
    if p + need > end || &data[p..p + 2] != b"--" || &data[p + 2..p + need] != boundary {
        return None;
    }
    let tail = |from: usize| -> Option<usize> {
        let mut q = from;
        while q < end && (data[q] == b' ' || data[q] == b'\t') {
            q += 1;
        }
        if q < end && data[q] == b'\r' {
            q += 1;
        }
        (q == end || data[q] == b'\n').then_some(q)
    };
    let after = p + need;
    if after + 2 <= end && &data[after..after + 2] == b"--" {
        if let Some(q) = tail(after + 2) {
            return Some((q, true));
        }
    }
    tail(after).map(|q| (q, false))
}

/// Walks the whole message. The result is in document order (a part before its children); the first entry is the
/// message itself.
pub fn walk(data: &[u8], mbox_from: bool) -> Result<Vec<Part>, WalkError> {
    let first_end = data.iter().position(|b| *b == b'\n').map(|p| p + 1).unwrap_or(data.len());
    // a Unix mbox "From " line first (a message saved from a mail spool) is allowed outside the locos profile
    let mbox = mbox_from && data.starts_with(b"From ");
    if !(mbox || is_field_start(&data[..first_end])) || data[..data.len().min(4096)].contains(&0) {
        return Err(WalkError::Refused(
            "This file is not an email message (.eml): it must start with header lines such as From: and Subject:.".into(),
        ));
    }
    let mut parts: Vec<Part> = Vec::new();
    let mut stack: Vec<(usize, usize, usize, Option<usize>)> = vec![(0, data.len(), 0, None)];
    while let Some((s, e, depth, parent)) = stack.pop() {
        if parts.len() + 1 > MAX_PARTS {
            return Err(WalkError::Limit(Cap::Parts, format!("This email has more than {MAX_PARTS} MIME parts. Split it or remove attachments.")));
        }
        if depth > MAX_DEPTH {
            return Err(WalkError::Limit(Cap::Depth, format!("This email nests MIME parts deeper than {MAX_DEPTH} levels.")));
        }
        let (hdr_end, body_start) = header_end(data, s, e)?;
        if hdr_end - s > MAX_HEADER_BLOCK {
            return Err(WalkError::Limit(Cap::Header, "This email has a header block over 1 MB; refused.".into()));
        }
        let f = read_fields(&data[s..hdr_end])?;
        let ctype = content_type_of(f.content_type.as_deref());
        if let Some((_, kind)) = REFUSED.iter().find(|(t, _)| *t == ctype) {
            return Err(WalkError::Refused(signed_message(kind)));
        }
        let cte = f.cte.as_deref().map(|c| c.trim().to_lowercase()).filter(|c| !c.is_empty()).unwrap_or_else(|| "7bit".into());
        let disposition = f.disposition.as_deref().map(|d| d.split(';').next().unwrap_or("").trim().to_lowercase());
        let idx = parts.len();
        let part = Part { start: s, hdr_end, body_start, end: e, ctype, children: vec![], content_type: f.content_type, cte, disposition };
        let multipart = part.maintype() == "multipart";
        let boundary = if multipart { part.param("boundary") } else { None };
        parts.push(part);
        if let Some(p) = parent {
            parts[p].children.push(idx);
        }
        if let Some(b) = boundary {
            if !b.is_empty() && b.chars().count() <= MAX_BOUNDARY && b.is_ascii() {
                let ranges = split_multipart(data, body_start, e, b.as_bytes());
                for (cs, ce) in ranges.into_iter().rev() {
                    stack.push((cs, ce, depth + 1, Some(idx)));
                }
            }
        }
    }
    Ok(parts)
}

// ------------------------------------------------------------------------------------------ parameters

/// `email.utils.unquote`.
fn unquote(s: &str) -> String {
    if s.len() > 1 {
        if s.starts_with('"') && s.ends_with('"') {
            return s[1..s.len() - 1].replace("\\\\", "\\").replace("\\\"", "\"");
        }
        if s.starts_with('<') && s.ends_with('>') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// `email._parseparam`: the items of `; a=b; c="d;e"` (the first is the type itself).
fn parse_param_items(value: &str) -> Vec<String> {
    let b = value.as_bytes();
    let mut out = Vec::new();
    // items are separated by ';' outside quotes; the text before the first ';' is the type itself
    let mut start = 0usize;
    loop {
        let (mut quotes, mut escaped) = (0usize, 0usize);
        let mut end = b.len();
        let mut i = start;
        while i < b.len() {
            match b[i] {
                b'"' => {
                    quotes += 1;
                    if i > start && b[i - 1] == b'\\' {
                        escaped += 1;
                    }
                }
                b';' if i == start || (quotes - escaped) % 2 == 0 => {
                    end = i;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        let mut f = value[start..end].to_string();
        if let Some(k) = f.find('=') {
            f = format!("{}={}", f[..k].trim().to_lowercase(), f[k + 1..].trim());
        }
        out.push(f.trim().to_string());
        if end >= b.len() {
            break;
        }
        start = end + 1;
    }
    out
}

fn percent_latin1(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = |c: u8| (c as char).to_digit(16);
            if let (Some(x), Some(y)) = (h(b[i + 1]), h(b[i + 2])) {
                out.push((x * 16 + y) as u8 as char);
                i += 3;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `Message.get_param(name)` for a Content-Type header value, including RFC 2231 continuations and extended
/// values (the value part of the `(charset, language, value)` triple).
pub fn get_param(header: &str, name: &str) -> Option<String> {
    let items = parse_param_items(header);
    let mut plain: Vec<(String, String)> = Vec::new();
    let mut sections: BTreeMap<String, Vec<(u32, String, bool)>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for item in items.iter().skip(1) {
        let (n, v) = match item.split_once('=') {
            Some((n, v)) => (n.trim().to_string(), v.trim().to_string()),
            None => (item.trim().to_string(), String::new()),
        };
        if let Some(star) = n.find('*') {
            let base = n[..star].to_lowercase();
            let rest = &n[star + 1..];
            let (num, enc) = if rest.is_empty() {
                (0u32, true)
            } else {
                let enc = rest.ends_with('*');
                let digits = rest.trim_end_matches('*');
                match digits.parse::<u32>() {
                    Ok(k) => (k, enc),
                    Err(_) => {
                        plain.push((n.to_lowercase(), v));
                        continue;
                    }
                }
            };
            if !sections.contains_key(&base) {
                order.push(base.clone());
            }
            sections.entry(base).or_default().push((num, unquote(&v), enc));
        } else {
            plain.push((n.to_lowercase(), v));
        }
    }
    let want = name.to_lowercase();
    if let Some((_, v)) = plain.iter().find(|(n, _)| *n == want) {
        return Some(unquote(v));
    }
    let secs = sections.get_mut(&want)?;
    secs.sort();
    let mut value = String::new();
    let mut extended = false;
    for (_, s, enc) in secs.iter() {
        if *enc {
            value.push_str(&percent_latin1(s));
            extended = true;
        } else {
            value.push_str(s);
        }
    }
    if extended {
        let mut it = value.splitn(3, '\'');
        let (a, b, c) = (it.next(), it.next(), it.next());
        return Some(match (a, b, c) {
            (_, _, Some(c)) => c.to_string(),
            _ => percent_latin1(&value),
        });
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params() {
        let h = r#"multipart/mixed; boundary="==a;b=="; charset=utf-8 ; Format=Flowed"#;
        assert_eq!(get_param(h, "boundary").unwrap(), "==a;b==");
        assert_eq!(get_param(h, "format").unwrap(), "Flowed");
        assert_eq!(get_param(h, "charset").unwrap(), "utf-8");
        assert!(get_param(h, "delsp").is_none());
        assert_eq!(get_param("text/plain; charset*=utf-8''caf%C3%A9", "charset").unwrap(), "caf\u{c3}\u{a9}");
        assert_eq!(get_param("m/x; boundary*0=ab; boundary*1=cd", "boundary").unwrap(), "abcd");
        assert_eq!(get_param("text/plain; charset", "charset").unwrap(), "");
    }

    #[test]
    fn walks_nested_and_open_ended() {
        let m = b"Subject: s\nContent-Type: multipart/mixed; boundary=q\n\npre\n--q\nContent-Type: text/plain\n\nbody\n--q\nContent-Type: multipart/alternative; boundary=r\n\n--r\n\nA\n--r--\n";
        let p = walk(m, false).unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].children, vec![1, 2]);
        assert_eq!(p[2].children, vec![3]);
        assert_eq!(&m[p[1].body_start..p[1].end], b"body");
        assert_eq!(&m[p[3].body_start..p[3].end], b"A");
    }

    #[test]
    fn refuses_what_it_should() {
        assert!(matches!(walk(b"%PDF-1.4 x", false), Err(WalkError::Refused(_))));
        assert!(matches!(walk(b"Subject: x\nContent-Type: application/pkcs7-mime\n\nA\n"), Err(WalkError::Refused(_))));
        let mut deep = b"Subject: d\n".to_vec();
        for i in 0..MAX_DEPTH + 2 {
            deep.extend_from_slice(format!("Content-Type: multipart/mixed; boundary=\"b{i}\"\n\n--b{i}\n").as_bytes());
        }
        deep.extend_from_slice(b"Content-Type: text/plain\n\nhi\n");
        assert!(matches!(walk(&deep, false), Err(WalkError::Limit(Cap::Depth, _))));
    }
}
