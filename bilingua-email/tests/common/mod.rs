#![allow(dead_code)]
//! Helpers shared by the test files: extract/merge shortcuts, LocOS-style unit text (inline tags as the
//! `⟦g1⟧` tokens), and a small independent MIME reader (it shares no code with the crate) to check what the
//! crate writes.

use bilingua_email::Email;
use bilingua_model::*;

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions { source_lang: Some("en".into()), target_lang: target.map(str::to_string), ..Default::default() }
}

pub fn ex(data: &[u8]) -> Document {
    Email.extract(data, &opts(None)).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn try_ex(data: &[u8]) -> Result<Document, Error> {
    Email.extract(data, &opts(None))
}

/// The unit text the way Localization OS shows it.
pub fn loc(u: &Unit) -> String {
    bilingua_model::locos::to_locos_text(&u.source()).0
}

pub fn texts(data: &[u8]) -> Vec<String> {
    ex(data).units.iter().map(loc).collect()
}

pub fn merged(d: &Document) -> Vec<u8> {
    Email.merge(d).unwrap_or_else(|e| panic!("merge failed: {e}"))
}

/// Sets the target of unit number `i`, written with LocOS tokens.
pub fn set_idx(d: &mut Document, i: usize, text: &str) {
    let u = &d.units[i];
    let (_, map) = bilingua_model::locos::to_locos_text(&u.source());
    let inl = bilingua_model::locos::from_locos_text(text, &map);
    let seg = u.segments[0].id.clone();
    d.set_target(&seg, inl).unwrap_or_else(|e| panic!("set_target {i}: {e}"));
}

/// Extract, translate the listed units (by position), merge.
pub fn build(data: &[u8], tr: &[(usize, &str)]) -> Vec<u8> {
    build_to(data, tr, None)
}

pub fn build_to(data: &[u8], tr: &[(usize, &str)], target: Option<&str>) -> Vec<u8> {
    let mut d = Email.extract(data, &opts(target)).unwrap_or_else(|e| panic!("extract failed: {e}"));
    for (i, t) in tr {
        set_idx(&mut d, *i, t);
    }
    merged(&d)
}

/// Translate every unit with its own text (the identity), then merge.
pub fn identity(data: &[u8]) -> Vec<u8> {
    let mut d = ex(data);
    for i in 0..d.units.len() {
        let t = loc(&d.units[i]);
        set_idx(&mut d, i, &t);
    }
    merged(&d)
}

pub fn pseudo(d: &mut Document) {
    for u in d.units.iter_mut() {
        let mut t = vec![Inline::text("\u{ab}")];
        t.extend(u.segments[0].source.iter().cloned());
        t.push(Inline::text("\u{bb}"));
        u.segments[0].target = t;
        u.segments[0].state = SegState::Draft;
    }
}

pub fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub fn contains(hay: &[u8], needle: &[u8]) -> bool {
    find(hay, needle).is_some()
}

pub fn count(hay: &[u8], needle: &[u8]) -> usize {
    let (mut n, mut i) = (0, 0);
    while i + needle.len() <= hay.len() {
        if &hay[i..i + needle.len()] == needle {
            n += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    n
}

// ------------------------------------------------------------------------------------ mini MIME reader

#[derive(Debug, Clone)]
pub struct Entity {
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub children: Vec<Entity>,
}

impl Entity {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    pub fn ctype(&self) -> String {
        self.header("content-type").map(|v| v.split(';').next().unwrap().trim().to_lowercase()).unwrap_or_else(|| "text/plain".into())
    }
    pub fn param(&self, name: &str) -> Option<String> {
        let v = self.header("content-type")?;
        for p in v.split(';').skip(1) {
            if let Some((k, val)) = p.split_once('=') {
                if k.trim().eq_ignore_ascii_case(name) {
                    return Some(val.trim().trim_matches('"').to_string());
                }
            }
        }
        None
    }
    pub fn cte(&self) -> String {
        self.header("content-transfer-encoding").map(|v| v.trim().to_lowercase()).unwrap_or_else(|| "7bit".into())
    }
    /// The decoded text of a text part.
    pub fn text(&self) -> String {
        let raw = match self.cte().as_str() {
            "base64" => b64(&self.body),
            "quoted-printable" => qp(&self.body),
            _ => self.body.clone(),
        };
        let cs = self.param("charset").unwrap_or_else(|| "us-ascii".into());
        let enc = encoding_rs::Encoding::for_label(cs.as_bytes()).unwrap_or(encoding_rs::UTF_8);
        enc.decode_without_bom_handling(&raw).0.into_owned()
    }
    pub fn leaves(&self) -> Vec<&Entity> {
        if self.children.is_empty() {
            vec![self]
        } else {
            self.children.iter().flat_map(|c| c.leaves()).collect()
        }
    }
    pub fn all(&self) -> Vec<&Entity> {
        let mut v = vec![self];
        for c in &self.children {
            v.extend(c.all());
        }
        v
    }
    /// The subject with encoded words decoded.
    pub fn subject(&self) -> String {
        let raw = self.header("subject").unwrap_or("");
        let bytes: Vec<u8> = raw.chars().map(|c| c as u8).collect();
        decode_words(&String::from_utf8(bytes).unwrap_or_else(|_| raw.to_string()))
    }
}

use bilingua_model::encoding_rs;

fn qp(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'=' {
            if i + 1 < b.len() && b[i + 1] == b'\n' {
                i += 2;
                continue;
            }
            if i + 2 < b.len() && b[i + 1] == b'\r' && b[i + 2] == b'\n' {
                i += 3;
                continue;
            }
            if i + 2 < b.len() {
                if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

pub fn b64(b: &[u8]) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for &c in b {
        if let Some(v) = val(c) {
            acc = acc << 6 | v as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
                acc &= (1 << bits) - 1;
            }
        }
    }
    out
}

pub fn decode_words(v: &str) -> String {
    let mut out = String::new();
    let mut rest = v;
    let mut prev_encoded = false;
    while let Some(p) = rest.find("=?") {
        let (before, after) = rest.split_at(p);
        let parts: Vec<&str> = after[2..].splitn(3, '?').collect();
        if parts.len() == 3 && parts[2].contains("?=") {
            let (text, tail) = parts[2].split_once("?=").unwrap();
            if !(prev_encoded && before.trim().is_empty()) {
                out.push_str(before);
            }
            let bytes = if parts[1].eq_ignore_ascii_case("q") {
                qp(text.replace('_', " ").as_bytes())
            } else {
                b64(text.as_bytes())
            };
            let enc = encoding_rs::Encoding::for_label(parts[0].as_bytes()).unwrap_or(encoding_rs::UTF_8);
            out.push_str(&enc.decode_without_bom_handling(&bytes).0);
            rest = tail;
            prev_encoded = true;
            continue;
        }
        out.push_str(before);
        out.push_str("=?");
        rest = &after[2..];
        prev_encoded = false;
    }
    out.push_str(rest);
    out
}

fn split_headers(block: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in block.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.starts_with([' ', '\t']) {
            if let Some(last) = out.last_mut() {
                last.1.push_str(line);
            }
        } else if let Some((n, v)) = line.split_once(':') {
            out.push((n.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

/// Parses a message into entities (multipart split at `--boundary` lines).
pub fn parse(data: &[u8]) -> Entity {
    let text: String = data.iter().map(|b| *b as char).collect();
    parse_str(&text)
}

fn parse_str(text: &str) -> Entity {
    let (head, body) = if let Some(p) = text.find("\r\n\r\n") {
        (&text[..p], &text[p + 4..])
    } else if let Some(p) = text.find("\n\n") {
        (&text[..p], &text[p + 2..])
    } else {
        (text, "")
    };
    let mut e = Entity { headers: split_headers(head), body: body.chars().map(|c| c as u8).collect(), children: vec![] };
    if e.ctype().starts_with("multipart/") {
        if let Some(b) = e.param("boundary") {
            let delim = format!("--{b}");
            let mut cur: Option<String> = None;
            for line in body.split_inclusive('\n') {
                let t = line.trim_end_matches(['\r', '\n']);
                if t == delim || t == format!("{delim}--") {
                    if let Some(c) = cur.take() {
                        let c = c.strip_suffix("\r\n").or_else(|| c.strip_suffix('\n')).unwrap_or(&c).to_string();
                        e.children.push(parse_str(&c));
                    }
                    if t == delim {
                        cur = Some(String::new());
                    }
                } else if let Some(c) = cur.as_mut() {
                    c.push_str(line);
                }
            }
            if let Some(c) = cur.take() {
                e.children.push(parse_str(&c));
            }
        }
    }
    e
}

// ------------------------------------------------------------------------------------------- fixtures

pub fn b64enc(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if ch.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if ch.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

/// `base64.encodebytes` with the line ends changed to CRLF.
pub fn b64_crlf(data: &[u8]) -> String {
    let flat = b64enc(data);
    flat.as_bytes().chunks(76).map(|c| format!("{}\r\n", std::str::from_utf8(c).unwrap())).collect()
}

pub fn png() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend(0u8..200);
    v
}

pub const MIXED_HTML: &str = "<html><head><meta charset=\"utf-8\"></head><body><p>Hello <b>Bob</b>,</p><p>The caf\u{e9} opens at 9.</p><img src=\"cid:logo@example.com\" alt=\"Company logo\"><a href=\"https://tracker.example.com/x\">Open</a></body></html>";

/// multipart/mixed > multipart/related > multipart/alternative (plain QP latin-1, html base64) + an inline image
/// (Content-ID) + a PDF attachment + a text attachment.
pub fn mixed() -> Vec<u8> {
    let png_b64 = b64enc(&png());
    let s = format!(
        "Return-Path: <ann@example.com>\r\n\
Received: from mx.example.com (mx.example.com [192.0.2.1])\r\n\
\tby mail.example.org; Sat, 10 Oct 2026 20:00:00 -0600\r\n\
From: Ann <ann@example.com>\r\n\
To: Bob <bob@example.org>\r\n\
Subject: =?ISO-8859-1?Q?Caf=E9_menu?= for {{0}} guests\r\n\
Date: Sat, 10 Oct 2026 20:00:00 -0600\r\n\
Message-ID: <abc123@example.com>\r\n\
Content-Language: en-US\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"==outer==\"\r\n\
\r\n\
This is a multi-part message in MIME format.\r\n\
--==outer==\r\n\
Content-Type: multipart/related; boundary=\"rel\"\r\n\
\r\n\
--rel\r\n\
Content-Type: multipart/alternative; boundary=alt\r\n\
\r\n\
--alt\r\n\
Content-Type: text/plain; charset=\"iso-8859-1\"\r\n\
Content-Transfer-Encoding: quoted-printable\r\n\
\r\n\
Hello Bob,\r\n\
\r\n\
The caf=E9 opens at 9. Please confirm by replying to this message before the =\r\n\
end of the week so we can book a table for everyone.\r\n\
\r\n\
> Earlier: are we still on?\r\n\
-- \r\n\
Ann\r\n\
--alt\r\n\
Content-Type: text/html; charset=utf-8\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
{html}--alt--\r\n\
\r\n\
--rel\r\n\
Content-Type: image/png\r\n\
Content-Transfer-Encoding: base64\r\n\
Content-ID: <logo@example.com>\r\n\
Content-Disposition: inline; filename=\"logo.png\"\r\n\
\r\n\
{png_b64}\r\n\
--rel--\r\n\
--==outer==\r\n\
Content-Type: text/plain; charset=us-ascii; name=\"notes.txt\"\r\n\
Content-Disposition: attachment; filename=\"notes.txt\"\r\n\
\r\n\
Attachment text is never translated.\r\n\
--==outer==\r\n\
Content-Type: application/pdf; name=\"menu.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
Content-Disposition: attachment; filename=\"menu.pdf\"\r\n\
\r\n\
JVBERi0xLjQKJcOkw7zDtsOfCg==\r\n\
--==outer==--\r\n\
Epilogue stays.\r\n",
        html = b64_crlf(MIXED_HTML.as_bytes()),
    );
    s.into_bytes()
}

pub const SIMPLE: &[u8] = b"From: a@example.com\n\
To: b@example.com\n\
Subject: Weekly update\n\
MIME-Version: 1.0\n\
Content-Type: text/plain; charset=us-ascii\n\
Content-Transfer-Encoding: 7bit\n\
\n\
Hi team,\n\
\n\
Your order %1$s ships on {date}.\n\
\x20 Indented line stays indented.\n\
- first item\n\
- second item\n";

pub const FLOWED: &[u8] = b"From: a@example.com\r\n\
Subject: Flowed\r\n\
Content-Type: text/plain; charset=utf-8; format=flowed\r\n\
Content-Transfer-Encoding: 8bit\r\n\
\r\n\
This is one long logical line that a mail client wrapped with soft breaks \r\n\
so it fits in seventy two columns.\r\n\
\r\n\
>Quoted flowed text \r\n\
>continues here.\r\n";

pub const BND_MSG: &[u8] = b"Subject: s\nMIME-Version: 1.0\n\
Content-Type: multipart/mixed; boundary=\"BND\"\n\n\
--BND\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: 8bit\n\n\
Hello there.\n\
--BND\nContent-Type: text/html; charset=us-ascii\n\
Content-Transfer-Encoding: quoted-printable\n\n\
<p>Hi</p>\n\
--BND\nContent-Type: application/octet-stream\n\
Content-Disposition: attachment; filename=a.bin\n\nAAAA\n--BND--\n";
