#![allow(dead_code)]
//! Helpers shared by the test files.

use bilingua_desktop::{CatkeysFilter, DesktopFilter};
use bilingua_model::*;

pub const LOCOS_DESKTOP: &[u8] = include_bytes!("../data/locos_example.desktop");
pub const LOCOS_CATKEYS: &[u8] = include_bytes!("../data/locos_example.catkeys");

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang(target.map(str::to_string))
}

pub fn flat(v: &[Inline]) -> String {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

pub fn desktop(bytes: &[u8]) -> Result<Document, Error> {
    DesktopFilter.extract(bytes, &opts(None))
}

pub fn catkeys(bytes: &[u8]) -> Result<Document, Error> {
    CatkeysFilter.extract(bytes, &opts(None))
}

pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| flat(&u.source())).collect()
}

pub fn dtexts(bytes: &[u8]) -> Vec<String> {
    texts(&desktop(bytes).unwrap())
}

pub fn ctexts(bytes: &[u8]) -> Vec<String> {
    texts(&catkeys(bytes).unwrap())
}

pub fn merge(d: &Document) -> Vec<u8> {
    if d.format == "desktop" { DesktopFilter.merge(d) } else { CatkeysFilter.merge(d) }.unwrap_or_else(|e| panic!("merge failed: {e}"))
}

pub fn set(d: &mut Document, n: usize, to: &str) {
    let u = &mut d.units[n];
    u.segments[0].target = vec![Inline::text(to)];
    u.segments[0].state = SegState::Draft;
}

/// Extract, translate the listed units, merge: the LocOS `build` (no target) or `build_localized` (target).
pub fn build_d(bytes: &[u8], tr: &[(usize, &str)], target: Option<&str>) -> Vec<u8> {
    let mut d = DesktopFilter.extract(bytes, &opts(target)).unwrap();
    for (i, t) in tr {
        set(&mut d, *i, t);
    }
    merge(&d)
}

pub fn build_c(bytes: &[u8], tr: &[(usize, &str)], target: Option<&str>) -> Vec<u8> {
    let mut d = CatkeysFilter.extract(bytes, &opts(target)).unwrap();
    for (i, t) in tr {
        set(&mut d, *i, t);
    }
    merge(&d)
}

pub fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

pub fn pseudo(d: &mut Document) {
    for i in 0..d.units.len() {
        if d.units[i].translate {
            let t = format!("\u{ab}{}\u{bb}", flat(&d.units[i].source()));
            set(d, i, &t);
        }
    }
}

pub fn fixtures() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "desktop" || x == "catkeys"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()))
        .collect();
    v.sort();
    v
}

pub fn extract_any(name: &str, bytes: &[u8]) -> Result<Document, Error> {
    if name.ends_with(".desktop") { desktop(bytes) } else { catkeys(bytes) }
}

pub fn is_typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_))
}
