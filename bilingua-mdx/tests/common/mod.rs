#![allow(dead_code)]
use bilingua_mdx::{Mdx, MdxOptions};
use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    Mdx.extract(data, &opts())
}

pub fn extract_with(data: &[u8], m: &MdxOptions) -> Result<Document, Error> {
    Mdx.extract_with(data, &opts(), m)
}

/// The unit text the way LocOS shows it: one token per inline code.
pub fn locos_text(u: &Unit) -> String {
    to_locos_text(&u.source()).0
}

pub fn texts(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(locos_text).collect()
}

pub fn texts_str(src: &str) -> Vec<String> {
    texts(src.as_bytes())
}

/// Sets targets by unit index from LocOS-style text (the LocOS adapter: tokens back to the unit's own codes, an
/// unknown token stays text) and merges. No tag check, so invented and duplicated tokens reach the writer.
pub fn build_doc(doc: &mut Document, mapping: &[(usize, &str)]) {
    for (k, t) in mapping {
        let map = to_locos_text(&doc.units[*k].source()).1;
        let inl = from_locos_text(t, &map);
        let seg = &mut doc.units[*k].segments[0];
        seg.target = inl;
        seg.state = SegState::Draft;
    }
}

pub fn build(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    let mut doc = extract(data).expect("extract");
    build_doc(&mut doc, mapping);
    Mdx.merge(&doc).expect("merge")
}

pub fn build_str(src: &str, mapping: &[(usize, &str)]) -> String {
    String::from_utf8(build(src.as_bytes(), mapping)).unwrap()
}

/// Every unit translated by `f(text)`.
pub fn build_all(data: &[u8], f: impl Fn(&str) -> String) -> Vec<u8> {
    let t = texts(data);
    let owned: Vec<String> = t.iter().map(|s| f(s)).collect();
    let m: Vec<(usize, &str)> = owned.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    build(data, &m)
}

/// Untouched merge and an echo merge both give back the original bytes.
pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert_eq!(Mdx.merge(&doc).unwrap(), data, "untouched merge");
    assert_eq!(build_all(data, |t| t.to_string()), data, "echo merge");
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn guide() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/guide.mdx")).unwrap()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/mdx")
}
