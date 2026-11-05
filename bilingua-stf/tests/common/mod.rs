#![allow(dead_code)]
use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_stf::StfFilter;

pub const T: &str = "\t";

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    StfFilter.extract(data, &opts())
}

pub fn extract_for(data: &[u8], target: Option<&str>) -> Result<Document, Error> {
    StfFilter.extract(data, &opts().with_target_lang(target.map(String::from)))
}

/// The unit texts as LocOS makes them (`\u{27E6}x1\u{27E7}` for a placeholder).
pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(|u| to_locos_text(&u.source()).0).collect()
}

/// LocOS `build` / `build_localized` through Bilingua: `mapping` holds LocOS-token texts by unit index (as the
/// adapter maps them), `target` the target locale (None = `build`), `merge` the options.
pub fn build_with(data: &[u8], mapping: &[(usize, String)], target: Option<&str>, merge: &MergeOptions) -> Vec<u8> {
    build_opts(data, mapping, &opts().with_target_lang(target.map(String::from)), merge)
}

/// The options the LocOS adapter extracts with: no targets loaded.
pub fn locos_opts() -> ExtractOptions {
    opts().with_format_options([(bilingua_stf::LOAD_TARGETS.to_string(), "false".to_string())].into())
}

/// `build_with` with explicit extract options.
pub fn build_opts(data: &[u8], mapping: &[(usize, String)], eo: &ExtractOptions, merge: &MergeOptions) -> Vec<u8> {
    let mut doc = StfFilter.extract(data, eo).expect("extract");
    for (i, text) in mapping {
        let (_, map) = to_locos_text(&doc.units[*i].source());
        let id = doc.units[*i].segments[0].id.clone();
        doc.set_target(&id, from_locos_text(text, &map)).expect("set_target");
    }
    StfFilter.merge_with(&doc, merge).expect("merge")
}

/// LocOS behaviour: UTF-8 out, stamp always when a target is given.
pub fn build(data: &[u8], mapping: &[(usize, &str)], target: Option<&str>) -> Vec<u8> {
    let m: Vec<(usize, String)> = mapping.iter().map(|(i, t)| (*i, t.to_string())).collect();
    build_with(data, &m, target, &bilingua_model::profile::locos().merge_options("stf"))
}

pub fn all_with(data: &[u8], f: impl Fn(&str) -> String) -> Vec<(usize, String)> {
    units(data).iter().enumerate().map(|(i, t)| (i, f(t))).collect()
}

pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert_eq!(StfFilter.merge(&doc).unwrap(), data, "untouched merge");
    let echo = all_with(data, |t| t.to_string());
    assert_eq!(build_with(data, &echo, None, &MergeOptions::default()), data, "echo merge");
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/stf")
}

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
