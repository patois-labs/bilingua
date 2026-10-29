//! Regression tests for patterns found by comparing Bilingua with other filters on sample files.

use crate::fixtures::*;
use crate::zipio::{read_zip, ZipLimits};
use crate::*;

fn with_slide1(slide: String) -> Vec<u8> {
    let base = presentation_zip();
    let entries = read_zip(&base, &ZipLimits::default()).unwrap();
    let files: Vec<(String, String)> = entries
        .iter()
        .map(|e| {
            let text = String::from_utf8(e.data.clone()).unwrap();
            (e.name.clone(), if e.name == "ppt/slides/slide1.xml" { slide.clone() } else { text })
        })
        .collect();
    let refs: Vec<(&str, String)> = files.iter().map(|(n, t)| (n.as_str(), t.clone())).collect();
    zip(&refs)
}

fn cell(text: &str, attr: &str) -> String {
    format!("<a:tc{attr}><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{text}</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>")
}

/// The cells that a merged range covers (`hMerge`, `vMerge`) are not shown, so they are not units. Their
/// leftover text must not move the ids of the units after them.
#[test]
fn covered_cells_of_merged_ranges_are_not_units() {
    let rows = [
        format!("<a:tr h=\"100\">{}{}{}</a:tr>", cell("Merged title", " gridSpan=\"2\""), cell("hidden leftover", " hMerge=\"1\""), cell("Side", "")),
        format!("<a:tr h=\"100\">{}{}{}</a:tr>", cell("Tall", " rowSpan=\"2\""), cell("Right", ""), cell("Edge", "")),
        format!("<a:tr h=\"100\">{}{}{}</a:tr>", cell("covered below", " vMerge=\"1\""), cell("Last", ""), cell("End", "")),
    ]
    .concat();
    let table = format!("<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"9\" name=\"Table 8\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblGrid><a:gridCol w=\"1\"/><a:gridCol w=\"1\"/><a:gridCol w=\"1\"/></a:tblGrid>{rows}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>");
    let slide = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sld xmlns:a=\"{NS_A}\" xmlns:p=\"{NS_P}\" xmlns:r=\"{NS_R}\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{table}</p:spTree></p:cSld></p:sld>"
    );
    let bytes = with_slide1(slide);
    let doc = Pptx.extract(&bytes, &ExtractOptions::default()).unwrap();
    let texts: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
    assert!(texts.contains(&"Merged title".to_string()) && texts.contains(&"Tall".to_string()), "{texts:?}");
    assert!(!texts.iter().any(|t| t.contains("leftover") || t.contains("covered")), "{texts:?}");
    // ids count every paragraph, covered or not: "Side" is the third paragraph of the table
    let side = doc.units.iter().find(|u| plain_text(&u.source()) == "Side").unwrap();
    assert!(side.id.ends_with("-p3"), "{}", side.id);
    let out = Pptx.merge(&doc).unwrap();
    assert_eq!(read_zip(&out, &ZipLimits::default()).unwrap().len(), read_zip(&bytes, &ZipLimits::default()).unwrap().len());
}
