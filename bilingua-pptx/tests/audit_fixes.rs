//! Regression tests for the PPTX fixes of the 0.9.1 security audit: building a paragraph's inlines is linear in
//! its runs and codes and an oversized paragraph is refused before the build, and under the LocOS profile a part
//! that starts with two byte order marks is refused instead of shifting every span.

use bilingua_model::{Document, Error, ExtractOptions, Filter, Inline, LimitKind};
use bilingua_pptx::Pptx;
use bilingua_zip::{write_zip, EntryMeta};
use std::time::{Duration, Instant};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// A one-slide deck; `slide` is the bytes of `ppt/slides/slide1.xml`.
fn deck(slide: Vec<u8>) -> Vec<u8> {
    let ct = "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slides/slide1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/></Types>".to_string();
    let root = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>");
    let pres = format!("<p:presentation xmlns:r=\"{R}\" xmlns:p=\"{P}\"><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId1\"/></p:sldIdLst></p:presentation>");
    let prels = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/slide\" Target=\"slides/slide1.xml\"/></Relationships>");
    let parts: Vec<(EntryMeta, Vec<u8>)> = [
        ("[Content_Types].xml", ct.into_bytes()),
        ("_rels/.rels", root.into_bytes()),
        ("ppt/presentation.xml", pres.into_bytes()),
        ("ppt/_rels/presentation.xml.rels", prels.into_bytes()),
        ("ppt/slides/slide1.xml", slide),
    ]
    .into_iter()
    .map(|(n, d)| (EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, d))
    .collect();
    write_zip(&parts).unwrap()
}

/// The slide XML with one shape whose only paragraph holds `inner`.
fn slide(inner: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sld xmlns:a=\"{A}\" xmlns:p=\"{P}\" xmlns:r=\"{R}\"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"S\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p>{inner}</a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"
    )
}

fn timed(bytes: &[u8]) -> (Result<Document, Error>, Duration) {
    let start = Instant::now();
    let r = Pptx.extract(bytes, &ExtractOptions::default());
    (r, start.elapsed())
}

/// One run followed by 90,000 line breaks: each break used to search the rest of the paragraph for the next text
/// run (about two seconds at 80,000 in a release build, far longer in a debug one). Now one pass.
#[test]
fn a_paragraph_of_many_breaks_builds_in_linear_time() {
    let bytes = deck(slide(&format!("<a:r><a:t>a</a:t></a:r>{}", "<a:br/>".repeat(90_000))).into_bytes());
    let (r, took) = timed(&bytes);
    let doc = r.unwrap();
    assert_eq!(doc.units.len(), 1);
    assert_eq!(doc.units[0].segments[0].source.len(), 90_001);
    assert!(took < Duration::from_secs(5), "took {took:?}");
}

/// One run followed by 90,000 empty runs: each empty run used to look ahead for the next run with text.
#[test]
fn a_paragraph_of_many_empty_runs_builds_in_linear_time() {
    let bytes = deck(slide(&format!("<a:r><a:t>a</a:t></a:r>{}", "<a:r><a:t/></a:r>".repeat(90_000))).into_bytes());
    let (r, took) = timed(&bytes);
    let doc = r.unwrap();
    assert_eq!(doc.units.len(), 1);
    assert_eq!(doc.units[0].segments[0].source, vec![Inline::text("a")]);
    assert!(took < Duration::from_secs(5), "took {took:?}");
}

/// A paragraph with more codes than a unit may have characters is refused with the unit-length limit, before the
/// build (it used to be refused only after it).
#[test]
fn an_oversized_paragraph_is_refused_before_the_build() {
    let bytes = deck(slide(&format!("<a:r><a:t>a</a:t></a:r>{}", "<a:br/>".repeat(140_000))).into_bytes());
    let (r, took) = timed(&bytes);
    match r {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::UnitChars),
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(_) => panic!("expected a limit error"),
    }
    assert!(took < Duration::from_secs(5), "took {took:?}");
}

/// A mixed paragraph keeps its inlines: the tag pair stays open across a break only when the next text has the
/// same formatting.
#[test]
fn a_mixed_paragraph_builds_as_before() {
    let inner = "<a:r><a:t>Plain </a:t></a:r><a:r><a:rPr b=\"1\"/><a:t>bold</a:t></a:r><a:br/><a:r><a:rPr b=\"1\"/><a:t>more</a:t></a:r><a:br/><a:r><a:t> tail of the text</a:t></a:r>";
    let doc = Pptx.extract(&deck(slide(inner).into_bytes()), &ExtractOptions::default()).unwrap();
    let shape: Vec<String> = doc.units[0].segments[0]
        .source
        .iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { kind, .. } => format!("{kind:?}"),
        })
        .collect();
    assert_eq!(shape, ["Plain ", "Open", "bold", "Standalone", "more", "Close", "Standalone", " tail of the text"]);
}

fn locos() -> ExtractOptions {
    ExtractOptions::new().with_profile("locos")
}

/// Two byte order marks, then a character of four bytes before the first run: the spans used to sit three bytes
/// early and the slice landed inside the character (a panic). The part is not XML and is refused.
#[test]
fn a_second_byte_order_mark_is_refused_under_the_locos_profile() {
    let inner = "\u{1F600}<a:r><a:t>one</a:t></a:r><a:r><a:t>two</a:t></a:r>";
    let mut twice = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    twice.extend(slide(inner).into_bytes());
    let bytes = deck(twice);
    let r = std::panic::catch_unwind(|| Pptx.extract(&bytes, &locos()));
    match r.expect("no panic") {
        Err(Error::Parse { .. }) => {}
        Err(e) => panic!("expected a parse error, got {e}"),
        Ok(_) => panic!("a part with two byte order marks was accepted"),
    }
    // the same without the emoji, which used to give run codes cut three bytes early
    let mut twice = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    twice.extend(slide("<a:r><a:t>one</a:t></a:r><a:r><a:t>two</a:t></a:r>").into_bytes());
    assert!(matches!(Pptx.extract(&deck(twice), &locos()), Err(Error::Parse { .. })));

    // one mark, or none, reads as before: the run codes are the run's own tags
    for marks in [0, 1] {
        let mut once = [0xEF, 0xBB, 0xBF].repeat(marks);
        once.extend(slide(inner).into_bytes());
        let doc = Pptx.extract(&deck(once), &locos()).unwrap();
        assert_eq!(doc.units.len(), 1, "{marks} marks");
        let code = &doc.units[0].codes()["1"];
        assert_eq!(code.open_markup(), Some("<a:r>"), "{marks} marks");
        assert_eq!(code.close_markup(), Some("</a:r>"), "{marks} marks");
    }
}
