#![allow(dead_code)]
use bilingua_dxf::Dxf;
use bilingua_model::*;

pub fn opts() -> ExtractOptions {
    ExtractOptions::new()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    Dxf.extract(data, &opts())
}

/// The unit text the way LocOS shows it: codes as tokens.
pub fn token_text(u: &Unit) -> String {
    profile::tokens::to_locos_text(&u.source()).0
}

pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(token_text).collect()
}

pub fn styles(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(|u| u.context.style.clone().unwrap()).collect()
}

/// A translation written in LocOS token text, turned back into inlines for unit `u`.
pub fn target_of(u: &Unit, text: &str) -> Vec<Inline> {
    let (_, map) = profile::tokens::to_locos_text(&u.source());
    let (inl, _problems) = profile::tokens::parse_locos_text(text, &map);
    // LocOS drops a token whose number it does not know, and so does its adapter: every token still in the text
    // after the known ones became tags is unknown.
    inl.into_iter()
        .map(|i| match i {
            Inline::Text(t) => Inline::Text(strip_tokens(&t)),
            t => t,
        })
        .collect()
}

fn strip_tokens(t: &str) -> String {
    let mut out = String::new();
    let mut rest = t;
    while let Some(a) = rest.find('\u{27E6}') {
        out.push_str(&rest[..a]);
        let tail = &rest[a..];
        match tail.find('\u{27E7}') {
            Some(b) => rest = &tail[b + '\u{27E7}'.len_utf8()..],
            None => {
                out.push_str(tail);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Sets targets by unit index (LocOS mapping, token text) and merges.
pub fn build(data: &[u8], mapping: &[(usize, String)]) -> Vec<u8> {
    let mut doc = extract(data).expect("extract");
    for (i, t) in mapping {
        let u = &doc.units[*i];
        let id = u.segments[0].id.clone();
        let tg = target_of(u, t);
        doc.set_target(&id, tg).expect("set_target");
    }
    Dxf.merge(&doc).expect("merge")
}

pub fn m(items: &[(usize, &str)]) -> Vec<(usize, String)> {
    items.iter().map(|(i, s)| (*i, s.to_string())).collect()
}

/// Untouched merge and an echo merge both give back the original bytes.
pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert_eq!(Dxf.merge(&doc).unwrap(), data, "untouched merge");
    let echo: Vec<(usize, String)> = units(data).into_iter().enumerate().collect();
    assert_eq!(build(data, &echo), data, "echo merge");
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn dxf(pairs: &[(i32, String)], nl: &str, enc: fn(&str) -> Vec<u8>) -> Vec<u8> {
    let mut s = String::new();
    for (c, v) in pairs {
        s.push_str(&format!("{c:>3}{nl}{v}{nl}"));
    }
    enc(&s)
}

pub fn utf8(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

/// Windows-1252 bytes of a text (ASCII plus Latin-1 range characters used in the fixtures).
pub fn cp1252(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            '\u{20ac}' => 0x80u8,
            c if (c as u32) < 0x100 => c as u32 as u8,
            c => panic!("not cp1252: {c}"),
        })
        .collect()
}

pub fn p(items: &[(i32, &str)]) -> Vec<(i32, String)> {
    items.iter().map(|(c, v)| (*c, v.to_string())).collect()
}

pub fn header(ver: &str, cp: &str) -> Vec<(i32, String)> {
    let mut h = p(&[(0, "SECTION"), (2, "HEADER"), (9, "$ACADVER"), (1, ver)]);
    if !cp.is_empty() {
        h.extend(p(&[(9, "$DWGCODEPAGE"), (3, cp)]));
    }
    h.extend(p(&[(9, "$INSUNITS"), (70, "4"), (0, "ENDSEC")]));
    h
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus")
}

pub fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap())
        })
        .collect();
    v.sort();
    v
}
