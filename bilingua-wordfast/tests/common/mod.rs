#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_model::*;
use bilingua_wordfast::{Txlf, Txml};

pub fn opts() -> ExtractOptions {
    ExtractOptions { target_lang: Some("de-DE".into()), granularity: Granularity::Paragraph, ..Default::default() }
}

/// The filters as the LocOS adapter uses them: no confirmed segments listed.
pub const TXML: Txml = Txml { load_targets: false };
pub const TXLF: Txlf = Txlf { load_targets: false };
/// The defaults: confirmed translations are listed as read-only units with their targets.
pub const TXML_T: Txml = Txml { load_targets: true };
pub const TXLF_T: Txlf = Txlf { load_targets: true };

pub fn extract(f: &dyn Filter, data: &[u8]) -> Document {
    f.extract(data, &opts()).expect("extract")
}

pub fn locked_opts() -> ExtractOptions {
    ExtractOptions { include_locked: true, ..opts() }
}

/// A unit's source as LocOS text (tokens).
pub fn locos(u: &Unit) -> String {
    locos::to_locos_text(&u.source()).0
}

/// (unit id, LocOS text) of every unit.
pub fn units(f: &dyn Filter, data: &[u8]) -> Vec<(String, String)> {
    extract(f, data).units.iter().map(|u| (u.id.clone(), locos(u))).collect()
}

pub fn unit_map(f: &dyn Filter, data: &[u8]) -> std::collections::BTreeMap<usize, String> {
    units(f, data).into_iter().map(|(i, t)| (i.parse().unwrap(), t)).collect()
}

/// Sets the target of unit `key` from LocOS text (tokens of the source).
pub fn set(doc: &mut Document, key: usize, text: &str) {
    let i = doc.units.iter().position(|u| u.id == key.to_string()).unwrap_or_else(|| panic!("no unit {key}"));
    let (_, map) = locos::to_locos_text(&doc.units[i].source());
    let (inl, problems) = locos::parse_locos_text(text, &map);
    assert!(problems.is_empty(), "{problems:?}");
    let id = doc.units[i].segments[0].id.clone();
    doc.set_target(&id, inl).expect("set_target");
}

pub fn translate(f: &dyn Filter, data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    let mut d = extract(f, data);
    for (k, t) in mapping {
        set(&mut d, *k, t);
    }
    f.merge(&d).expect("merge")
}

pub fn merged(f: &dyn Filter, d: &Document) -> Vec<u8> {
    f.merge(d).expect("merge")
}

fn slice<'a>(out: &'a [u8], open: &str, close: &str) -> &'a [u8] {
    let o = open.as_bytes();
    let start = out.windows(o.len()).position(|w| w == o).unwrap_or_else(|| panic!("{open} not found"));
    let c = close.as_bytes();
    let rel = out[start..].windows(c.len()).position(|w| w == c).expect("close");
    &out[start..start + rel + c.len()]
}

/// The `<segment segmentId="N">` element of a TXML file.
pub fn seg<'a>(out: &'a [u8], id: &str) -> &'a [u8] {
    slice(out, &format!("<segment segmentId=\"{id}\""), "</segment>")
}

/// The `<trans-unit id="N">` element of a TXLF file.
pub fn tu<'a>(out: &'a [u8], id: &str) -> &'a [u8] {
    slice(out, &format!("<trans-unit id=\"{id}\""), "</trans-unit>")
}

pub fn contains(hay: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    hay.windows(n.len()).any(|w| w == n)
}

fn replace(hay: &[u8], needle: &[u8]) -> Vec<u8> {
    let at = hay.windows(needle.len()).position(|w| w == needle).expect("cut present");
    let mut v = hay[..at].to_vec();
    v.extend_from_slice(b"<CUT/>");
    v.extend_from_slice(&hay[at + needle.len()..]);
    v
}

/// Cuts the given elements out of both files and checks the rest of the bytes are the same.
pub fn only_changed(before: &[u8], after: &[u8], cut: fn(&[u8], &str) -> Vec<u8>, ids: &[&str]) {
    let (mut a, mut b) = (before.to_vec(), after.to_vec());
    for id in ids {
        let (ca, cb) = (cut(before, id), cut(after, id));
        a = replace(&a, &ca);
        b = replace(&b, &cb);
    }
    assert_eq!(String::from_utf8_lossy(&a), String::from_utf8_lossy(&b), "bytes outside the written segments changed");
}

pub fn seg_owned(out: &[u8], id: &str) -> Vec<u8> {
    seg(out, id).to_vec()
}

pub fn tu_owned(out: &[u8], id: &str) -> Vec<u8> {
    tu(out, id).to_vec()
}

pub fn lock_reasons(f: &dyn Filter, data: &[u8]) -> std::collections::BTreeMap<String, usize> {
    let d = f.extract(data, &locked_opts()).expect("extract");
    let mut m = std::collections::BTreeMap::new();
    for u in d.units.iter().filter(|u| !u.translate) {
        *m.entry(u.lock_reason.clone().unwrap_or_default()).or_insert(0) += 1;
    }
    m
}
