#![allow(dead_code)]
use bilingua_model::*;
use bilingua_xtg::Xtg;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    Xtg.extract(data, &opts())
}

/// The unit text the way LocOS shows it: codes as `\u{27E6}x1\u{27E7}` tokens.
pub fn token_text(u: &Unit) -> String {
    profile::tokens::to_locos_text(&u.source()).0
}

pub fn units(data: &[u8]) -> Vec<(String, String)> {
    extract(data)
        .expect("extract")
        .units
        .iter()
        .map(|u| (token_text(u), u.context.style.clone().unwrap_or_default()))
        .collect()
}

pub fn texts(data: &[u8]) -> Vec<String> {
    units(data).into_iter().map(|u| u.0).collect()
}

/// LocOS token text to inline codes by token number, whatever the unit has: an unknown number stays a tag with
/// that id (merge must drop it), a repeated token is repeated.
pub fn from_tokens(text: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c == '\u{27E6}' {
            if let Some(end) = rest.find('\u{27E7}') {
                let body = &rest[3..end];
                let (slash, body) = match body.strip_prefix('/') {
                    Some(b) => (true, b),
                    None => (false, body),
                };
                let letter = body.chars().next().unwrap();
                let id = &body[1..];
                if !lit.is_empty() {
                    out.push(Inline::Text(std::mem::take(&mut lit)));
                }
                let kind = match (letter, slash) {
                    ('g', false) => TagKind::Open,
                    ('g', true) => TagKind::Close,
                    _ => TagKind::Standalone,
                };
                out.push(Inline::tag(id, kind, ""));
                rest = &rest[end + 3..];
                continue;
            }
        }
        lit.push(c);
        rest = &rest[c.len_utf8()..];
    }
    if !lit.is_empty() {
        out.push(Inline::Text(lit));
    }
    out
}

/// Sets targets by unit index (LocOS token text) and merges.
pub fn build_with(data: &[u8], mapping: &[(usize, &str)], opts: &ExtractOptions) -> Vec<u8> {
    let mut doc = Xtg.extract(data, opts).expect("extract");
    for (i, t) in mapping {
        doc.units[*i].segments[0].target = from_tokens(t);
    }
    Xtg.merge(&doc).expect("merge")
}

pub fn build(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    build_with(data, mapping, &opts())
}

pub fn build_s(data: &[u8], mapping: &[(usize, &str)]) -> String {
    String::from_utf8(build(data, mapping)).expect("utf-8 output")
}

/// Untouched merge and an echo merge both give back the original bytes.
pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert_eq!(Xtg.merge(&doc).unwrap(), data, "untouched merge");
    let t = texts(data);
    let echo: Vec<(usize, &str)> = t.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    assert_eq!(build(data, &echo), data, "echo merge");
}

pub fn utf16le(s: &str, bom: bool) -> Vec<u8> {
    let mut v = if bom { vec![0xFF, 0xFE] } else { vec![] };
    v.extend(s.encode_utf16().flat_map(|u| u.to_le_bytes()));
    v
}

pub fn utf16be(s: &str, bom: bool) -> Vec<u8> {
    let mut v = if bom { vec![0xFE, 0xFF] } else { vec![] };
    v.extend(s.encode_utf16().flat_map(|u| u.to_be_bytes()));
    v
}

pub fn enc(e: &'static bilingua_model::encoding_rs::Encoding, s: &str) -> Vec<u8> {
    let (b, _, bad) = e.encode(s);
    assert!(!bad, "not encodable: {s}");
    b.into_owned()
}

pub fn err_text(data: &[u8]) -> String {
    match extract(data) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("not refused: {:?}", &data[..data.len().min(60)]),
    }
}
