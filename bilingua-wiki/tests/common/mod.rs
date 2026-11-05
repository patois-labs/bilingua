#![allow(dead_code)]
use bilingua_model::*;
use bilingua_wiki::{DokuWiki, MediaWiki};

pub const MW: &str = include_str!("../data/sample.wiki");
pub const DW: &str = include_str!("../data/sample.dokuwiki");

pub fn mw() -> MediaWiki {
    MediaWiki::default()
}

pub fn dw() -> DokuWiki {
    DokuWiki
}

pub fn extract(f: &dyn Filter, data: &[u8]) -> Result<Document, Error> {
    f.extract(data, &ExtractOptions::default())
}

/// The unit text the way LocOS shows it: codes as `\u{27E6}x1\u{27E7}` tokens.
pub fn token_text(u: &Unit) -> String {
    locos::to_locos_text(&u.source()).0
}

pub fn texts(f: &dyn Filter, data: &[u8]) -> Vec<String> {
    extract(f, data).expect("extract").units.iter().map(token_text).collect()
}

pub fn code<'a>(u: &'a Unit, n: &str) -> &'a CodeData {
    u.codes.get(n).unwrap_or_else(|| panic!("no code {n}"))
}

pub fn target_of(u: &Unit, text: &str) -> Vec<Inline> {
    let (_, map) = locos::to_locos_text(&u.source());
    locos::from_locos_text(text, &map)
}

/// Sets targets by unit index (LocOS token text) and merges.
pub fn build(f: &dyn Filter, data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    let mut doc = extract(f, data).expect("extract");
    for (i, t) in mapping {
        let u = &doc.units[*i];
        let id = u.segments[0].id.clone();
        let tg = target_of(u, t);
        doc.set_target(&id, tg).expect("set_target");
    }
    f.merge(&doc).expect("merge")
}

pub fn build_str(f: &dyn Filter, data: &[u8], mapping: &[(usize, &str)]) -> String {
    String::from_utf8(build(f, data, mapping)).expect("utf-8 output")
}

/// Every unit translated with `FR ` in front of its own token text.
pub fn prefixed(f: &dyn Filter, data: &[u8]) -> String {
    let t = texts(f, data);
    let m: Vec<(usize, String)> = t.iter().enumerate().map(|(i, s)| (i, format!("FR {s}"))).collect();
    let m2: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
    build_str(f, data, &m2)
}

/// An untouched merge and an echo merge both give back the original bytes.
pub fn assert_identity(f: &dyn Filter, data: &[u8]) {
    let doc = extract(f, data).unwrap();
    assert_eq!(f.merge(&doc).unwrap(), data, "untouched merge");
    let t = texts(f, data);
    let echo: Vec<(usize, &str)> = t.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    assert_eq!(build(f, data, &echo), data, "echo merge");
}

pub fn crlf(s: &str) -> String {
    s.replace('\n', "\r\n")
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/wiki")
}

/// The right filter for a file name: `.dokuwiki` is DokuWiki, everything else MediaWiki.
pub fn filter_for(name: &str) -> Box<dyn Filter> {
    if name.ends_with(".dokuwiki") {
        Box::new(DokuWiki)
    } else {
        Box::new(MediaWiki::default())
    }
}

/// Every corpus file: (name, bytes), sorted by name.
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
