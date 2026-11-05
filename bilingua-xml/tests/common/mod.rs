#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_model::locos::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_xml::XmlFilter;

pub fn data(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions::new().with_target_lang(target.map(String::from)).with_granularity(Granularity::Paragraph)
}

pub fn ex(f: &XmlFilter, d: &[u8]) -> Document {
    f.extract(d, &opts(None)).expect("extract")
}

pub fn ex_to(f: &XmlFilter, d: &[u8], target: &str) -> Document {
    f.extract(d, &opts(Some(target))).expect("extract")
}

/// A unit's source as LocOS shows it: tags as `⟦g1⟧` tokens.
pub fn show(u: &Unit) -> String {
    to_locos_text(&u.source()).0
}

pub fn texts(f: &XmlFilter, d: &[u8]) -> Vec<String> {
    ex(f, d).units.iter().map(show).collect()
}

/// Sets unit `i`'s target from LocOS token text.
pub fn set(doc: &mut Document, i: usize, text: &str) {
    let (_, map) = to_locos_text(&doc.units[i].source());
    let inl = from_locos_text(text, &map);
    let id = doc.units[i].segments[0].id.clone();
    doc.set_target(&id, inl).expect("set_target");
}

/// Sets inlines without the tag checks (to simulate a damaged translation).
pub fn set_raw(doc: &mut Document, i: usize, inl: Vec<Inline>) {
    doc.units[i].segments[0].target = inl;
}

/// LocOS `_fr`: tokens stay, words get an accent on `a`, the whole gets a `FR:` prefix.
pub fn fr(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(p) = rest.find('\u{27E6}') {
        let chunk = &rest[..p];
        out.push_str(&if chunk.trim().is_empty() { chunk.to_string() } else { chunk.replace('a', "\u{e0}") });
        let e = rest[p..].find('\u{27E7}').map_or(rest.len(), |x| p + x + 3);
        out.push_str(&rest[p..e]);
        rest = &rest[e..];
    }
    out.push_str(&rest.replace('a', "\u{e0}"));
    format!("FR:{out}")
}

pub fn translate_all(f: &XmlFilter, d: &[u8], locale: Option<&str>) -> Vec<u8> {
    let mut doc = match locale {
        Some(l) => ex_to(f, d, l),
        None => ex(f, d),
    };
    for i in 0..doc.units.len() {
        let t = fr(&show(&doc.units[i]));
        set(&mut doc, i, &t);
    }
    f.merge(&doc).expect("merge")
}

pub fn merge_with(f: &XmlFilter, d: &[u8], target: Option<&str>, map: &[(usize, &str)]) -> Vec<u8> {
    let mut doc = match target {
        Some(l) => ex_to(f, d, l),
        None => ex(f, d),
    };
    for (i, t) in map {
        set(&mut doc, *i, t);
    }
    f.merge(&doc).expect("merge")
}

pub fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).expect("utf-8 output")
}

/// Every unit echoed back must give the original bytes.
pub fn echo(f: &XmlFilter, d: &[u8], target: Option<&str>) -> Vec<u8> {
    let mut doc = match target {
        Some(l) => ex_to(f, d, l),
        None => ex(f, d),
    };
    for i in 0..doc.units.len() {
        let t = show(&doc.units[i]);
        set(&mut doc, i, &t);
    }
    f.merge(&doc).expect("merge")
}

/// Parses with the crate itself: the output must read back as a well-formed document.
pub fn well_formed(f: &XmlFilter, out: &[u8]) {
    f.extract(out, &opts(None)).unwrap_or_else(|e| panic!("output is not well-formed: {e}\n{}", String::from_utf8_lossy(out)));
}
