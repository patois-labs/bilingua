//! Shared helpers: fixtures, zip building and reading, translating a document.
#![allow(dead_code)]

use bilingua_model::{Document, ExtractOptions, Filter, Inline, SegState};
use std::io::{Cursor, Read, Write};

pub const MQ: &[u8] = include_bytes!("../data/sample.mqxliff");
pub const SDL: &[u8] = include_bytes!("../data/sample.sdlxliff");
pub const WS: &[u8] = include_bytes!("../data/content.xlf");
pub const TMX: &[u8] = b"<?xml version=\"1.0\"?><tmx version=\"1.4\"><header/><body/></tmx>";
pub const TBX: &[u8] = b"<?xml version=\"1.0\"?><martif type=\"TBX\"><text><body/></text></martif>";
pub const DATE: (u16, u8, u8, u8, u8, u8) = (2024, 5, 6, 7, 8, 10);

pub fn skeleton_bytes() -> Vec<u8> {
    (0..1024).map(|i| (i % 256) as u8).collect()
}

pub fn filter() -> Box<dyn Filter> {
    bilingua_xliffpkg::filters().remove(0)
}

/// (name, body, deflated)
pub fn zip_of(members: &[(&str, &[u8], bool)], comment: &[u8]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, b, deflate) in members {
        let m = if *deflate { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored };
        let t = zip::DateTime::from_date_and_time(DATE.0, DATE.1, DATE.2, DATE.3, DATE.4, DATE.5).unwrap();
        let o = zip::write::SimpleFileOptions::default().compression_method(m).last_modified_time(t);
        w.start_file(*n, o).unwrap();
        w.write_all(b).unwrap();
    }
    if !comment.is_empty() {
        w.set_raw_comment(comment.to_vec().into_boxed_slice()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub fn mqxlz() -> Vec<u8> {
    zip_of(&[("document.mqxliff", MQ, true), ("skeleton", &skeleton_bytes(), false), ("preview.html", b"<p>x</p>", true)], b"memoQ export")
}
pub fn xlz() -> Vec<u8> {
    zip_of(&[("content.xlf", WS, true), ("content.skl", b"<skl>{1}</skl>", true)], b"")
}
pub fn wsxz() -> Vec<u8> {
    zip_of(
        &[("tm/project.tmx", TMX, true), ("fr-FR/guide.docx.sdlxliff", SDL, true), ("tb/terms.tbx", TBX, true), ("fr-FR/help.html.xlf", WS, true)],
        b"",
    )
}

/// (name, date, stored, body) per member in order.
pub type Date = Option<(u16, u8, u8, u8, u8, u8)>;
pub type Member = (String, Date, bool, Vec<u8>);

pub fn members(pkg: &[u8]) -> Vec<Member> {
    let mut z = zip::ZipArchive::new(Cursor::new(pkg)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            let d = f.last_modified().map(|d| (d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second()));
            (f.name().unwrap().into_owned(), d, f.compression() == zip::CompressionMethod::Stored, b)
        })
        .collect()
}

pub fn member(pkg: &[u8], name: &str) -> Vec<u8> {
    members(pkg).into_iter().find(|m| m.0 == name).unwrap().3
}

pub fn comment(pkg: &[u8]) -> Vec<u8> {
    zip::ZipArchive::new(Cursor::new(pkg)).unwrap().comment().to_vec()
}

pub fn text(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

/// The `<trans-unit id="..">...</trans-unit>` of `out`.
pub fn tu(out: &str, id: &str) -> String {
    let start = out.find(&format!("<trans-unit id=\"{id}\"")).unwrap_or_else(|| panic!("no trans-unit {id}"));
    out[start..start + out[start..].find("</trans-unit>").unwrap()].to_string()
}

pub fn extract(f: &dyn Filter, pkg: &[u8]) -> Document {
    f.extract(pkg, &ExtractOptions::default()).unwrap()
}

pub fn extract_to(f: &dyn Filter, pkg: &[u8], lang: &str) -> Document {
    f.extract(pkg, &ExtractOptions { target_lang: Some(lang.into()), ..Default::default() }).unwrap()
}

/// Text of the inlines with every tag as a marker.
pub fn locos(v: &[Inline]) -> String {
    bilingua_model::locos::to_locos_text(v).0
}

/// A translated copy of the source: text parts replaced by `f`, tags kept.
pub fn translated(src: &[Inline], f: impl Fn(&str) -> String) -> Vec<Inline> {
    src.iter()
        .map(|i| match i {
            Inline::Text(t) => Inline::Text(f(t)),
            t => t.clone(),
        })
        .collect()
}

pub fn translate(d: &mut Document, seg_id: &str, f: impl Fn(&str) -> String) {
    let src = d.find(seg_id).unwrap_or_else(|| panic!("no segment {seg_id}")).1.source.clone();
    d.set_target(seg_id, translated(&src, f)).unwrap();
    d.set_state(seg_id, SegState::Translated).unwrap();
}

pub fn ids(d: &Document) -> Vec<String> {
    d.segments().map(|(_, s)| s.id.clone()).collect()
}
