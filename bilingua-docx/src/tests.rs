use super::*;
use std::io::{Read, Write};

pub(crate) const NS: &str =
    "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\" \
xmlns:v=\"urn:schemas-microsoft-com:vml\"";

pub(crate) const CT: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>";

pub(crate) const RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";

pub(crate) fn body(inner: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document {NS}><w:body>{inner}<w:sectPr/></w:body></w:document>")
}

pub(crate) fn part(root: &str, inner: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:{root} {NS}>{inner}</w:{root}>")
}

pub(crate) fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

pub(crate) fn make(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let deflate = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    w.start_file("[Content_Types].xml", deflate).unwrap();
    w.write_all(CT.as_bytes()).unwrap();
    w.start_file("_rels/.rels", deflate).unwrap();
    w.write_all(RELS.as_bytes()).unwrap();
    for (name, data) in parts {
        let m = if name.ends_with(".bin") {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        w.start_file(
            *name,
            zip::write::SimpleFileOptions::default().compression_method(m),
        )
        .unwrap();
        w.write_all(data.as_bytes()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub(crate) fn doc_of(inner: &str) -> Vec<u8> {
    make(&[("word/document.xml", &body(inner))])
}

pub(crate) fn entries_of(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut v = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut d = Vec::new();
        f.read_to_end(&mut d).unwrap();
        v.push((f.name().unwrap().into_owned(), d));
    }
    v
}

pub(crate) fn xml_of(bytes: &[u8], name: &str) -> String {
    let e = entries_of(bytes)
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no {name}"));
    String::from_utf8(e.1).unwrap()
}

pub(crate) fn extract(bytes: &[u8]) -> Document {
    Docx.extract(bytes, &ExtractOptions::default()).unwrap()
}

pub(crate) fn src(doc: &Document, id: &str) -> String {
    let (_, s) = doc.find(id).unwrap_or_else(|| panic!("no segment {id}"));
    plain_text(&s.source)
}

pub(crate) fn same_entries(a: &[u8], b: &[u8]) {
    let (ea, eb) = (entries_of(a), entries_of(b));
    assert_eq!(ea.len(), eb.len());
    for (x, y) in ea.iter().zip(&eb) {
        assert_eq!(x.0, y.0);
        assert!(
            x.1 == y.1,
            "entry {} differs:
{}
{}",
            x.0,
            String::from_utf8_lossy(&x.1),
            String::from_utf8_lossy(&y.1)
        );
    }
}

/// Every document must survive an unchanged round trip with identical parts, and every segment translated
/// to a marked string must come back when the result is extracted again.
pub(crate) fn round_trip(bytes: &[u8]) -> Document {
    let doc = extract(bytes);
    let (out, rep) = merge_with_report(&doc).unwrap();
    assert!(rep.warnings.is_empty());
    same_entries(bytes, &out);
    doc
}

pub(crate) const LANGS: [(&str, &str); 11] = [
    ("en", "The quick brown fox."),
    ("es", "El veloz zorro marrón."),
    ("fr", "Le renard brun rapide."),
    ("it", "La rapida volpe marrone."),
    ("de", "Der schnelle braune Fuchs."),
    ("nl", "De snelle bruine vos."),
    ("pt", "A rápida raposa marrom."),
    ("ja", "素早い茶色の狐。"),
    ("zh", "敏捷的棕色狐狸。"),
    ("ko", "빠른 갈색 여우."),
    ("ar", "الثعلب البني السريع."),
];

#[test]
pub(crate) fn eleven_languages_round_trip_and_translate() {
    let inner: String = LANGS.iter().map(|(_, t)| p(t)).collect();
    let bytes = doc_of(&inner);
    let mut doc = round_trip(&bytes);
    assert_eq!(doc.units.len(), 11);
    for (i, (_, t)) in LANGS.iter().enumerate() {
        assert_eq!(src(&doc, &format!("p{}", i + 1)), *t);
    }
    // Translate every paragraph with text from the next language (a rotation).
    for i in 0..11 {
        let t = LANGS[(i + 1) % 11].1;
        doc.set_target(&format!("p{}", i + 1), vec![Inline::text(t)])
            .unwrap();
    }
    let out = Docx.merge(&doc).unwrap();
    let again = extract(&out);
    for i in 0..11 {
        assert_eq!(src(&again, &format!("p{}", i + 1)), LANGS[(i + 1) % 11].1);
    }
}

#[test]
pub(crate) fn untranslated_segments_write_the_source() {
    let bytes = doc_of(&(p("One") + &p("Two")));
    let mut doc = extract(&bytes);
    doc.set_target("p2", vec![Inline::text("Dos")]).unwrap();
    let out = Docx.merge(&doc).unwrap();
    let x = xml_of(&out, "word/document.xml");
    assert!(x.contains("<w:t>One</w:t>"));
    assert!(x.contains("<w:t>Dos</w:t>"));
    assert!(!x.contains("Two"));
}
