#![allow(dead_code)]
use bilingua_adoc::Adoc;
use bilingua_model::*;

pub fn crlf(s: &str) -> String {
    s.replace('\n', "\r\n")
}

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    Adoc.extract(data, &opts())
}

/// The unit text the way LocOS shows it: codes as `\u{27E6}x1\u{27E7}` tokens.
pub fn token_text(u: &Unit) -> String {
    locos::to_locos_text(&u.source()).0
}

pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(token_text).collect()
}

/// A translation written in LocOS token text, turned back into inlines for unit `u`.
pub fn target_of(u: &Unit, text: &str) -> Vec<Inline> {
    let (_, map) = locos::to_locos_text(&u.source());
    locos::from_locos_text(text, &map)
}

/// Sets targets by unit index (LocOS mapping, token text) and merges, with an optional target language.
pub fn build_for(data: &[u8], mapping: &[(usize, &str)], target: Option<&str>) -> Vec<u8> {
    let mut doc = extract(data).expect("extract");
    doc.target_lang = target.map(str::to_string);
    for (i, t) in mapping {
        let u = &doc.units[*i];
        let id = u.segments[0].id.clone();
        let tg = target_of(u, t);
        doc.set_target(&id, tg).expect("set_target");
    }
    Adoc.merge(&doc).expect("merge")
}

pub fn build(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    build_for(data, mapping, None)
}

pub fn build_str(data: &[u8], mapping: &[(usize, &str)]) -> String {
    String::from_utf8(build(data, mapping)).expect("utf-8 output")
}

/// Untouched merge and an echo merge both give back the original bytes.
pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert_eq!(Adoc.merge(&doc).unwrap(), data, "untouched merge");
    let texts = units(data);
    let echo: Vec<(usize, &str)> = texts.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
    assert_eq!(build(data, &echo), data, "echo merge");
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/adoc")
}
