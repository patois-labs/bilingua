#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_image::{ImageFilter, SvgFilter};
use bilingua_model::*;

/// The Localization OS profile's merge options (UTF-8 out, language stamped even when nothing is translated).
pub fn locos_opts() -> MergeOptions {
    profile::locos().merge_options("svg")
}

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_granularity(Granularity::Paragraph)
}

pub fn with_target(t: &str) -> ExtractOptions {
    ExtractOptions::new().with_target_lang(t).with_granularity(Granularity::Paragraph)
}

pub fn extract(data: &[u8]) -> Document {
    SvgFilter.extract(data, &opts()).expect("extract")
}

pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| plain_text(&u.source())).collect()
}

pub fn styles(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| u.context.style.clone().unwrap_or_default()).collect()
}

/// Sets the translation of unit `i` (what LocOS calls `paragraph_texts[str(i)]`).
pub fn set(d: &mut Document, i: usize, t: &str) {
    d.units[i].segments[0].target = vec![Inline::text(t)];
}

/// LocOS `build(original, {str(i): text})`: no target language, UTF-8 out.
pub fn build(data: &[u8], map: &[(usize, &str)]) -> Result<Vec<u8>, Error> {
    let mut d = SvgFilter.extract(data, &opts())?;
    for (i, t) in map {
        set(&mut d, *i, t);
    }
    SvgFilter.merge_with(&d, &locos_opts())
}

/// LocOS `build_localized(original, map, locale)`.
pub fn build_localized(data: &[u8], map: &[(usize, &str)], locale: &str) -> Result<Vec<u8>, Error> {
    let o = if locale.is_empty() { opts() } else { with_target(locale) };
    let mut d = SvgFilter.extract(data, &o)?;
    for (i, t) in map {
        set(&mut d, *i, t);
    }
    SvgFilter.merge_with(&d, &locos_opts())
}

pub fn s(b: Vec<u8>) -> String {
    String::from_utf8(b).expect("utf-8 output")
}

pub fn image_extract(data: &[u8]) -> Result<Document, Error> {
    ImageFilter.extract(data, &opts())
}

pub fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for &x in b {
        h = (h ^ x as u64).wrapping_mul(0x0000_0100_0000_01B3);
    }
    format!("{}:{:016x}", b.len(), h)
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn chunk(ty: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = (data.len() as u32).to_be_bytes().to_vec();
    v.extend_from_slice(ty);
    v.extend_from_slice(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(ty);
    crc.update(data);
    v.extend_from_slice(&crc.finalize().to_be_bytes());
    v
}

/// A grey 8-bit PNG of the given size with a valid zlib pixel stream; `json` goes into a `tEXt` chunk named
/// `loc_tms_regions` (characters up to U+00FF as Latin-1 bytes), `extra` chunks follow it before the image data.
pub fn png(json: Option<&str>, width: u32, height: u32, extra: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = width.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
    out.extend(chunk(b"IHDR", &ihdr));
    if let Some(j) = json {
        let mut t = b"loc_tms_regions\0".to_vec();
        t.extend(j.chars().map(|c| c as u32 as u8));
        out.extend(chunk(b"tEXt", &t));
    }
    for (ty, d) in extra {
        out.extend(chunk(ty, d));
    }
    let raw: Vec<u8> = (0..height).flat_map(|_| std::iter::once(0u8).chain(std::iter::repeat_n(0x80u8, width as usize))).collect();
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(&mut e, &raw).unwrap();
    out.extend(chunk(b"IDAT", &e.finish().unwrap()));
    out.extend(chunk(b"IEND", &[]));
    out
}

/// Parses `text` as XML with quick-xml (matching end tags, decodable entities) and checks every character is legal
/// in XML 1.0.
pub fn wellformed(text: &str) -> Result<(), String> {
    if let Some(c) = text.chars().find(|c| matches!(*c as u32, 0..=8 | 0xB | 0xC | 0xE..=0x1F | 0xFFFE | 0xFFFF)) {
        return Err(format!("illegal XML character U+{:04X}", c as u32));
    }
    let mut r = quick_xml::Reader::from_str(text);
    loop {
        match r.read_event() {
            Ok(quick_xml::events::Event::Eof) => return Ok(()),
            Ok(quick_xml::events::Event::Text(t)) => {
                let d = t.decode().map_err(|e| e.to_string())?;
                quick_xml::escape::unescape(&d).map_err(|e| e.to_string())?;
            }
            Ok(_) => {}
            Err(e) => return Err(format!("{e} at {}", r.error_position())),
        }
    }
}

/// The start tag of the first element whose `id` attribute is `id`.
pub fn tag_with_id(text: &str, id: &str) -> String {
    let needle = format!("id=\"{id}\"");
    let at = text.find(&needle).unwrap_or_else(|| panic!("no element with id {id} in {text}"));
    let s = text[..at].rfind('<').unwrap();
    let e = text[at..].find('>').unwrap() + at;
    text[s..=e].to_string()
}

/// Element names of all start tags, in order.
pub fn start_names(text: &str) -> Vec<String> {
    let mut r = quick_xml::Reader::from_str(text);
    let mut out = Vec::new();
    loop {
        match r.read_event() {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e)) => out.push(String::from_utf8_lossy(e.name().as_ref()).into_owned()),
            Ok(quick_xml::events::Event::Eof) | Err(_) => return out,
            _ => {}
        }
    }
}
