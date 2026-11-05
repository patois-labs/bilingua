#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_ttx::Ttx;

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_target_lang("de-DE").with_granularity(Granularity::Paragraph)
}

/// Bilingua's default: finished translations are loaded as targets.
pub const DEFAULT: Ttx = Ttx { skip_translated: false };
/// The filter as Localization OS reads TTX: a `<Tu>` with a finished translation is not a unit.
pub const LOCOS: Ttx = Ttx { skip_translated: true };

pub fn extract_with(f: &Ttx, data: &[u8]) -> Document {
    f.extract(data, &opts()).expect("extract")
}

/// The unit's source as LocOS tokens.
pub fn tok(inl: &[Inline]) -> String {
    to_locos_text(inl).0
}

/// (LocOS key, token text) of every unit.
pub fn units_with(f: &Ttx, data: &[u8]) -> Vec<(usize, String)> {
    let d = extract_with(f, data);
    d.units.iter().map(|u| (u.context.key.as_ref().expect("key").parse().expect("number"), tok(&u.source()))).collect()
}

pub fn id_of(d: &Document, key: usize) -> String {
    d.units.iter().find(|u| u.context.key.as_deref() == Some(&key.to_string())).unwrap_or_else(|| panic!("no unit for key {key}")).id.clone()
}

/// Sets a target written as LocOS tokens.
pub fn set(d: &mut Document, key: usize, text: &str) {
    let id = id_of(d, key);
    let u = d.units.iter().find(|u| u.id == id).unwrap();
    let (_, map) = to_locos_text(&u.source());
    let inl = from_locos_text(text, &map);
    d.set_target(&id, inl).expect("set_target");
}

pub fn translate_with(f: &Ttx, data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    let mut d = extract_with(f, data);
    for (k, t) in mapping {
        set(&mut d, *k, t);
    }
    f.merge(&d).expect("merge")
}

/// UTF-16 with a BOM, CRLF and `encoding='utf-16'`: the usual TTX on disk.
pub fn utf16(raw: &[u8], be: bool, bom: bool) -> Vec<u8> {
    let text = String::from_utf8(raw.to_vec()).unwrap().replace("encoding='utf-8'", "encoding='utf-16'").replace('\n', "\r\n");
    let mut v = Vec::new();
    if bom {
        v.extend_from_slice(if be { &[0xFE, 0xFF] } else { &[0xFF, 0xFE] });
    }
    for u in text.encode_utf16() {
        v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
    }
    v
}

pub fn decode16(raw: &[u8], be: bool, bom: bool) -> String {
    let body = if bom { &raw[2..] } else { raw };
    let u: Vec<u16> = body.chunks(2).map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect();
    String::from_utf16(&u).unwrap()
}

/// Every `<Tu ...>...</Tu>` in order (UTF-8 bytes).
pub fn tus(out: &[u8]) -> Vec<Vec<u8>> {
    let mut res = Vec::new();
    let mut pos = 0;
    while let Some(s) = find(out, b"<Tu ", pos) {
        let e = find(out, b"</Tu>", s).expect("closed") + 5;
        res.push(out[s..e].to_vec());
        pos = e;
    }
    res
}

pub fn outside(out: &[u8]) -> Vec<u8> {
    let mut o = out.to_vec();
    for tu in tus(out) {
        let at = find(&o, &tu, 0).unwrap();
        o.splice(at..at + tu.len(), b"<TU/>".iter().copied());
    }
    o
}

pub fn find(h: &[u8], n: &[u8], from: usize) -> Option<usize> {
    h[from..].windows(n.len()).position(|w| w == n).map(|p| p + from)
}

pub fn contains(h: &[u8], n: &[u8]) -> bool {
    find(h, n, 0).is_some()
}

/// An untouched merge must be the original bytes.
pub fn assert_identity(f: &Ttx, data: &[u8]) {
    let d = extract_with(f, data);
    assert_eq!(f.merge(&d).unwrap(), data, "untouched merge");
}
