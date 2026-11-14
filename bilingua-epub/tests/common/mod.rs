//! Fixtures and helpers shared by the test files: the hand-written EPUB of the LocOS suite (zipped at run time),
//! readers for the members of a result, and an independent well-formedness check.
#![allow(dead_code)]

use bilingua_epub::Epub;
use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

pub const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"#;

pub const OPF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid" xml:lang="en">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>
    <dc:title id="title">The Little Guide</dc:title>
    <dc:creator>Ada Writer</dc:creator>
    <dc:language>en</dc:language>
    <dc:description>A short book about &amp; for testing.</dc:description>
    <meta property="dcterms:modified">2026-10-10T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="c2" href="text/chapter%202.xhtml" media-type="application/xhtml+xml"/>
    <item id="c1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="css" href="style.css" media-type="text/css"/>
    <item id="font" href="fonts/f.otf" media-type="font/otf"/>
    <item id="img" href="img/cover.png" media-type="image/png"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="c1"/>
    <itemref idref="c2"/>
  </spine>
</package>
"#;

pub const CH1: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"
  xmlns:epub="http://www.idpf.org/2007/ops" lang="en" xml:lang="en">
<head><title>Chapter One</title><link rel="stylesheet" href="../style.css"/></head>
<body>
<h1>Chapter One</h1>
<p>Hello <b>bold</b> world, you have {count} items and %1$s left.</p>
<p><img src="../img/cover.png" alt="A red cover"/></p>
</body>
</html>
"#;

pub const CH2: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" lang="en">
<head><title>Chapter Two</title></head>
<body><p>Second chapter text.</p></body>
</html>
"#;

pub const NAV: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"
  xmlns:epub="http://www.idpf.org/2007/ops" lang="en" xml:lang="en">
<head><title>Contents</title></head>
<body><nav epub:type="toc"><ol>
<li><a href="text/ch1.xhtml">Start here</a></li>
<li><a href="text/chapter%202.xhtml">Next steps</a></li>
</ol></nav></body>
</html>
"#;

pub const NCX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd">
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1" xml:lang="en">
  <head><meta name="dtb:uid" content="urn:uuid:12345678-1234-1234-1234-123456789abc"/></head>
  <docTitle><text>The Little Guide</text></docTitle>
  <navMap>
    <navPoint id="p1" playOrder="1">
      <navLabel><text>Start here</text></navLabel>
      <content src="text/ch1.xhtml"/>
    </navPoint>
    <navPoint id="p2" playOrder="2">
      <navLabel><text>Next steps</text></navLabel>
      <content src="text/chapter%202.xhtml"/>
    </navPoint>
  </navMap>
</ncx>
"#;

pub const CSS: &[u8] = b"body { font-family: 'F'; }\n";

pub fn font() -> Vec<u8> {
    (0..1024).map(|i| (i % 256) as u8).collect()
}

pub fn png() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend(std::iter::repeat_n(0u8, 64));
    v
}

pub fn base_members() -> Vec<(String, Vec<u8>)> {
    let m = |n: &str, d: &[u8]| (n.to_string(), d.to_vec());
    vec![
        m("META-INF/container.xml", CONTAINER.as_bytes()),
        m("OEBPS/content.opf", OPF.as_bytes()),
        m("OEBPS/nav.xhtml", NAV.as_bytes()),
        m("OEBPS/toc.ncx", NCX.as_bytes()),
        m("OEBPS/text/ch1.xhtml", CH1.as_bytes()),
        m("OEBPS/text/chapter 2.xhtml", CH2.as_bytes()),
        m("OEBPS/style.css", CSS),
        m("OEBPS/fonts/f.otf", &font()),
        m("OEBPS/img/cover.png", &png()),
    ]
}

/// Builds an EPUB: the default members with `over` replacing (or appending), `drop` removing; `mimetype` stored
/// first unless `None` or `first` is false.
pub fn build(over: &[(&str, &[u8])], drop: &[&str], mimetype: Option<&[u8]>, first: bool) -> Vec<u8> {
    let mut members = base_members();
    for (n, d) in over {
        match members.iter_mut().find(|(k, _)| k == n) {
            Some(m) => m.1 = d.to_vec(),
            None => members.push((n.to_string(), d.to_vec())),
        }
    }
    members.retain(|(n, _)| !drop.contains(&n.as_str()));
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    if let (Some(mt), true) = (mimetype, first) {
        w.start_file("mimetype", stored).unwrap();
        w.write_all(mt).unwrap();
    }
    for (n, d) in &members {
        w.start_file(n.as_str(), deflate).unwrap();
        w.write_all(d).unwrap();
    }
    if let (Some(mt), false) = (mimetype, first) {
        w.start_file("mimetype", stored).unwrap();
        w.write_all(mt).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub fn epub() -> Vec<u8> {
    build(&[], &[], Some(b"application/epub+zip"), true)
}

pub fn epub_with(over: &[(&str, &[u8])]) -> Vec<u8> {
    build(over, &[], Some(b"application/epub+zip"), true)
}

#[derive(Debug, Clone)]
pub struct Info {
    pub name: String,
    pub stored: bool,
    pub extra: usize,
}

/// Member name to bytes, plus the order and storage of the members.
pub fn members(data: &[u8]) -> (BTreeMap<String, Vec<u8>>, Vec<Info>) {
    let mut z = zip::ZipArchive::new(Cursor::new(data)).expect("a zip");
    let mut map = BTreeMap::new();
    let mut infos = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut d = Vec::new();
        f.read_to_end(&mut d).unwrap();
        let name = f.name().unwrap().into_owned();
        infos.push(Info { name: name.clone(), stored: f.compression() == zip::CompressionMethod::Stored, extra: f.extra_data().map_or(0, |e| e.len()) });
        map.insert(name, d);
    }
    (map, infos)
}

pub fn member_text(m: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    to_utf8(m.get(name).unwrap_or_else(|| panic!("no member {name}")))
}

pub fn to_utf8(raw: &[u8]) -> String {
    if raw.starts_with(&[0xFF, 0xFE]) || raw.starts_with(&[0xFE, 0xFF]) {
        let be = raw[0] == 0xFE;
        let u: Vec<u16> = raw[2..].chunks(2).map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect();
        return String::from_utf16(&u).unwrap();
    }
    let raw = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
    match String::from_utf8(raw.to_vec()) {
        Ok(s) => s,
        Err(_) => raw.iter().map(|b| *b as char).collect(), // a single-byte code page, enough for these checks
    }
}

/// Every XML member parses with quick-xml (matching end tags, entities resolvable).
pub fn well_formed(m: &BTreeMap<String, Vec<u8>>) {
    for (name, raw) in m {
        if !(name.ends_with(".xhtml") || name.ends_with(".opf") || name.ends_with(".ncx") || name.ends_with(".xml")) {
            continue;
        }
        let text = to_utf8(raw);
        let mut r = quick_xml::Reader::from_str(&text);
        let mut depth = 0i32;
        loop {
            match r.read_event() {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(quick_xml::events::Event::Start(_)) => depth += 1,
                Ok(quick_xml::events::Event::End(_)) => depth -= 1,
                Ok(quick_xml::events::Event::Text(t)) => {
                    let s = t.decode().unwrap_or_else(|e| panic!("{name}: {e}"));
                    quick_xml::escape::unescape(&s).unwrap_or_else(|e| panic!("{name}: undefined entity in {s:?}: {e}"));
                }
                Ok(_) => {}
                Err(e) => panic!("{name} is not well-formed: {e}\n{text}"),
            }
        }
        assert_eq!(depth, 0, "{name}");
    }
}

pub fn extract(bytes: &[u8]) -> Document {
    Epub.extract(bytes, &ExtractOptions::default()).unwrap()
}

pub fn extract_err(bytes: &[u8]) -> Error {
    Epub.extract(bytes, &ExtractOptions::default()).expect_err("expected a refusal")
}

/// The unit text as LocOS shows it (inline codes as tokens).
pub fn loc_text(u: &Unit) -> String {
    to_locos_text(&u.source()).0
}

pub fn loc_texts(d: &Document) -> Vec<String> {
    d.units.iter().map(loc_text).collect()
}

/// Sets the target of unit number `i` (LocOS `p_index`) from LocOS text.
pub fn set(doc: &mut Document, i: usize, text: &str) {
    let u = &doc.units[i];
    let (_, map) = to_locos_text(&u.source());
    let inl = from_locos_text(text, &map);
    let id = u.segments[0].id.clone();
    doc.set_target(&id, inl).unwrap();
}

/// "FR " before every unit, as the LocOS suite does.
pub fn translate_all(doc: &mut Document) {
    for i in 0..doc.units.len() {
        let t = format!("FR {}", loc_text(&doc.units[i]));
        set(doc, i, &t);
    }
}

pub fn merge(doc: &Document) -> Vec<u8> {
    Epub.merge(doc).unwrap()
}
