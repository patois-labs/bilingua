#![allow(dead_code)]
//! Helpers shared by the test files.

use bilingua_model::locos::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_xmb::{Xmb, Xtb};

pub const XMB: &[u8] = include_bytes!("../data/messages.xmb");
pub const XMB_LOCALIZE: &[u8] = include_bytes!("../data/messages-localize.xmb");
pub const XTB: &[u8] = include_bytes!("../data/messages.fr.xtb");

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("fr")
}

pub fn opts_to(lang: &str) -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang(lang)
}

pub fn ex(bytes: &[u8]) -> Document {
    try_ex(bytes).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn try_ex(bytes: &[u8]) -> Result<Document, Error> {
    Xmb.extract(bytes, &opts())
}

pub fn ex_to(bytes: &[u8], lang: &str) -> Document {
    Xmb.extract(bytes, &opts_to(lang)).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

/// The unit text as Localization OS writes it, tokens included.
pub fn locos(u: &Unit) -> String {
    to_locos_text(&u.source()).0
}

pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(locos).collect()
}

/// Sets unit `i` (0-based, in unit order) to the LocOS-token text `t`, resolving tokens with the unit's own map.
pub fn set_locos(d: &mut Document, i: usize, t: &str) {
    let (_, map) = to_locos_text(&d.units[i].source());
    d.units[i].segments[0].target = from_locos_text(t, &map);
    d.units[i].segments[0].state = SegState::Draft;
}

/// Every unit translated as `f(LocOS text)`.
pub fn translate_all(d: &mut Document, f: impl Fn(&str) -> String) {
    for i in 0..d.units.len() {
        let t = f(&locos(&d.units[i]));
        set_locos(d, i, &t);
    }
}

pub fn pseudo(d: &mut Document) {
    translate_all(d, |s| format!("FR {s}"));
}

pub fn merged(d: &Document) -> Vec<u8> {
    match d.format.as_str() {
        "xtb" => Xtb.merge(d),
        _ => Xmb.merge(d),
    }
    .unwrap_or_else(|e| panic!("merge failed: {e}"))
}

pub fn msg(r: Result<Document, Error>) -> String {
    match r {
        Err(e) => e.to_string(),
        Ok(_) => panic!("must be refused"),
    }
}

pub fn tag_ids(d: &Document) -> Vec<Vec<String>> {
    d.units
        .iter()
        .map(|u| {
            u.source()
                .iter()
                .filter_map(|i| match i {
                    Inline::Tag { id, kind, .. } => Some(format!("{id}:{kind:?}")),
                    _ => None,
                })
                .collect()
        })
        .collect()
}
