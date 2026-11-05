#![allow(dead_code)]
//! Helpers shared by the test files.

use bilingua_model::*;
use bilingua_resjson::{Limits, Resjson};

pub const LOCOS_SUITE: &[u8] = include_bytes!("../data/locos_suite.resjson");
pub const VS_TEMPLATE: &[u8] = include_bytes!("../data/vs_template.resjson");
pub const LANGUAGES: &[u8] = include_bytes!("../data/languages.resjson");
pub const LANGUAGES_CRLF_BOM: &[u8] = include_bytes!("../data/languages_crlf_bom.resjson");
pub const LANGUAGES_UTF16LE: &[u8] = include_bytes!("../data/languages_utf16le.resjson");
pub const LANGUAGES_UTF16BE: &[u8] = include_bytes!("../data/languages_utf16be.resjson");

pub fn fixtures() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("locos_suite", LOCOS_SUITE),
        ("vs_template", VS_TEMPLATE),
        ("languages", LANGUAGES),
        ("languages_crlf_bom", LANGUAGES_CRLF_BOM),
        ("languages_utf16le", LANGUAGES_UTF16LE),
        ("languages_utf16be", LANGUAGES_UTF16BE),
    ]
}

pub fn opts(locked: bool) -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("de").with_include_locked(locked)
}

pub fn ex(bytes: &[u8]) -> Document {
    Resjson.extract(bytes, &opts(false)).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn ex_locked(bytes: &[u8]) -> Document {
    Resjson.extract(bytes, &opts(true)).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn try_ex(bytes: &[u8]) -> Result<Document, Error> {
    Resjson.extract(bytes, &opts(false))
}

pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| plain_text(&u.source())).collect()
}

pub fn merged(d: &Document) -> Vec<u8> {
    Resjson.merge(d).unwrap_or_else(|e| panic!("merge failed: {e}"))
}

/// Sets every translatable unit's target to `f(source text)` (written directly: `set_target` searches the
/// whole document, which is slow for 40 000 units).
pub fn translate_all(d: &mut Document, mut f: impl FnMut(&str) -> String) {
    for u in d.units.iter_mut().filter(|u| u.translate) {
        let t = f(&plain_text(&u.source()));
        u.segments[0].target = vec![Inline::text(t)];
        u.segments[0].state = SegState::Draft;
    }
}

pub fn pseudo(d: &mut Document) {
    translate_all(d, |s| format!("\u{ab}{s}\u{bb}"));
}

pub fn small_limits(max_units: usize) -> Limits {
    Limits { max_units: max_units, ..Limits::default() }
}

pub fn parse_msg(r: Result<Document, Error>) -> String {
    match r {
        Err(e) => e.to_string(),
        Ok(_) => panic!("must be refused"),
    }
}
