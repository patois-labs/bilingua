//! Shared helpers: units as LocOS writes them (inline codes as numbered tokens) and builds from LocOS-style text,
//! the way the Localization OS adapter calls the filter (UTF-8 out, a token that names no code stays text).
#![allow(dead_code)]
use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_subtext::{Sbv, Sub};

pub fn filter(fmt: &str) -> &'static dyn Filter {
    match fmt {
        "sbv" => &Sbv,
        "sub" => &Sub,
        _ => unreachable!(),
    }
}

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph)
}

pub fn extract(fmt: &str, data: &[u8]) -> Result<Document, Error> {
    filter(fmt).extract(data, &opts())
}

pub fn units(fmt: &str, data: &[u8]) -> Vec<String> {
    extract(fmt, data).expect("extract").units.iter().map(|u| to_locos_text(&u.source()).0).collect()
}

/// LocOS text to a target: a token that names no code of the unit (an invented `x9`) stays text, as the LocOS build
/// keeps it (issue 814).
pub fn target(u: &Unit, t: &str) -> Vec<Inline> {
    let (_, map) = to_locos_text(&u.source());
    from_locos_text(t, &map)
}

pub fn try_build_with(fmt: &str, data: &[u8], tr: &[(usize, String)], mo: &MergeOptions) -> Result<Vec<u8>, Error> {
    let mut doc = extract(fmt, data)?;
    for (i, t) in tr {
        let tg = target(&doc.units[*i], t);
        let u = &mut doc.units[*i];
        u.segments[0].target = tg;
        u.segments[0].state = SegState::Draft;
    }
    filter(fmt).merge_with(&doc, mo)
}

/// The adapter's way: UTF-8 out.
pub fn try_build(fmt: &str, data: &[u8], tr: &[(usize, String)]) -> Result<Vec<u8>, Error> {
    try_build_with(fmt, data, tr, &bilingua_model::profile::locos().merge_options("xml"))
}

pub fn build(fmt: &str, data: &[u8], tr: &[(usize, &str)]) -> Vec<u8> {
    let v: Vec<(usize, String)> = tr.iter().map(|(i, s)| (*i, s.to_string())).collect();
    try_build(fmt, data, &v).expect("merge")
}

/// An empty build and an echo build are both the original bytes.
pub fn assert_identity(fmt: &str, data: &[u8]) {
    let doc = extract(fmt, data).expect("extract");
    assert_eq!(filter(fmt).merge(&doc).unwrap(), data, "empty build must be byte-identical");
    let echo: Vec<(usize, String)> = units(fmt, data).into_iter().enumerate().collect();
    assert_eq!(try_build_with(fmt, data, &echo, &MergeOptions::default()).unwrap(), data, "echo build must be byte-identical");
}
