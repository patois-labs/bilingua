#![allow(dead_code)]
use bilingua_model::locos::to_locos_text;
use bilingua_model::*;
use bilingua_toml::TomlFilter;

pub fn extract(b: &[u8]) -> Document {
    TomlFilter.extract(b, &ExtractOptions::default()).unwrap()
}

pub fn try_extract(b: &[u8]) -> Result<Document, Error> {
    TomlFilter.extract(b, &ExtractOptions::default())
}

pub fn merge(d: &Document) -> Vec<u8> {
    TomlFilter.merge(d).unwrap()
}

/// (style, LocOS-token text) per unit, like the LocOS suite's `(u.style, u.text)`.
pub fn pairs(d: &Document) -> Vec<(String, String)> {
    d.units.iter().map(|u| (u.context.style.clone().unwrap_or_default(), to_locos_text(&u.source()).0)).collect()
}

/// Sets every unit's target to `f(source inlines)`.
pub fn translate_all(d: &mut Document, f: impl Fn(&Unit) -> Vec<Inline>) {
    let ids: Vec<(String, Vec<Inline>)> = d.units.iter().filter(|u| u.translate).map(|u| (u.id.clone(), f(u))).collect();
    for (id, t) in ids {
        d.set_target(&id, t).unwrap();
    }
}

/// "DE " + the source, tags kept.
pub fn prefix_de(u: &Unit) -> Vec<Inline> {
    let mut t = vec![Inline::text("DE ")];
    t.extend(u.source());
    merge_text(t)
}

pub fn toml_value(b: &[u8]) -> toml::Table {
    let s = std::str::from_utf8(b).unwrap();
    toml::from_str(s.strip_prefix('\u{feff}').unwrap_or(s)).unwrap_or_else(|e| panic!("output is not valid TOML: {e}\n{s}"))
}

pub fn set(d: &mut Document, i: usize, text: &str) {
    let id = d.units[i].id.clone();
    d.set_target(&id, vec![Inline::text(text)]).unwrap();
}
