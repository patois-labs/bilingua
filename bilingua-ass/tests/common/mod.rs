#![allow(dead_code)]
//! Helpers shared by the test files: the LocOS view of a document (unit texts with `\u{27E6}..\u{27E7}` tokens, builds
//! from a `{index: text}` mapping) on top of the Bilingua model.
use bilingua_ass::AssFilter;
use bilingua_model::profile::tokens::{from_locos_text, parse_locos_text, to_locos_text, TokenProblem};
use bilingua_model::*;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    AssFilter.extract(data, &opts())
}

pub fn extract_with(data: &[u8], o: &ExtractOptions) -> Result<Document, Error> {
    AssFilter.extract(data, o)
}

/// The units as LocOS prints them (inline codes as tokens).
pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(|u| to_locos_text(&u.source()).0).collect()
}

/// A translation written the LocOS way, as inline codes: tokens that name no code of the unit are dropped, as the
/// LocOS build drops them.
pub fn inlines_for(u: &Unit, locos_text: &str) -> Vec<Inline> {
    let (_src, map) = to_locos_text(&u.source());
    let mut text = locos_text.to_string();
    let (_, problems) = parse_locos_text(&text, &map);
    for p in problems {
        match p {
            TokenProblem::Unknown { token } | TokenProblem::KindMismatch { token } => text = text.replace(&token, ""),
            TokenProblem::Stray { .. } => {}
        }
    }
    from_locos_text(&text, &map)
}

/// Sets targets by unit index (as the LocOS mapping does) and merges with the default options.
pub fn build(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    build_with(data, mapping, None, &MergeOptions::default())
}

/// LocOS `build`: UTF-8 out.
pub fn build_locos(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    build_with(data, mapping, None, &bilingua_model::profile::locos().merge_options("xml"))
}

/// LocOS `build_localized`.
pub fn build_localized(data: &[u8], mapping: &[(usize, &str)], target: Option<&str>) -> Vec<u8> {
    build_with(data, mapping, target, &bilingua_model::profile::locos().merge_options("xml"))
}

pub fn build_with(data: &[u8], mapping: &[(usize, &str)], target: Option<&str>, mo: &MergeOptions) -> Vec<u8> {
    let mut doc = extract(data).expect("extract");
    doc.target_lang = target.map(str::to_string);
    for (i, t) in mapping {
        let inl = inlines_for(&doc.units[*i], t);
        let id = doc.units[*i].segments[0].id.clone();
        doc.set_target(&id, inl).expect("set_target");
    }
    AssFilter.merge_with(&doc, mo).expect("merge")
}

pub fn s(b: Vec<u8>) -> String {
    String::from_utf8(b).expect("utf-8")
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/ass")
}

/// Untouched merge and an echo merge (every unit's own inlines as target) both give back the original bytes.
pub fn assert_identity(data: &[u8]) {
    let mut doc = extract(data).unwrap();
    assert_eq!(AssFilter.merge(&doc).unwrap(), data, "untouched merge");
    let items: Vec<(String, Vec<Inline>)> = doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), u.source())).collect();
    doc.set_targets(items).unwrap();
    assert_eq!(AssFilter.merge(&doc).unwrap(), data, "echo merge");
}
