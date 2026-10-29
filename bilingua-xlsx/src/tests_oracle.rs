//! Regression tests for patterns found by comparing Bilingua with other filters on real-world files.

use crate::fixtures::*;
use crate::*;

fn mini_workbook(sheet_data: &str) -> Vec<u8> {
    let rels = |body: &str| {
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>")
    };
    zip(&[
        (
            "[Content_Types].xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>".to_string(),
        ),
        ("_rels/.rels", rels("<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>")),
        (
            "xl/workbook.xml",
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"{NS}\" xmlns:r=\"{RNS}\"><sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>"),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            rels("<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/>"),
        ),
        (
            "xl/worksheets/sheet1.xml",
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"{NS}\"><sheetData>{sheet_data}</sheetData></worksheet>"),
        ),
    ])
}

fn open(bytes: &[u8]) -> Document {
    extract_with(bytes, &ExtractOptions::default(), &XlsxOptions::default()).unwrap()
}

/// Some writers leave out the `r` attribute of rows and cells. The position then follows the previous one.
#[test]
fn cells_without_r_attributes_are_extracted() {
    let mut rows = String::new();
    for (_, g) in GREETINGS {
        rows.push_str(&format!("<row><c t=\"inlineStr\"><is><t>{g}</t></is></c><c t=\"inlineStr\"><is><t>{g} two</t></is></c></row>"));
    }
    // A mix: a row with r followed by one without, and a cell with r after one without.
    rows.push_str("<row r=\"20\"><c t=\"inlineStr\"><is><t>Twenty</t></is></c><c r=\"D20\" t=\"inlineStr\"><is><t>Skipped to D</t></is></c><c t=\"inlineStr\"><is><t>After D</t></is></c></row>");
    rows.push_str("<row><c t=\"inlineStr\"><is><t>Next row</t></is></c></row>");
    let bytes = mini_workbook(&rows);
    let doc = open(&bytes);
    assert_eq!(doc.units.len(), 11 * 2 + 4);
    let by_loc = |loc: &str| doc.units.iter().find(|u| u.context.location.as_deref() == Some(loc)).map(|u| plain_text(&u.source()));
    assert_eq!(by_loc("Sheet1!A1").as_deref(), Some("Welcome"));
    assert_eq!(by_loc("Sheet1!B2").as_deref(), Some("Bienvenido two"));
    assert_eq!(by_loc("Sheet1!B11").as_deref(), Some("مرحبا بكم two"));
    assert_eq!(by_loc("Sheet1!A20").as_deref(), Some("Twenty"));
    assert_eq!(by_loc("Sheet1!D20").as_deref(), Some("Skipped to D"));
    assert_eq!(by_loc("Sheet1!E20").as_deref(), Some("After D"));
    assert_eq!(by_loc("Sheet1!A21").as_deref(), Some("Next row"));
    // Unchanged merge keeps the file as it was, and a translation reaches the cell.
    let (same, _) = merge_with_report(&doc).unwrap();
    assert_eq!(open(&same).units.len(), doc.units.len());
    let mut t = doc.clone();
    let seg = t.units[0].segments[0].id.clone();
    t.set_target(&seg, vec![Inline::text("ようこそ")]).unwrap();
    let (out, _) = merge_with_report(&t).unwrap();
    assert_eq!(plain_text(&open(&out).units[0].source()), "ようこそ");
}
