#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_ttml::Ttml;

pub const F: Ttml = Ttml;

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_granularity(Granularity::Paragraph)
}

pub fn opts_for(lang: Option<&str>) -> ExtractOptions {
    ExtractOptions::new().with_target_lang(lang.map(String::from)).with_granularity(Granularity::Paragraph)
}

pub fn extract(data: &[u8]) -> Document {
    F.extract(data, &opts()).expect("extract")
}

/// The unit text as LocOS shows it: tags as one token each.
pub fn show(u: &Unit) -> String {
    to_locos_text(&u.source()).0
}

/// (caption index, LocOS text) for each unit, like the LocOS `p_index` and `text`.
pub fn units(data: &[u8]) -> Vec<(usize, String)> {
    extract(data).units.iter().map(|u| (index_of(u), show(u))).collect()
}

pub fn texts(data: &[u8]) -> Vec<String> {
    units(data).into_iter().map(|(_, t)| t).collect()
}

pub fn index_of(u: &Unit) -> usize {
    u.id.trim_start_matches('p').parse::<usize>().unwrap() - 1
}

/// Sets the target of the unit for caption `idx` from LocOS text (tokens back to tags).
pub fn set(d: &mut Document, idx: usize, text: &str) {
    let u = d.units.iter_mut().find(|u| index_of(u) == idx).expect("unit for caption");
    let (_, map) = to_locos_text(&u.source());
    u.segments[0].target = from_locos_text(text, &map);
}

/// LocOS `build` / `build_localized`: the mapping is caption index to LocOS text; with a locale the file is
/// stamped (always, as LocOS does) and written as UTF-8.
pub fn build(data: &[u8], mapping: &[(usize, &str)], locale: Option<&str>) -> Vec<u8> {
    let mut d = F.extract(data, &opts_for(locale)).expect("extract");
    for (i, t) in mapping {
        set(&mut d, *i, t);
    }
    F.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).expect("merge")
}

pub fn build_str(data: &[u8], mapping: &[(usize, &str)], locale: Option<&str>) -> String {
    String::from_utf8(build(data, mapping, locale)).expect("utf-8 output")
}

/// An untouched merge and a merge with every unit echoed back must both give the original bytes.
pub fn assert_identity(data: &[u8]) {
    let d = extract(data);
    assert_eq!(F.merge(&d).expect("merge"), data, "untouched merge");
    let mut e = d.clone();
    for u in &mut e.units {
        u.segments[0].target = u.segments[0].source.clone();
    }
    assert_eq!(F.merge(&e).expect("merge"), data, "echo merge");
}

pub fn well_formed(out: &[u8]) -> Document {
    F.extract(out, &opts()).expect("output is still a TTML file")
}
