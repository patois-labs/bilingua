//! Shared helpers: units as LocOS writes them (inline codes as numbered tokens) and builds from LocOS-style text.
#![allow(dead_code)]
use bilingua_model::locos::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_rst::Rst;

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph)
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    Rst.extract(data, &opts())
}

/// The unit texts LocOS would show: one token per inline code.
pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(|u| to_locos_text(&u.source()).0).collect()
}

pub fn styles(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(|u| u.context.style.clone().unwrap_or_default()).collect()
}

/// Builds with LocOS-style translated texts for the given unit indexes (tokens resolved against the source).
pub fn try_build(data: &[u8], tr: &[(usize, String)]) -> Result<Vec<u8>, Error> {
    let mut doc = extract(data)?;
    for (i, t) in tr {
        let u = &mut doc.units[*i];
        let (_, map) = to_locos_text(&u.source());
        u.segments[0].target = from_locos_text(t, &map);
        u.segments[0].state = SegState::Draft;
    }
    Rst.merge(&doc)
}

pub fn build(data: &[u8], tr: &[(usize, &str)]) -> Vec<u8> {
    let v: Vec<(usize, String)> = tr.iter().map(|(i, s)| (*i, s.to_string())).collect();
    try_build(data, &v).expect("merge")
}

pub fn build_str(data: &[u8], tr: &[(usize, &str)]) -> String {
    String::from_utf8(build(data, tr)).expect("utf-8 output")
}

/// An empty build and an echo build are both the original bytes.
pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).expect("extract");
    assert_eq!(Rst.merge(&doc).unwrap(), data, "empty build must be byte-identical");
    let texts = units(data);
    let echo: Vec<(usize, String)> = texts.iter().cloned().enumerate().collect();
    assert_eq!(try_build(data, &echo).unwrap(), data, "echo build must be byte-identical");
}

/// Like `try_build`, but each target is the parsed text exactly (no tag-count check), as LocOS accepts it.
pub fn build_raw(data: &[u8], tr: &[(usize, String)]) -> Result<Vec<u8>, Error> {
    try_build(data, tr)
}
