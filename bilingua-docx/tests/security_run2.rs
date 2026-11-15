//! Regression tests for the DOCX fixes of security review run 2: the LocOS image inventory and alt-text merge are
//! linear in the drawings, the native writer stamps only a language tag (escaped for either quote), and a merge
//! reads the stored package under the caller's merge limits.

use bilingua_docx::Docx;
use bilingua_model::{plain_text, Document, Error, ExtractOptions, Filter, Inline, LimitKind, Limits, MergeOptions, SegState};
use bilingua_zip::{read_zip, write_zip, EntryMeta, ZipLimits};
use std::sync::mpsc;
use std::time::Duration;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const MAIN_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Runs `f` on its own thread and waits at most `secs` seconds: a quadratic path fails at the deadline.
fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let _ = tx.send(f());
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(secs)).unwrap_or_else(|_| panic!("did not finish within {secs} s"))
}

fn package(body: &str) -> Vec<u8> {
    let ct = format!(
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"{MAIN_CT}\"/></Types>"
    );
    let rels = format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{RT}/officeDocument\" Target=\"word/document.xml\"/></Relationships>"
    );
    let doc = format!("<w:document xmlns:w=\"{W}\" xmlns:wp=\"{WP}\" xmlns:pic=\"{PIC}\"><w:body>{body}</w:body></w:document>");
    let parts: Vec<(EntryMeta, Vec<u8>)> = [("[Content_Types].xml", ct), ("_rels/.rels", rels), ("word/document.xml", doc)]
        .into_iter()
        .map(|(n, d)| (EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, d.into_bytes()))
        .collect();
    write_zip(&parts).unwrap()
}

fn part(zip: &[u8], name: &str) -> String {
    let e = read_zip("docx", zip, &ZipLimits::default()).unwrap();
    String::from_utf8(e.into_iter().find(|e| e.name == name).unwrap().data).unwrap()
}

fn locos() -> ExtractOptions {
    ExtractOptions::new().with_profile("locos")
}

fn translate(u: &mut bilingua_model::Unit, text: &str) {
    u.segments[0].target = vec![Inline::text(text)];
    u.segments[0].state = SegState::Translated;
}

/// 45,000 text paragraphs and 300,000 drawings in a paragraph after them: each drawing was anchored by a walk over
/// the body units and a second walk over all units (about 2.7 x 10^10 steps). Now a binary search and a map; the
/// drawings count on the last text unit.
#[test]
fn docx_locos_many_drawings_one_paragraph_linear() {
    let (paras, n) = (45_000, 300_000);
    let mut body: String = (0..paras).map(|i| format!("<w:p><w:r><w:t>Line {i}</w:t></w:r></w:p>")).collect();
    let drawings: String = (0..n).map(|i| format!("<w:r><w:drawing><wp:inline><wp:docPr id=\"{i}\" name=\"d\" descr=\"\"/></wp:inline></w:drawing></w:r>")).collect();
    body.push_str(&format!("<w:p>{drawings}</w:p>"));
    let bytes = package(&body);
    let doc = within(60, move || Docx.extract(&bytes, &locos()).unwrap());
    let last = doc.units.iter().find(|u| plain_text(&u.segments[0].source).trim() == format!("Line {}", paras - 1)).expect("last paragraph");
    assert_eq!(last.context.extra.get("locos.a11y_images").map(String::as_str), Some(n.to_string().as_str()));
}

/// 20,000 drawings with alt text straight under the body, each with a picture name in the same container: every
/// translated alt text walked the whole body to update the picture's copy (alt units x body size). Now the
/// container is indexed once; each picture still gets its own drawing's new text.
#[test]
fn docx_locos_alt_text_merge_linear() {
    let n = 20_000;
    let body: String = (0..n)
        .map(|i| format!("<w:p><w:r><w:t>Para {i}</w:t></w:r></w:p><wp:docPr id=\"{i}\" name=\"d{i}\" descr=\"Picture {i}\"/><pic:cNvPr id=\"{i}\" name=\"d{i}\" descr=\"Picture {i}\"/>"))
        .collect();
    let bytes = package(&body);
    let out = within(60, move || {
        let mut doc = Docx.extract(&bytes, &locos()).unwrap();
        for u in doc.units.iter_mut().filter(|u| u.context.kind.as_deref() == Some("alt-text")) {
            let t = format!("Bild {}", plain_text(&u.segments[0].source).trim_start_matches("Picture "));
            translate(u, &t);
        }
        Docx.merge_with(&doc, &MergeOptions::new().with_profile("locos")).unwrap()
    });
    let xml = part(&out, "word/document.xml");
    assert!(xml.contains("<wp:docPr id=\"7\" name=\"d7\" descr=\"Bild 7\"/>"), "{}", &xml[..xml.len().min(600)]);
    assert!(xml.contains("<pic:cNvPr id=\"7\" name=\"d7\" descr=\"Bild 7\"/>"));
    assert!(!xml.contains("descr=\"Picture "));
}

/// A picture inside a drawing and an outer container: the drawing's new alt text reaches its own picture only when
/// the picture had the drawing's old value, and a second drawing with the same old value finds nothing left to move.
#[test]
fn docx_locos_alt_text_updates_only_matching_pictures() {
    let body = "<w:p><w:r><w:t>Text</w:t></w:r><w:r><w:drawing><wp:inline><wp:docPr id=\"1\" name=\"a\" descr=\"Old\"/><pic:cNvPr id=\"1\" name=\"a\" descr=\"Old\"/><pic:cNvPr id=\"2\" name=\"b\" descr=\"Other\"/></wp:inline></w:drawing></w:r></w:p>";
    let mut doc = Docx.extract(&package(body), &locos()).unwrap();
    for u in doc.units.iter_mut().filter(|u| u.context.kind.as_deref() == Some("alt-text")) {
        translate(u, "Neu");
    }
    let xml = part(&Docx.merge_with(&doc, &MergeOptions::new().with_profile("locos")).unwrap(), "word/document.xml");
    assert!(xml.contains("<pic:cNvPr id=\"1\" name=\"a\" descr=\"Neu\"/>"), "{xml}");
    assert!(xml.contains("<pic:cNvPr id=\"2\" name=\"b\" descr=\"Other\"/>"), "{xml}");
}

fn native_doc() -> Document {
    let body = "<w:p><w:r><w:rPr><w:lang w:val='en-US'/></w:rPr><w:t>Hello world</w:t></w:r></w:p>";
    let mut doc = Docx.extract(&package(body), &ExtractOptions::new()).unwrap();
    translate(&mut doc.units[0], "Hallo Welt");
    doc
}

/// The native writer stamps only a value that can be a language tag, in its canonical form, and escapes the value
/// for the quote the attribute uses.
#[test]
fn docx_pptx_native_lang_stamp_validated() {
    let mut doc = native_doc();
    doc.target_lang = Some("de' w:eastAsia='x".into());
    let xml = part(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(xml.contains("Hallo Welt"));
    assert!(!xml.contains("w:eastAsia"), "{xml}");
    assert!(xml.contains("w:val='en-US'"), "{xml}");
    let mut doc = native_doc();
    doc.target_lang = Some("pt_br".into());
    let xml = part(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(xml.contains("w:val='pt-BR'"), "{xml}");
}

/// The LocOS merge reads the stored package under the caller's merge limits.
#[test]
fn office_merge_uses_caller_limits() {
    let body: String = (0..300).map(|i| format!("<w:p><w:r><w:t>Line {i}</w:t></w:r></w:p>")).collect();
    let mut doc = Docx.extract(&package(&body), &locos()).unwrap();
    translate(&mut doc.units[0], "Zeile");
    let tight = MergeOptions::new().with_profile("locos").with_limits(Limits::new().with_max_xml_elements(100));
    match Docx.merge_with(&doc, &tight) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlElements),
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(_) => panic!("the merge ignored the caller's element cap"),
    }
    assert!(part(&Docx.merge_with(&doc, &MergeOptions::new().with_profile("locos")).unwrap(), "word/document.xml").contains("Zeile"));
}
