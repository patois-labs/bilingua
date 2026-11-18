//! BILI-116: which formatting of a paragraph is the base (the one that needs no tag). A translator reads the
//! plain run as untagged text and the bold or italic run as tagged, whatever run carries the most text. The deck
//! of the report: "[b]Bold claim[/b] with an [i]italic aside[/i]", where the italic run is the longest and
//! `a:endParaRPr` copies its italic, as PowerPoint writes it.

use bilingua_model::{Document, ExtractOptions, Filter, Inline};
use bilingua_pptx::Pptx;
use bilingua_zip::{read_zip, write_zip, EntryMeta, ZipLimits};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// A one-slide deck whose one shape holds the paragraph `inner`.
fn deck(inner: &str) -> Vec<u8> {
    let ct = "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slides/slide1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/></Types>".to_string();
    let root = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>");
    let pres = format!("<p:presentation xmlns:r=\"{R}\" xmlns:p=\"{P}\"><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId1\"/></p:sldIdLst></p:presentation>");
    let prels = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/slide\" Target=\"slides/slide1.xml\"/></Relationships>");
    let slide = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sld xmlns:a=\"{A}\" xmlns:p=\"{P}\" xmlns:r=\"{R}\"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"S\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p>{inner}</a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"
    );
    let parts: Vec<(EntryMeta, Vec<u8>)> = [
        ("[Content_Types].xml", ct),
        ("_rels/.rels", root),
        ("ppt/presentation.xml", pres),
        ("ppt/_rels/presentation.xml.rels", prels),
        ("ppt/slides/slide1.xml", slide),
    ]
    .into_iter()
    .map(|(n, d)| (EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, d.into_bytes()))
    .collect();
    write_zip(&parts).unwrap()
}

fn run(rpr: &str, text: &str) -> String {
    format!("<a:r><a:rPr lang=\"en-US\"{rpr} dirty=\"0\"/><a:t>{text}</a:t></a:r>")
}

fn extract(bytes: &[u8]) -> Document {
    Pptx.extract(bytes, &ExtractOptions::new().with_target_lang("es-ES")).unwrap()
}

/// The source of the deck's only unit, text as `T:..` and tags by their display.
fn shown(doc: &Document) -> Vec<String> {
    assert_eq!(doc.units.len(), 1);
    doc.units[0]
        .source()
        .iter()
        .map(|i| match i {
            Inline::Text(t) => format!("T:{t}"),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

fn assert_same_parts(a: &[u8], b: &[u8]) {
    let ea = read_zip("pptx", a, &ZipLimits::default()).unwrap();
    let eb = read_zip("pptx", b, &ZipLimits::default()).unwrap();
    assert_eq!(ea.len(), eb.len());
    for (x, y) in ea.iter().zip(eb.iter()) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.data, y.data, "{} changed", x.name);
    }
}

fn slide_xml(bytes: &[u8]) -> String {
    let es = read_zip("pptx", bytes, &ZipLimits::default()).unwrap();
    String::from_utf8(es.into_iter().find(|e| e.name == "ppt/slides/slide1.xml").unwrap().data).unwrap()
}

/// The deck of the report: the plain run is the base, bold and italic are tagged.
#[test]
fn the_plain_run_is_the_base_though_the_italic_run_is_longer() {
    let inner = format!(
        "{}{}{}<a:endParaRPr lang=\"en-US\" i=\"1\" dirty=\"0\"/>",
        run(" b=\"1\"", "Bold claim"),
        run("", " with an "),
        run(" i=\"1\"", "italic aside")
    );
    let bytes = deck(&inner);
    let doc = extract(&bytes);
    assert_eq!(shown(&doc), vec!["<b>", "T:Bold claim", "</b>", "T: with an ", "<i>", "T:italic aside", "</i>"]);
    let u = &doc.units[0];
    let base = u.codes()["_base"].standalone_markup().map(String::from).unwrap_or_default();
    assert!(!base.contains("b=\"1\"") && !base.contains("i=\"1\""), "{base}");
    // An unchanged deck comes back with the same bytes.
    assert_same_parts(&bytes, &Pptx.merge(&doc).unwrap());

    // A translation that moves the tags keeps each run's formatting: the plain text is written plain.
    let mut doc = doc;
    let src = doc.units[0].source();
    let tag = |n: usize| src.iter().filter(|i| matches!(i, Inline::Tag { .. })).nth(n).unwrap().clone();
    let target = vec![
        Inline::text("Una "),
        tag(2),
        Inline::text("nota en cursiva"),
        tag(3),
        Inline::text(" con "),
        tag(0),
        Inline::text("afirmación audaz"),
        tag(1),
    ];
    let seg = doc.units[0].segments[0].id.clone();
    doc.set_target(&seg, target).unwrap();
    let out = Pptx.merge(&doc).unwrap();
    let x = slide_xml(&out);
    assert!(x.contains("<a:rPr lang=\"es-ES\"/><a:t>Una </a:t>"), "{x}");
    assert!(x.contains("<a:rPr lang=\"es-ES\" i=\"1\"/><a:t>nota en cursiva</a:t>"), "{x}");
    assert!(x.contains("<a:rPr lang=\"es-ES\" b=\"1\"/><a:t>afirmación audaz</a:t>"), "{x}");
    let again = extract(&out);
    assert_eq!(
        shown(&again),
        vec!["T:Una ", "<i>", "T:nota en cursiva", "</i>", "T: con ", "<b>", "T:afirmación audaz", "</b>"]
    );
}

/// A size or font of its own does not make a run formatted: the run with only `sz` is still the plain one.
#[test]
fn a_run_with_only_a_size_is_plain() {
    let inner = format!(
        "{}{}{}<a:endParaRPr lang=\"en-US\" sz=\"2400\" i=\"1\" dirty=\"0\"/>",
        run(" sz=\"2400\" b=\"1\"", "Bold"),
        run(" sz=\"2400\"", " and "),
        run(" sz=\"2400\" i=\"1\"", "a much longer italic part")
    );
    let bytes = deck(&inner);
    let doc = extract(&bytes);
    assert_eq!(shown(&doc), vec!["<b>", "T:Bold", "</b>", "T: and ", "<i>", "T:a much longer italic part", "</i>"]);
    assert_same_parts(&bytes, &Pptx.merge(&doc).unwrap());
}

/// No plain run: the paragraph default (`a:endParaRPr`) is the base, then the most text.
#[test]
fn without_a_plain_run_the_paragraph_default_then_the_most_text_is_the_base() {
    let bold_end = format!(
        "{}{}<a:endParaRPr lang=\"en-US\" b=\"1\" dirty=\"0\"/>",
        run(" b=\"1\"", "Short"),
        run(" i=\"1\"", " and a longer italic part")
    );
    let bytes = deck(&bold_end);
    let doc = extract(&bytes);
    assert_eq!(shown(&doc), vec!["T:Short", "<i>", "T: and a longer italic part", "</i>"]);
    assert_same_parts(&bytes, &Pptx.merge(&doc).unwrap());

    let no_end = format!("{}{}", run(" b=\"1\"", "Short"), run(" i=\"1\"", " and a longer italic part"));
    let bytes = deck(&no_end);
    let doc = extract(&bytes);
    assert_eq!(shown(&doc), vec!["<b>", "T:Short", "</b>", "T: and a longer italic part"]);
    assert_same_parts(&bytes, &Pptx.merge(&doc).unwrap());
}

/// Of two plain formattings, the one equal to the paragraph default is the base, before the most text.
#[test]
fn of_two_plain_formattings_the_paragraph_default_wins() {
    let inner = format!(
        "{}{}<a:endParaRPr lang=\"en-US\" sz=\"1800\" dirty=\"0\"/>",
        run(" sz=\"2400\"", "A long run in a larger size"),
        run(" sz=\"1800\"", " then small")
    );
    let bytes = deck(&inner);
    let doc = extract(&bytes);
    assert_eq!(shown(&doc), vec!["<fmt>", "T:A long run in a larger size", "</fmt>", "T: then small"]);
    assert_same_parts(&bytes, &Pptx.merge(&doc).unwrap());
}

/// A bold paragraph with a plain space at the end has no tags: the space takes the bold of its neighbour.
#[test]
fn a_plain_space_does_not_become_the_base() {
    let inner = format!("{}{}", run(" b=\"1\"", "All of this is bold"), run("", " "));
    let bytes = deck(&inner);
    let doc = extract(&bytes);
    assert_eq!(shown(&doc), vec!["T:All of this is bold "]);
    assert_same_parts(&bytes, &Pptx.merge(&doc).unwrap());
}

/// Strike counts against plain, as in DOCX: the plain run is the base though the struck run is longer. The tag
/// keeps its `<fmt>` display and is labelled "strike".
#[test]
fn a_struck_run_is_not_plain() {
    let inner = format!("{}{}", run(" strike=\"sngStrike\"", "a long struck-through phrase"), run("", " and plain"));
    let bytes = deck(&inner);
    let doc = extract(&bytes);
    assert_eq!(shown(&doc), vec!["<fmt>", "T:a long struck-through phrase", "</fmt>", "T: and plain"]);
    assert_eq!(doc.units[0].codes()["1"].label(), Some("strike"));
    assert_same_parts(&bytes, &Pptx.merge(&doc).unwrap());
}
