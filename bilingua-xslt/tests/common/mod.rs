#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_model::*;
use bilingua_xslt::Xslt;

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_granularity(Granularity::Paragraph)
}

pub fn extract(data: &[u8]) -> Document {
    Xslt.extract(data, &opts()).expect("extract")
}

/// A unit's source as LocOS shows it: tags as `\u{27E6}g1\u{27E7}` tokens.
pub fn locos(u: &Unit) -> String {
    locos::to_locos_text(&u.source()).0
}

pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).units.iter().map(locos).collect()
}

pub fn styles(data: &[u8]) -> Vec<String> {
    extract(data).units.iter().map(|u| u.context.style.clone().unwrap_or_default()).collect()
}

/// Sets the target of unit `i` from LocOS token text.
pub fn set_locos(d: &mut Document, i: usize, text: &str) {
    let (_, map) = locos::to_locos_text(&d.units[i].source());
    let inl = locos::from_locos_text(text, &map);
    d.units[i].segments[0].target = inl;
}

pub fn translate(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    let mut d = extract(data);
    for (i, t) in mapping {
        set_locos(&mut d, *i, t);
    }
    Xslt.merge(&d).expect("merge")
}

/// Every unit translated through `f` (applied to its LocOS text).
pub fn translate_all(data: &[u8], f: impl Fn(&str) -> String) -> Vec<u8> {
    let mut d = extract(data);
    for i in 0..d.units.len() {
        let t = f(&locos(&d.units[i]));
        set_locos(&mut d, i, &t);
    }
    Xslt.merge(&d).expect("merge")
}

/// An untouched merge and a merge with every unit echoed back must both give the original bytes.
pub fn assert_identity(data: &[u8]) {
    let d = extract(data);
    assert_eq!(Xslt.merge(&d).expect("merge"), data, "untouched merge");
    assert!(translate_all(data, |t| t.to_string()) == data, "echo merge");
}

pub fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

/// The output must read back as XSLT with the same number of units as `n`.
pub fn well_formed(out: &[u8]) -> Document {
    Xslt.extract(out, &opts()).expect("output is still well-formed XSLT")
}
