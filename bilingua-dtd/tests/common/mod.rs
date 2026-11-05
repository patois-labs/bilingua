#![allow(dead_code)]
use bilingua_dtd::DtdFilter;
use bilingua_model::*;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    DtdFilter.extract(data, &opts())
}

/// A segment with every tag written as its display: what LocOS shows as the unit text.
pub fn flat(v: &[Inline]) -> String {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) => t.as_str(),
            Inline::Tag { display, .. } => display.as_str(),
        })
        .collect()
}

pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(|u| flat(&u.source())).collect()
}

/// Sets plain-text targets by unit index (as the LocOS mapping does) and merges.
pub fn build(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    let mut doc = extract(data).expect("extract");
    for (i, t) in mapping {
        let id = doc.units[*i].segments[0].id.clone();
        doc.set_target(&id, vec![Inline::text(*t)]).expect("set_target");
    }
    DtdFilter.merge(&doc).expect("merge")
}

/// Untouched merge and an echo merge (every unit's own text as target) both give back the original bytes.
pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert_eq!(DtdFilter.merge(&doc).unwrap(), data, "untouched merge");
    let texts = units(data);
    let echo: Vec<(usize, &str)> = texts.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
    assert_eq!(build(data, &echo), data, "echo merge");
    // the same with the real tags as the target
    let mut doc = doc;
    for u in doc.units.iter_mut().filter(|u| u.translate) {
        u.segments[0].target = u.segments[0].source.clone();
    }
    assert_eq!(DtdFilter.merge(&doc).unwrap(), data, "tag echo merge");
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/dtd")
}

pub fn data_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}
