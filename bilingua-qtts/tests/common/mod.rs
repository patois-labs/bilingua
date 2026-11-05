#![allow(dead_code)]
//! Helpers shared by the test files.

use bilingua_model::*;
use bilingua_qtts::QtTs;
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data")
}

/// A fixture from `tests/data` (or `okapi/NAME`).
pub fn data(name: &str) -> Vec<u8> {
    std::fs::read(data_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Every `.ts` file in `tests/data`, name and bytes.
pub fn all_data() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(data_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".ts"))
        .map(|e| (e.file_name().to_string_lossy().to_string(), std::fs::read(e.path()).unwrap()))
        .collect();
    v.sort();
    assert!(v.len() >= 25, "fixtures went missing");
    v
}

/// The Okapi test resources for okf_ts.
pub fn okapi_data() -> Vec<(String, Vec<u8>)> {
    let dir = data_dir().join("okapi");
    let mut v: Vec<(String, Vec<u8>)> =
        std::fs::read_dir(dir).unwrap().flatten().map(|e| (e.file_name().to_string_lossy().to_string(), std::fs::read(e.path()).unwrap())).collect();
    v.sort();
    v
}

/// The fixture as the LocOS suite reads it: LF on every checkout.
pub fn lf(name: &str) -> Vec<u8> {
    let s = data(name);
    String::from_utf8(s).unwrap().replace("\r\n", "\n").into_bytes()
}

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions { source_lang: Some("en".into()), target_lang: target.map(str::to_string), ..Default::default() }
}

pub fn ex(bytes: &[u8]) -> Document {
    QtTs.extract(bytes, &opts(None)).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn ex_to(bytes: &[u8], target: &str) -> Document {
    QtTs.extract(bytes, &opts(Some(target))).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn ex_locked(bytes: &[u8]) -> Document {
    QtTs.extract(bytes, &ExtractOptions { include_locked: true, ..opts(None) }).unwrap()
}

pub fn try_ex(bytes: &[u8]) -> Result<Document, Error> {
    QtTs.extract(bytes, &opts(None))
}

pub fn merged(d: &Document) -> Vec<u8> {
    QtTs.merge(d).unwrap_or_else(|e| panic!("merge failed: {e}"))
}

/// A unit's source in the Localization OS token form (`⟦x1⟧` for a tag).
pub fn token_text(u: &Unit) -> String {
    locos::to_locos_text(&u.source()).0
}

pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(token_text).collect()
}

/// Sets a unit's target to plain text.
pub fn set(d: &mut Document, unit: usize, text: &str) {
    d.units[unit].segments[0].target = vec![Inline::text(text)];
    d.units[unit].segments[0].state = SegState::Draft;
}

/// Sets every translatable unit's target to `f(source)` with the source's tags kept in place: the source's inline
/// list is mapped text piece by text piece.
pub fn translate_all(d: &mut Document, mut f: impl FnMut(&str) -> String) {
    for u in d.units.iter_mut().filter(|u| u.translate) {
        let t: Vec<Inline> = u
            .source()
            .into_iter()
            .map(|i| match i {
                Inline::Text(t) => Inline::Text(f(&t)),
                tag => tag,
            })
            .collect();
        u.segments[0].target = t;
        u.segments[0].state = SegState::Draft;
    }
}

pub fn pseudo(d: &mut Document) {
    translate_all(d, |s| format!("\u{ab}{s}\u{bb}"));
}

pub fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).expect("utf-8 output")
}

/// The merged bytes read back as text through the engine's own decoder.
pub fn decoded(b: &[u8]) -> String {
    enc::decode_text(b, &enc::DecodeOptions { declared: true, source_lang: Some("en"), limits: Limits::default() }).unwrap().text
}

pub fn parse_msg(r: Result<Document, Error>) -> String {
    match r {
        Err(e) => e.to_string(),
        Ok(_) => panic!("must be refused"),
    }
}
