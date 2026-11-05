#![allow(dead_code)]
use bilingua_model::*;
use bilingua_yaml::scan::{scan, Ev};
use bilingua_yaml::Yaml;
use std::collections::BTreeMap;

pub const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");

pub fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{DATA}/{name}")).unwrap()
}

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions { target_lang: target.map(String::from), ..Default::default() }
}

pub fn extract(b: &[u8]) -> Document {
    Yaml.extract(b, &opts(None)).unwrap()
}

pub fn extract_to(b: &[u8], target: &str) -> Document {
    Yaml.extract(b, &opts(Some(target))).unwrap()
}

/// The unit's source with tags written out as the file spelled them (what LocOS calls the unit text).
pub fn text_of(u: &Unit) -> String {
    let mut s = String::new();
    for i in u.source() {
        match i {
            Inline::Text(t) => s.push_str(&t),
            Inline::Tag { id, kind, .. } => {
                let c = &u.codes[&id];
                s.push_str(match kind {
                    TagKind::Open => c.open.as_deref(),
                    TagKind::Close => c.close.as_deref(),
                    TagKind::Standalone => c.standalone.as_deref(),
                }
                .unwrap_or(""));
            }
        }
    }
    s
}

pub fn texts(b: &[u8]) -> Vec<String> {
    extract(b).units.iter().map(text_of).collect()
}

/// Translates unit `i` to the plain text `f(i, source text)` (units not in the map stay untouched).
pub fn translate(b: &[u8], target: Option<&str>, f: impl Fn(usize, &str) -> Option<String>) -> Vec<u8> {
    let mut d = match target {
        Some(t) => extract_to(b, t),
        None => extract(b),
    };
    for i in 0..d.units.len() {
        if let Some(new) = f(i, &text_of(&d.units[i])) {
            let id = d.units[i].segments[0].id.clone();
            d.set_target(&id, vec![Inline::text(new)]).unwrap();
        }
    }
    Yaml.merge(&d).unwrap()
}

pub fn all(b: &[u8], target: Option<&str>, f: impl Fn(&str) -> String) -> Vec<u8> {
    translate(b, target, |_, t| Some(f(t)))
}

/// Pseudo-translation that keeps the tags: "[xx] " in front of the source.
pub fn pseudo(d: &mut Document, prefix: &str) {
    for u in &mut d.units {
        let mut t = vec![Inline::text(prefix)];
        t.extend(u.source());
        let id = u.segments[0].id.clone();
        let _ = id;
        u.segments[0].target = merge_text(t);
        u.segments[0].state = SegState::Translated;
    }
}

pub fn merge(d: &Document) -> Vec<u8> {
    Yaml.merge(d).unwrap()
}

/// Every scalar value of the file in document order (keys included), read with the crate's own reader.
pub fn scalars(b: &[u8]) -> Vec<String> {
    let t = std::str::from_utf8(b).unwrap().trim_start_matches('\u{feff}');
    scan(t, 200)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e {
            Ev::Scalar(s) => Some(s.value),
            _ => None,
        })
        .collect()
}

/// key path -> value for a flat-ish file (used to read back what a merge wrote).
pub fn kv(b: &[u8]) -> BTreeMap<String, String> {
    let d = extract(b);
    d.units.iter().map(|u| (u.context.key.clone().unwrap(), text_of(u))).collect()
}
