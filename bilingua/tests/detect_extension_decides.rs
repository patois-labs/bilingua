//! T8: a file with an extension is never sniffed, except where one extension is shared by several formats. These
//! tests pin that a mislabelled file keeps the filter its extension names (as before the change), and that the
//! lazy SRX rules still split sentences once a document is resegmented.

use bilingua::{detect, extract, filters, resegment, ExtractOptions, Granularity};

/// Bytes that several filters' sniffers recognise.
const SAMPLES: &[&[u8]] = &[
    b"",
    b"just words",
    b"<?xml version=\"1.0\"?><resources><string name=\"a\">A</string></resources>",
    b"<?xml version=\"1.0\"?><root><p>Hello</p></root>",
    b"<html><body><p>Hello</p></body></html>",
    b"{\"@@locale\": \"en\", \"a\": \"A\"}",
    b"{\"a\": \"b\"}",
    b"msgid \"\"\nmsgstr \"\"\n",
    b"a = b\n",
    b"PK\x03\x04",
];

/// Extensions whose bytes decide between formats (the special cases at the top of `detect_with`).
const SHARED: &[&str] = &["ts", "json", "xml", "xlf", "xliff"];

#[test]
fn every_claimed_extension_goes_to_its_first_claiming_filter_whatever_the_bytes() {
    let all = filters();
    for f in &all {
        for ext in f.extensions() {
            if SHARED.contains(ext) {
                continue;
            }
            let want = all.iter().find(|g| g.extensions().contains(ext)).unwrap().id();
            for bytes in SAMPLES {
                let got = detect(&format!("file.{ext}"), bytes).map(|g| g.id());
                assert_eq!(got, Some(want), "file.{ext} with {:?}", String::from_utf8_lossy(bytes));
            }
        }
    }
}

#[test]
fn a_mislabelled_file_keeps_its_extension_filter() {
    let xml = b"<?xml version=\"1.0\"?><resources><string name=\"a\">A</string></resources>";
    assert_eq!(detect("notes.txt", xml).unwrap().id(), "txt");
    assert_eq!(detect("notes.txt", b"{\"a\": \"b\"}").unwrap().id(), "txt");
    assert_eq!(detect("a.po", b"<html><body>x</body></html>").unwrap().id(), "po");
    assert_eq!(detect("a.properties", b"{\"a\": \"b\"}").unwrap().id(), "properties");
    // the shared extensions still look at the bytes
    assert_eq!(detect("strings.xml", xml).unwrap().id(), "android");
    // no extension: the bytes decide; an extension nobody claims: refused
    assert_eq!(detect("data", b"{\"a\": \"b\"}").unwrap().id(), "json");
    assert!(detect("x.unknownext", b"just words").is_none());
}

#[test]
fn paragraph_extraction_resegments_to_sentences() {
    let src = b"One sentence here. Another one there.\n";
    let opts = ExtractOptions::new().with_granularity(Granularity::Paragraph);
    let mut doc = extract("a.txt", src, &opts).unwrap();
    assert_eq!(doc.units[0].segments.len(), 1);
    resegment(&mut doc, "en", Granularity::Sentence);
    assert_eq!(doc.units[0].segments.len(), 2);
    resegment(&mut doc, "en", Granularity::Document);
    assert_eq!(doc.units[0].segments.len(), 1);
}
