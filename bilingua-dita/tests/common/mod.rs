#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;

pub use bilingua_dita::{Dita, DocBook};

pub fn data(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The made-up corpus in `oracle/corpus/dita` (name, bytes), sorted.
pub fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/dita");
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| (e.file_name().to_string_lossy().to_string(), std::fs::read(e.path()).unwrap())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

pub fn corpus_file(name: &str) -> Vec<u8> {
    corpus().into_iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name} missing from the corpus")).1
}

/// The filter LocOS picks for this file name.
pub fn filter_for(name: &str) -> Box<dyn Filter> {
    let ext = name.rsplit('.').next().unwrap_or_default();
    if matches!(ext, "dbk" | "docbook") {
        Box::new(DocBook::new())
    } else {
        Box::new(Dita::new())
    }
}

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions::new().with_target_lang(target.map(String::from)).with_granularity(Granularity::Paragraph)
}

pub fn ex(f: &dyn Filter, d: &[u8]) -> Document {
    f.extract(d, &opts(None)).expect("extract")
}

pub fn ex_to(f: &dyn Filter, d: &[u8], target: &str) -> Document {
    f.extract(d, &opts(Some(target))).expect("extract")
}

/// A unit's source as LocOS shows it: tags as `⟦g1⟧` tokens.
pub fn show(u: &Unit) -> String {
    to_locos_text(&u.source()).0
}

pub fn texts(f: &dyn Filter, d: &[u8]) -> Vec<String> {
    ex(f, d).units.iter().map(show).collect()
}

/// Sets unit `i`'s target from LocOS token text (tokens the unit does not have are dropped, as LocOS does).
pub fn set(doc: &mut Document, i: usize, text: &str) {
    let (_, map) = to_locos_text(&doc.units[i].source());
    doc.units[i].segments[0].target = from_locos_text(text, &map);
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

pub fn translate_all(f: &dyn Filter, d: &[u8], locale: Option<&str>) -> Vec<u8> {
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

pub fn merge_map(f: &dyn Filter, d: &[u8], target: Option<&str>, map: &[(usize, &str)]) -> Vec<u8> {
    let mut doc = match target {
        Some(l) => ex_to(f, d, l),
        None => ex(f, d),
    };
    for (i, t) in map {
        set(&mut doc, *i, t);
    }
    f.merge(&doc).expect("merge")
}

/// Every unit echoed back.
pub fn echo(f: &dyn Filter, d: &[u8], target: Option<&str>) -> Vec<u8> {
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

/// `[` + source + `]` on every unit.
pub fn pseudo(doc: &mut Document) {
    for u in &mut doc.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
}

pub fn tag_count(d: &Document) -> usize {
    d.units.iter().map(|u| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

pub fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).expect("utf-8 output")
}

/// The output must read back with the same filter.
pub fn well_formed(f: &dyn Filter, out: &[u8]) {
    f.extract(out, &opts(None)).unwrap_or_else(|e| panic!("output is not well-formed: {e}\n{}", String::from_utf8_lossy(out)));
}
