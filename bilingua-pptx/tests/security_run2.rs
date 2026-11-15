//! Regression tests for the PPTX fixes of security review run 2: SmartArt lookups by modelId are hashed, the
//! native writer stamps only a language tag (escaped for either quote), and a LocOS merge reads the stored package
//! under the caller's merge limits.

use bilingua_model::{plain_text, Error, ExtractOptions, Filter, Inline, LimitKind, Limits, MergeOptions, SegState, Unit};
use bilingua_pptx::Pptx;
use bilingua_zip::{read_zip, write_zip, EntryMeta, ZipLimits};
use std::sync::mpsc;
use std::time::Duration;

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const DGM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
const DSP: &str = "http://schemas.microsoft.com/office/drawing/2008/diagram";

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

/// A one-slide deck with `slide` and any more parts.
fn deck(slide: String, more: Vec<(&str, String)>) -> Vec<u8> {
    let ct = "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slides/slide1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/></Types>".to_string();
    let root = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>");
    let pres = format!("<p:presentation xmlns:r=\"{R}\" xmlns:p=\"{P}\"><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId1\"/></p:sldIdLst></p:presentation>");
    let prels = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/slide\" Target=\"slides/slide1.xml\"/></Relationships>");
    let mut parts = vec![
        ("[Content_Types].xml", ct),
        ("_rels/.rels", root),
        ("ppt/presentation.xml", pres),
        ("ppt/_rels/presentation.xml.rels", prels),
        ("ppt/slides/slide1.xml", slide),
    ];
    parts.extend(more);
    let parts: Vec<(EntryMeta, Vec<u8>)> = parts.into_iter().map(|(n, d)| (EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, d.into_bytes())).collect();
    write_zip(&parts).unwrap()
}

fn slide(inner: &str) -> String {
    format!(
        "<p:sld xmlns:a=\"{A}\" xmlns:p=\"{P}\" xmlns:r=\"{R}\" xmlns:dgm=\"{DGM}\"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"S\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p>{inner}</a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"
    )
}

fn part(zip: &[u8], name: &str) -> String {
    let e = read_zip("pptx", zip, &ZipLimits::default()).unwrap();
    String::from_utf8(e.into_iter().find(|e| e.name == name).unwrap().data).unwrap()
}

fn translate(u: &mut Unit, text: &str) {
    u.segments[0].target = vec![Inline::text(text)];
    u.segments[0].state = SegState::Translated;
}

fn locos() -> ExtractOptions {
    ExtractOptions::new().with_profile("locos")
}

/// A SmartArt diagram of 150,000 points with distinct modelIds and a cached drawing of 150,000 shapes: the merge
/// looked each point and each shape up by a scan (about 4 x 10^10 compares). Now by hash; the translated point's
/// cached shape gets the translation.
#[test]
fn pptx_locos_many_smartart_points_merge_linear() {
    let n = 150_000;
    let mut pts: String = (0..n).map(|i| format!("<dgm:pt modelId=\"m{i}\"><dgm:t/></dgm:pt>")).collect();
    pts.push_str("<dgm:pt modelId=\"hello\"><dgm:t><a:bodyPr/><a:p><a:r><a:t>Hello there</a:t></a:r></a:p></dgm:t></dgm:pt>");
    let data = format!(
        "<dgm:dataModel xmlns:dgm=\"{DGM}\" xmlns:a=\"{A}\" xmlns:dsp=\"{DSP}\"><dgm:ptLst>{pts}</dgm:ptLst><dgm:extLst><a:ext uri=\"x\"><dsp:dataModelExt relId=\"rId3\"/></a:ext></dgm:extLst></dgm:dataModel>"
    );
    let mut sps: String = (0..n).map(|i| format!("<dsp:sp modelId=\"m{i}\"><dsp:txBody/></dsp:sp>")).collect();
    sps.push_str("<dsp:sp modelId=\"hello\"><dsp:txBody><a:bodyPr/><a:p><a:r><a:t>Hello there</a:t></a:r></a:p></dsp:txBody></dsp:sp>");
    let drawing = format!("<dsp:drawing xmlns:dsp=\"{DSP}\" xmlns:a=\"{A}\"><dsp:spTree>{sps}</dsp:spTree></dsp:drawing>");
    let srels = format!(
        "<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId2\" Type=\"{R}/diagramData\" Target=\"../diagrams/data1.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.microsoft.com/office/2007/relationships/diagramDrawing\" Target=\"../diagrams/drawing1.xml\"/></Relationships>"
    );
    let sl = slide("<a:r><a:t>Title</a:t></a:r></a:p></p:txBody></p:sp><p:graphicFrame><a:graphic><a:graphicData><dgm:relIds r:dm=\"rId2\"/></a:graphicData></a:graphic></p:graphicFrame><p:sp><p:txBody><a:p>");
    let bytes = deck(sl, vec![("ppt/slides/_rels/slide1.xml.rels", srels), ("ppt/diagrams/data1.xml", data), ("ppt/diagrams/drawing1.xml", drawing)]);
    let out = within(60, move || {
        let mut doc = Pptx.extract(&bytes, &locos()).unwrap();
        let u = doc.units.iter_mut().find(|u| plain_text(&u.segments[0].source) == "Hello there").expect("SmartArt unit");
        translate(u, "Hallo zusammen");
        Pptx.merge_with(&doc, &MergeOptions::new().with_profile("locos")).unwrap()
    });
    assert!(part(&out, "ppt/diagrams/data1.xml").contains("Hallo zusammen"));
    assert!(part(&out, "ppt/diagrams/drawing1.xml").contains("Hallo zusammen"));
}

/// The native writer stamps only a value that can be a language tag, in its canonical form, and escapes it for
/// the quote the attribute uses.
#[test]
fn docx_pptx_native_lang_stamp_validated() {
    let bytes = deck(slide("<a:r><a:rPr lang='en-US'/><a:t>Hello world</a:t></a:r>"), vec![]);
    for (target, want, banned) in [("de' b='1", "lang='en-US'", " b='1"), ("pt_br", "lang='pt-BR'", "lang='en-US'")] {
        let mut doc = Pptx.extract(&bytes, &ExtractOptions::new()).unwrap();
        translate(&mut doc.units[0], "Hallo Welt");
        doc.target_lang = Some(target.into());
        let xml = part(&Pptx.merge(&doc).unwrap(), "ppt/slides/slide1.xml");
        assert!(xml.contains("Hallo Welt"), "{xml}");
        assert!(xml.contains(want), "{target}: {xml}");
        assert!(!xml.contains(banned), "{target}: {xml}");
    }
}

/// The LocOS merge reads the stored package under the caller's merge limits.
#[test]
fn office_merge_uses_caller_limits() {
    let runs: String = (0..200).map(|i| format!("<a:r><a:t>Word {i} </a:t></a:r>")).collect();
    let mut doc = Pptx.extract(&deck(slide(&runs), vec![]), &locos()).unwrap();
    translate(&mut doc.units[0], "Wort");
    let tight = MergeOptions::new().with_profile("locos").with_limits(Limits::new().with_max_xml_elements(100));
    match Pptx.merge_with(&doc, &tight) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlElements),
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(_) => panic!("the merge ignored the caller's element cap"),
    }
    assert!(part(&Pptx.merge_with(&doc, &MergeOptions::new().with_profile("locos")).unwrap(), "ppt/slides/slide1.xml").contains("Wort"));
}
