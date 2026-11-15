//! Regression tests for the XLSX fixes of security review run 2 and BILI-50: a repeated cell reference keeps both
//! units' translations, a LocOS merge reads the stored package under the caller's merge limits, and a workbook
//! outside the SpreadsheetML namespace is refused instead of opening with no units.

use bilingua_model::{plain_text, Error, ExtractOptions, Filter, Inline, LimitKind, Limits, MergeOptions, SegState, Unit};
use bilingua_xlsx::Xlsx;
use bilingua_zip::{read_zip, write_zip, EntryMeta, ZipLimits};

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const BETA: &str = "http://schemas.microsoft.com/office/excel/2006/2";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// A one-sheet workbook in namespace `ns` whose `sheetData` is `rows`.
fn workbook(ns: &str, rows: &str) -> Vec<u8> {
    let ct = "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>".to_string();
    let root = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>");
    let wb = format!("<workbook xmlns=\"{ns}\" xmlns:r=\"{R}\"><sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>");
    let wrels = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>");
    let sheet = format!("<worksheet xmlns=\"{ns}\"><sheetData>{rows}</sheetData></worksheet>");
    let parts: Vec<(EntryMeta, Vec<u8>)> = [("[Content_Types].xml", ct), ("_rels/.rels", root), ("xl/workbook.xml", wb), ("xl/_rels/workbook.xml.rels", wrels), ("xl/worksheets/sheet1.xml", sheet)]
        .into_iter()
        .map(|(n, d)| (EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, d.into_bytes()))
        .collect();
    write_zip(&parts).unwrap()
}

fn sheet_xml(zip: &[u8]) -> String {
    let e = read_zip("xlsx", zip, &ZipLimits::default()).unwrap();
    String::from_utf8(e.into_iter().find(|e| e.name == "xl/worksheets/sheet1.xml").unwrap().data).unwrap()
}

fn translate(u: &mut Unit, text: &str) {
    u.segments[0].target = vec![Inline::text(text)];
    u.segments[0].state = SegState::Translated;
}

fn inline(r: &str, text: &str) -> String {
    format!("<c r=\"{r}\" t=\"inlineStr\"><is><t>{text}</t></is></c>")
}

/// Two inline-string cells with the same reference: both are units with their own ids, and each one's translation
/// is written into its own cell (before, both markers named the later unit and the first translation was lost).
#[test]
fn xlsx_repeated_cell_ref_unique_ids() {
    let bytes = workbook(NS, &format!("<row r=\"1\">{}{}</row>", inline("A1", "First text"), inline("A1", "Second text")));
    let mut doc = Xlsx::default().extract(&bytes, &ExtractOptions::new()).unwrap();
    let ids: Vec<&str> = doc.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids.len(), 2, "{ids:?}");
    assert_ne!(ids[0], ids[1]);
    for u in doc.units.iter_mut() {
        let t = format!("{} DE", plain_text(&u.segments[0].source));
        translate(u, &t);
    }
    let xml = sheet_xml(&Xlsx::default().merge(&doc).unwrap());
    assert!(xml.contains("First text DE") && xml.contains("Second text DE"), "{xml}");
}

/// The LocOS merge reads the stored package under the caller's merge limits.
#[test]
fn office_merge_uses_caller_limits() {
    let rows: String = (1..=100).map(|i| format!("<row r=\"{i}\">{}</row>", inline(&format!("A{i}"), &format!("Cell {i}")))).collect();
    let locos = ExtractOptions::new().with_profile("locos");
    let mut doc = Xlsx::default().extract(&workbook(NS, &rows), &locos).unwrap();
    translate(&mut doc.units[0], "Zelle");
    let tight = MergeOptions::new().with_profile("locos").with_limits(Limits::new().with_max_xml_elements(50));
    match Xlsx::default().merge_with(&doc, &tight) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlElements),
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(_) => panic!("the merge ignored the caller's element cap"),
    }
    assert!(sheet_xml(&Xlsx::default().merge_with(&doc, &MergeOptions::new().with_profile("locos")).unwrap()).contains("Zelle"));
}

fn refused(r: Result<bilingua_model::Document, Error>) {
    match r {
        Err(Error::Parse { msg, .. }) => assert!(msg.contains("save it as .xlsx"), "{msg}"),
        Err(e) => panic!("expected a parse error, got {e}"),
        Ok(d) => panic!("a beta-namespace workbook opened with {} units", d.units.len()),
    }
}

/// BILI-50: a workbook in the Office 2007 beta namespace opened with no units and no word where names are read by
/// namespace (the LocOS profile, and a prefixed workbook root on the native path). It is refused there with a
/// message that says how to fix it; the native reader, which matches unprefixed names, still reads an unprefixed
/// one; the same file in the SpreadsheetML namespace opens on both paths.
#[test]
fn a_beta_namespace_workbook_is_refused_not_empty() {
    let rows = format!("<row r=\"1\">{}</row>", inline("A1", "Hello there"));
    let locos = ExtractOptions::new().with_profile("locos");
    refused(Xlsx::default().extract(&workbook(BETA, &rows), &locos));
    assert_eq!(Xlsx::default().extract(&workbook(BETA, &rows), &ExtractOptions::new()).unwrap().units.len(), 1);
    // a prefixed root: `x:workbook` in the beta namespace
    let prefixed = {
        let e = read_zip("xlsx", &workbook(BETA, &rows), &ZipLimits::default()).unwrap();
        let parts: Vec<(EntryMeta, Vec<u8>)> = e
            .into_iter()
            .map(|e| {
                let data = if e.name == "xl/workbook.xml" {
                    String::from_utf8(e.data).unwrap().replace("<workbook xmlns=", "<x:workbook xmlns:x=").replace("</workbook>", "</x:workbook>").replace("<sheets>", "<x:sheets>").replace("</sheets>", "</x:sheets>").replace("<sheet ", "<x:sheet ").into_bytes()
                } else {
                    e.data
                };
                (EntryMeta { name: e.name, stored: false, dir: false, modified: None }, data)
            })
            .collect();
        write_zip(&parts).unwrap()
    };
    refused(Xlsx::default().extract(&prefixed, &ExtractOptions::new()));
    for opts in [ExtractOptions::new(), locos] {
        assert_eq!(Xlsx::default().extract(&workbook(NS, &rows), &opts).unwrap().units.len(), 1);
    }
}
