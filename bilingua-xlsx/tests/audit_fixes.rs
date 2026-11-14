//! Regression tests for the XLSX fixes of the 0.9.1 security audit: a comment's note goes to one cell and counts
//! against the context budget, a worksheet part named by many `sheet` elements is read once, the spreadsheet limits
//! apply under the LocOS profile and in column mode before cell text is copied, and building a rich string's
//! inlines is linear in its runs.

use bilingua_model::{Document, Error, ExtractOptions, Filter, Inline, LimitKind, Limits};
use bilingua_xlsx::Xlsx;
use bilingua_zip::{write_zip, EntryMeta};
use std::time::{Duration, Instant};

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

fn zip(entries: Vec<(&str, String)>) -> Vec<u8> {
    let parts: Vec<(EntryMeta, Vec<u8>)> = entries
        .into_iter()
        .map(|(n, d)| (EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, d.into_bytes()))
        .collect();
    write_zip(&parts).unwrap()
}

/// A workbook with `sheets` sheet elements that all name the one worksheet part, whose `sheetData` is `rows`;
/// `sst` is the shared string items and `comment` the text of a comment on A1.
fn workbook(sheets: usize, rows: &str, sst: Option<&str>, comment: Option<&str>) -> Vec<u8> {
    let ct = "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>".to_string();
    let root = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>");
    let sheet_els: String = (1..=sheets).map(|i| format!("<sheet name=\"Sheet{i}\" sheetId=\"{i}\" r:id=\"rId1\"/>")).collect();
    let wb = format!("<workbook xmlns=\"{NS}\" xmlns:r=\"{R}\"><sheets>{sheet_els}</sheets></workbook>");
    let wrels = format!(
        "<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/worksheet\" Target=\"worksheets/sheet1.xml\"/><Relationship Id=\"rId2\" Type=\"{R}/sharedStrings\" Target=\"sharedStrings.xml\"/></Relationships>"
    );
    let sheet = format!("<worksheet xmlns=\"{NS}\"><sheetData>{rows}</sheetData></worksheet>");
    let mut entries = vec![
        ("[Content_Types].xml", ct),
        ("_rels/.rels", root),
        ("xl/workbook.xml", wb),
        ("xl/_rels/workbook.xml.rels", wrels),
        ("xl/worksheets/sheet1.xml", sheet),
    ];
    if let Some(items) = sst {
        entries.push(("xl/sharedStrings.xml", format!("<sst xmlns=\"{NS}\">{items}</sst>")));
    }
    if let Some(text) = comment {
        entries.push((
            "xl/worksheets/_rels/sheet1.xml.rels",
            format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/comments\" Target=\"../comments1.xml\"/></Relationships>"),
        ));
        entries.push((
            "xl/comments1.xml",
            format!("<comments xmlns=\"{NS}\"><authors><author>A</author></authors><commentList><comment ref=\"A1\" authorId=\"0\"><text><t>{text}</t></text></comment></commentList></comments>"),
        ));
    }
    zip(entries)
}

fn extract(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    Xlsx::default().extract(bytes, opts)
}

fn limit_kind(r: Result<Document, Error>) -> LimitKind {
    match r {
        Err(Error::Limit(e)) => e.kind,
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(d) => panic!("expected a limit error, the file was accepted with {} units", d.units.len()),
    }
}

fn note_bytes(doc: &Document) -> usize {
    doc.units.iter().flat_map(|u| u.context.notes.iter()).map(String::len).sum()
}

fn locos() -> ExtractOptions {
    ExtractOptions::new().with_profile("locos")
}

/// 200 cells that all say they are A1, under one long comment on A1: the note used to be copied onto every one
/// of the 200 units. It belongs to one cell.
#[test]
fn a_comment_note_goes_to_one_cell() {
    let comment = "x".repeat(5000);
    let cells: String = (0..200).map(|i| format!("<c r=\"A1\" t=\"s\"><v>{i}</v></c>")).collect();
    let sst: String = (0..200).map(|i| format!("<si><t>word {i}</t></si>")).collect();
    let bytes = workbook(1, &format!("<row r=\"1\">{cells}</row>"), Some(&sst), Some(&comment));
    let doc = extract(&bytes, &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.len(), 200);
    assert_eq!(doc.units.iter().filter(|u| !u.context.notes.is_empty()).count(), 1);
    assert!(note_bytes(&doc) < 6000, "{}", note_bytes(&doc));
    // the same with inline strings
    let cells: String = (0..200).map(|i| format!("<c r=\"A1\" t=\"inlineStr\"><is><t>word {i}</t></is></c>")).collect();
    let doc = extract(&workbook(1, &format!("<row r=\"1\">{cells}</row>"), None, Some(&comment)), &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.iter().filter(|u| !u.context.notes.is_empty()).count(), 1);
    assert!(note_bytes(&doc) < 6000, "{}", note_bytes(&doc));
}

/// The note on a unit counts against `max_context_bytes`, for a shared-string cell and for an inline one.
#[test]
fn a_comment_note_is_charged_to_the_context_budget() {
    let comment = "x".repeat(5000);
    let tight = ExtractOptions::new().with_limits(Limits::new().with_max_context_bytes(1000));
    let shared = workbook(1, "<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c></row>", Some("<si><t>word</t></si>"), Some(&comment));
    assert_eq!(limit_kind(extract(&shared, &tight)), LimitKind::Context);
    let inline = workbook(1, "<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>word</t></is></c></row>", None, Some(&comment));
    assert_eq!(limit_kind(extract(&inline, &tight)), LimitKind::Context);
    // an ordinary budget takes it, with the note on the cell's unit
    let doc = extract(&shared, &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.len(), 1);
    assert!(doc.units[0].context.notes[0].starts_with("Comment (A): xxx"));
}

/// 300 `sheet` elements that name one worksheet part: the part, its relationships and its comments are read once,
/// so there is one cell unit and one copy of the note, not 300. The sheet-name units keep one per element.
#[test]
fn a_worksheet_named_many_times_is_read_once() {
    let comment = "x".repeat(5000);
    let rows = "<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>Hello</t></is></c></row>";
    let bytes = workbook(300, rows, None, Some(&comment));
    let doc = extract(&bytes, &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.len(), 1);
    assert_eq!(doc.units[0].id, "is1-A1");
    assert!(note_bytes(&doc) < 6000, "{}", note_bytes(&doc));
    let named = extract(&bytes, &ExtractOptions::new().with_format_option("xlsx.translate_sheet_names", true)).unwrap();
    let names: Vec<&str> = named.units.iter().filter(|u| u.id.starts_with("sheetname-")).map(|u| u.id.as_str()).collect();
    assert_eq!(names.len(), 300);
    assert_eq!((names[0], names[299]), ("sheetname-1", "sheetname-300"));
    assert_eq!(named.units.len(), 301);

    // a workbook with two sheets on two parts is untouched: this one has a single sheet element and reads the same
    let one = extract(&workbook(1, rows, None, Some(&comment)), &ExtractOptions::new()).unwrap();
    assert_eq!(one.units.len(), 1);
    assert_eq!(one.units[0].context.notes, doc.units[0].context.notes);
}

/// 400 cells that point at one shared string of 1000 letters.
fn repeated_shared_string() -> Vec<u8> {
    let rows: String = (1..=400).map(|r| format!("<row r=\"{r}\"><c r=\"A{r}\" t=\"s\"><v>0</v></c></row>")).collect();
    workbook(1, &rows, Some(&format!("<si><t>{}</t></si>", "a".repeat(1000))), None)
}

/// The four spreadsheet limits and the unit cap under the LocOS profile, where none of the four applied before
/// (the file gave 400 units whatever they were set to) and the unit cap came after every value was copied.
#[test]
fn the_spreadsheet_limits_apply_under_the_locos_profile() {
    let bytes = repeated_shared_string();
    let cases = [
        ("cells", Limits::new().with_xlsx_max_cells(10)),
        ("rows", Limits::new().with_xlsx_max_rows(10)),
        ("cell chars", Limits::new().with_xlsx_max_cell_chars(10)),
        ("total text", Limits::new().with_xlsx_max_total_text_chars(10_000)),
    ];
    for (what, l) in cases {
        assert_eq!(limit_kind(extract(&bytes, &locos().with_limits(l))), LimitKind::Spreadsheet, "locos, {what}");
        // the native walk refuses the same file for the same limit
        assert_eq!(limit_kind(extract(&bytes, &ExtractOptions::new().with_limits(l))), LimitKind::Spreadsheet, "native, {what}");
    }
    assert_eq!(limit_kind(extract(&bytes, &locos().with_limits(Limits::new().with_max_units(50)))), LimitKind::Units);
    // inside the limits the walk is LocOS's: one unit per cell
    let doc = extract(&bytes, &locos()).unwrap();
    assert_eq!(doc.units.len(), 400);
    // an inline string counts too
    let inline = workbook(1, &format!("<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>{}</t></is></c></row>", "a".repeat(50)), None, None);
    assert_eq!(limit_kind(extract(&inline, &locos().with_limits(Limits::new().with_xlsx_max_cell_chars(10)))), LimitKind::Spreadsheet);
    assert_eq!(extract(&inline, &locos()).unwrap().units.len(), 1);
}

/// The same limits in column mode, with and without the profile.
#[test]
fn the_spreadsheet_limits_apply_in_column_mode() {
    let bytes = repeated_shared_string();
    let cases = [
        ("cells", Limits::new().with_xlsx_max_cells(10)),
        ("rows", Limits::new().with_xlsx_max_rows(10)),
        ("cell chars", Limits::new().with_xlsx_max_cell_chars(10)),
        ("total text", Limits::new().with_xlsx_max_total_text_chars(10_000)),
    ];
    for (path, base) in [("columns", ExtractOptions::new()), ("columns + locos", locos())] {
        let columns = base.with_format_option("xlsx.columns", "source=1, header=no");
        for (what, l) in cases {
            assert_eq!(limit_kind(extract(&bytes, &columns.clone().with_limits(l))), LimitKind::Spreadsheet, "{path}, {what}");
        }
        assert_eq!(limit_kind(extract(&bytes, &columns.clone().with_limits(Limits::new().with_max_units(50)))), LimitKind::Units, "{path}");
        assert_eq!(extract(&bytes, &columns).unwrap().units.len(), 400, "{path}");
    }
}

/// A rich string of one run and 90,000 empty runs: each empty run used to look ahead for the next run with text.
#[test]
fn a_rich_string_of_many_empty_runs_builds_in_linear_time() {
    let si = format!("<si><r><t>a</t></r>{}</si>", "<r><t/></r>".repeat(90_000));
    let bytes = workbook(1, "<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c></row>", Some(&si), None);
    let start = Instant::now();
    let doc = extract(&bytes, &ExtractOptions::new()).unwrap();
    let took = start.elapsed();
    assert_eq!(doc.units.len(), 1);
    assert_eq!(doc.units[0].segments[0].source, vec![Inline::text("a")]);
    assert!(took < Duration::from_secs(5), "took {took:?}");
}

/// A rich string keeps its inlines: plain text, a bold run in a tag pair, plain text.
#[test]
fn a_rich_string_builds_as_before() {
    let si = "<si><r><t xml:space=\"preserve\">Total </t></r><r><rPr><b/></rPr><t>amount</t></r><r><t xml:space=\"preserve\"> </t></r><r><t xml:space=\"preserve\"> due today</t></r></si>";
    let bytes = workbook(1, "<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c></row>", Some(si), None);
    let doc = extract(&bytes, &ExtractOptions::new()).unwrap();
    let shape: Vec<String> = doc.units[0].segments[0]
        .source
        .iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { kind, .. } => format!("{kind:?}"),
        })
        .collect();
    assert_eq!(shape, ["Total ", "Open", "amount ", "Close", " due today"]);
}
