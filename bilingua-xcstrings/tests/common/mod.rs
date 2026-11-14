#![allow(dead_code)]
//! Helpers shared by the test files.
use bilingua_model::*;
use bilingua_xcstrings::{Xcloc, Xcstrings};

pub const LOCOS_CATALOG: &[u8] = include_bytes!("../data/locos_Localizable.xcstrings");

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang(target.map(str::to_string))
}

pub fn ex(bytes: &[u8], target: Option<&str>) -> Document {
    Xcstrings.extract(bytes, &opts(target)).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

/// A unit's text the way LocOS shows it: each placeholder as its LocOS token.
pub fn locos_text(u: &Unit) -> String {
    profile::tokens::to_locos_text(&u.source()).0
}

pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().filter(|u| u.translate).map(locos_text).collect()
}

/// LocOS's `build_localized(data, {index: text}, locale)`: sets the targets (text with LocOS tokens) by unit
/// number and merges.
pub fn build(bytes: &[u8], tr: &[(usize, &str)], locale: Option<&str>) -> Vec<u8> {
    let mut d = ex(bytes, locale);
    set_all(&mut d, tr);
    Xcstrings.merge(&d).unwrap_or_else(|e| panic!("merge failed: {e}"))
}

pub fn set_all(d: &mut Document, tr: &[(usize, &str)]) {
    for (i, t) in tr {
        let u = d.units.iter_mut().find(|u| u.id == format!("u{}", i + 1)).expect("unit");
        let (_, map) = profile::tokens::to_locos_text(&u.source());
        u.segments[0].target = profile::tokens::from_locos_text(t, &map);
        u.segments[0].state = SegState::Draft;
    }
}

/// Every unit translated as `f(LocOS text)`.
pub fn translate_all(d: &mut Document, f: impl Fn(&str) -> String) {
    for u in d.units.iter_mut().filter(|u| u.translate) {
        let (text, map) = profile::tokens::to_locos_text(&u.source());
        u.segments[0].target = profile::tokens::from_locos_text(&f(&text), &map);
        u.segments[0].state = SegState::Draft;
    }
}

pub fn json(b: &[u8]) -> serde_json::Value {
    serde_json::from_slice(b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(b)).unwrap_or_else(|e| panic!("output is not JSON: {e}"))
}

pub fn merge_xcloc(d: &Document) -> Vec<u8> {
    Xcloc.merge(d).unwrap_or_else(|e| panic!("xcloc merge failed: {e}"))
}
