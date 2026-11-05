//! Shared helpers: units as LocOS writes them (inline codes as numbered tokens) and builds from LocOS-style text,
//! the way the Localization OS adapter calls the filter (UTF-8 out, unresolved tokens dropped).
#![allow(dead_code)]
use bilingua_model::locos::{parse_locos_text, to_locos_text, TokenProblem};
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
    ExtractOptions { source_lang: Some("en".into()), granularity: Granularity::Paragraph, ..Default::default() }
}

pub fn extract(fmt: &str, data: &[u8]) -> Result<Document, Error> {
    filter(fmt).extract(data, &opts())
}

pub fn units(fmt: &str, data: &[u8]) -> Vec<String> {
    extract(fmt, data).expect("extract").units.iter().map(|u| to_locos_text(&u.source()).0).collect()
}

/// LocOS text to a target: unknown tokens (an invented `x9`) are dropped, as LocOS drops them.
pub fn target(u: &Unit, t: &str) -> Vec<Inline> {
    let (_, map) = to_locos_text(&u.source());
    let (_, problems) = parse_locos_text(t, &map);
    let mut t = t.to_string();
    for p in problems {
        if let TokenProblem::Unknown { token } | TokenProblem::KindMismatch { token } = p {
            t = t.replace(&token, "");
        }
    }
    parse_locos_text(&t, &map).0
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
    try_build_with(fmt, data, tr, &MergeOptions::locos())
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
