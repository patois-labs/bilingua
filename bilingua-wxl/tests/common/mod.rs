#![allow(dead_code)]
//! Helpers shared by the test files.

use bilingua_model::*;
use bilingua_wxl::Wxl;

pub const LOCOS_PACKAGE: &[u8] = include_bytes!("../data/locos_package.wxl");
pub const V3_OVERRIDABLE: &[u8] = include_bytes!("../data/v3_overridable.wxl");
pub const EDGE: &[u8] = include_bytes!("../data/edge.wxl");
pub const LANGUAGES: &[u8] = include_bytes!("../data/languages.wxl");
pub const LANGUAGES_CRLF_BOM: &[u8] = include_bytes!("../data/languages_crlf_bom.wxl");
pub const LANGUAGES_UTF16LE: &[u8] = include_bytes!("../data/languages_utf16le.wxl");
pub const LANGUAGES_UTF16BE: &[u8] = include_bytes!("../data/languages_utf16be.wxl");
pub const LEGACY_1252: &[u8] = include_bytes!("../data/legacy_windows_1252.wxl");
pub const LEGACY_1251: &[u8] = include_bytes!("../data/legacy_windows_1251.wxl");
pub const LEGACY_SJIS: &[u8] = include_bytes!("../data/legacy_shift_jis.wxl");
pub const LEGACY_GBK: &[u8] = include_bytes!("../data/legacy_gbk.wxl");
pub const LEGACY_EUCKR: &[u8] = include_bytes!("../data/legacy_euc_kr.wxl");
pub const LEGACY_1256: &[u8] = include_bytes!("../data/legacy_windows_1256.wxl");

pub fn fixtures() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("locos_package", LOCOS_PACKAGE),
        ("edge", EDGE),
        ("v3_overridable", V3_OVERRIDABLE),
        ("languages", LANGUAGES),
        ("languages_crlf_bom", LANGUAGES_CRLF_BOM),
        ("languages_utf16le", LANGUAGES_UTF16LE),
        ("languages_utf16be", LANGUAGES_UTF16BE),
        ("legacy_1252", LEGACY_1252),
        ("legacy_1251", LEGACY_1251),
        ("legacy_sjis", LEGACY_SJIS),
        ("legacy_gbk", LEGACY_GBK),
        ("legacy_euckr", LEGACY_EUCKR),
        ("legacy_1256", LEGACY_1256),
    ]
}

pub fn opts(locked: bool) -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("de").with_include_locked(locked)
}

pub fn ex(bytes: &[u8]) -> Document {
    Wxl.extract(bytes, &opts(false)).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn ex_locked(bytes: &[u8]) -> Document {
    Wxl.extract(bytes, &opts(true)).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn try_ex(bytes: &[u8]) -> Result<Document, Error> {
    Wxl.extract(bytes, &opts(false))
}

/// Unit text with each tag shown as its display, the way LocOS shows a placeholder kept as plain text.
pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| flat(&u.source())).collect()
}

pub fn flat(v: &[Inline]) -> String {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

pub fn merged(d: &Document) -> Vec<u8> {
    Wxl.merge(d).unwrap_or_else(|e| panic!("merge failed: {e}"))
}

/// Sets every translatable unit's target to `f(source text)` (tags kept: the source tags are re-used).
pub fn translate_all(d: &mut Document, mut f: impl FnMut(&str) -> String) {
    for u in d.units.iter_mut().filter(|u| u.translate) {
        let t = f(&flat(&u.source()));
        u.segments[0].target = vec![Inline::text(t)];
        u.segments[0].state = SegState::Draft;
    }
}

pub fn pseudo(d: &mut Document) {
    translate_all(d, |s| format!("\u{ab}{s}\u{bb}"));
}
