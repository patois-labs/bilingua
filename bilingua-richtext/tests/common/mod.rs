#![allow(dead_code)]
use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use serde_json::Value;

pub const CONTENTFUL: &[u8] = include_bytes!("../data/contentful.json");
pub const PORTABLE: &[u8] = include_bytes!("../data/portable.json");
pub const JIRA: &[u8] = include_bytes!("../data/jira.json");
pub const NOTION: &[u8] = include_bytes!("../data/notion.json");

pub const L: char = '\u{27E6}';
pub const R: char = '\u{27E7}';

/// `G` and `H` stand for the token brackets in test strings.
pub fn tk(s: &str) -> String {
    s.replace('G', &L.to_string()).replace('H', &R.to_string())
}

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("de-DE").with_granularity(Granularity::Paragraph)
}

pub fn open(f: &dyn Filter, bytes: &[u8]) -> Document {
    f.extract(bytes, &opts()).unwrap_or_else(|e| panic!("extract: {e}"))
}

/// Each unit's text with LocOS tokens (`G`..`H` spelled as the real brackets).
pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| to_locos_text(&u.source()).0).collect()
}

/// Sets unit `i`'s target from LocOS token text.
pub fn set(d: &mut Document, i: usize, locos: &str) -> Result<(), Error> {
    let map = to_locos_text(&d.units[i].source()).1;
    let id = d.units[i].segments[0].id.clone();
    d.set_target(&id, from_locos_text(locos, &map))
}

/// Every unit translated as `prefix + text`.
pub fn prefix_all(d: &mut Document, prefix: &str) {
    for i in 0..d.units.len() {
        let t = format!("{prefix}{}", texts(d)[i]);
        set(d, i, &t).unwrap();
    }
}

pub fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("not JSON: {e}\n{}", String::from_utf8_lossy(bytes)))
}

/// The translation is refused: by `set_target` itself, or by the merge with a typed `Unwritable`.
pub fn refused(f: &dyn Filter, mut d: Document, i: usize, text: &str) -> bool {
    match set(&mut d, i, text) {
        Err(_) => true,
        Ok(()) => matches!(f.merge(&d), Err(Error::Unwritable { .. })),
    }
}

pub fn find_all<'a>(v: &'a Value, key: &str, val: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            if m.get(key).and_then(Value::as_str) == Some(val) {
                out.push(v);
            }
            for x in m.values() {
                find_all(x, key, val, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| find_all(x, key, val, out)),
        _ => {}
    }
}

pub fn nodes<'a>(v: &'a Value, key: &str, val: &str) -> Vec<&'a Value> {
    let mut o = Vec::new();
    find_all(v, key, val, &mut o);
    o
}
