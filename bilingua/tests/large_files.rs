//! Big ordinary files (BILI-112). Under `Limits::large()` a file far over the hosted caps (more than 50,000 units
//! or paragraphs, more than 500,000 cells) extracts, merges back unchanged, and a pseudo-translated merge reads back
//! with the same unit count. Under the hosted limits (`Limits::new()`) the same file is refused with a limit error.
//!
//! The files are generated here, so nothing large is committed. The full set is `#[ignore]`d (it takes a while
//! in a debug build); run it with `cargo test -p bilingua --release --test large_files -- --ignored`.

use bilingua::*;
use std::io::Write;

/// Units per generated file: over the hosted 50,000.
const N: usize = 60_000;

fn zip(entries: &[(&str, String)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in entries {
        w.start_file(*name, o).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn sentence(i: usize) -> String {
    format!("Paragraph {i} explains how the pump at station {} is started safely.", i % 97)
}

fn txt() -> Vec<u8> {
    (0..N).map(|i| sentence(i) + "\n\n").collect::<String>().into_bytes()
}

fn md() -> Vec<u8> {
    let mut s = String::new();
    for i in 0..N {
        if i % 50 == 0 {
            s.push_str(&format!("## Section {i}\n\n"));
        }
        s.push_str(&format!("{} It is **important** to read [the guide](https://example.com/{i}).\n\n", sentence(i)));
    }
    s.into_bytes()
}

fn html() -> Vec<u8> {
    let mut s = String::from("<!DOCTYPE html><html lang=\"en\"><head><title>Big page</title></head><body>\n");
    for i in 0..N {
        s.push_str(&format!("<p>{} See <a href=\"/p/{i}\">part {i}</a>.</p>\n", sentence(i)));
    }
    s.push_str("</body></html>\n");
    s.into_bytes()
}

fn json() -> Vec<u8> {
    let body: Vec<String> = (0..N).map(|i| format!("  \"key_{i}\": \"{}\"", sentence(i))).collect();
    format!("{{\n{}\n}}\n", body.join(",\n")).into_bytes()
}

fn po() -> Vec<u8> {
    let mut s = String::from("msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\n");
    for i in 0..N {
        s.push_str(&format!("msgctxt \"c{i}\"\nmsgid \"{}\"\nmsgstr \"\"\n\n", sentence(i)));
    }
    s.into_bytes()
}

fn yaml() -> Vec<u8> {
    let mut s = String::from("en:\n");
    for i in 0..N {
        s.push_str(&format!("  key_{i}: \"{}\"\n", sentence(i)));
    }
    s.into_bytes()
}

fn properties() -> Vec<u8> {
    (0..N).map(|i| format!("key.{i}={}\n", sentence(i))).collect::<String>().into_bytes()
}

fn csv() -> Vec<u8> {
    let mut s = String::from("id,text\n");
    for i in 0..N {
        s.push_str(&format!("{i},\"{}\"\n", sentence(i)));
    }
    s.into_bytes()
}

fn xliff() -> Vec<u8> {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\">\n<file original=\"big\" source-language=\"en\" target-language=\"de\" datatype=\"plaintext\"><body>\n",
    );
    for i in 0..N {
        s.push_str(&format!("<trans-unit id=\"u{i}\"><source>{}</source></trans-unit>\n", sentence(i)));
    }
    s.push_str("</body></file></xliff>\n");
    s.into_bytes()
}

fn srt() -> Vec<u8> {
    let mut s = String::new();
    for i in 0..N {
        let (a, b) = (i * 2, i * 2 + 1);
        s.push_str(&format!(
            "{}\n{:02}:{:02}:{:02},000 --> {:02}:{:02}:{:02},500\n{}\n\n",
            i + 1,
            a / 3600,
            a / 60 % 60,
            a % 60,
            b / 3600,
            b / 60 % 60,
            b % 60,
            sentence(i)
        ));
    }
    s.into_bytes()
}

const CT_DOCX: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const RELS_DOCX: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

/// A docx of N paragraphs (61,623 was the report's refused manual), with bold runs and a table every 500.
fn docx() -> Vec<u8> {
    let mut b = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
    );
    for i in 0..N {
        if i % 500 == 0 {
            b.push_str(&format!(r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell {i} A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Cell {i} B</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#));
        }
        b.push_str(&format!(
            r#"<w:p><w:r><w:t xml:space="preserve">{} </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>Never skip this step.</w:t></w:r></w:p>"#,
            sentence(i)
        ));
    }
    b.push_str("<w:sectPr/></w:body></w:document>");
    zip(&[("[Content_Types].xml", CT_DOCX.into()), ("_rels/.rels", RELS_DOCX.into()), ("word/document.xml", b)])
}

/// An xlsx with 600,000 text cells (over the hosted 500,000) in inline strings, 120,000 rows by 5 columns.
fn xlsx() -> Vec<u8> {
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let wb = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let wbrels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let cols = ["A", "B", "C", "D", "E"];
    let mut s = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#);
    for r in 1..=120_000usize {
        s.push_str(&format!("<row r=\"{r}\">"));
        for (c, col) in cols.iter().enumerate() {
            s.push_str(&format!("<c r=\"{col}{r}\" t=\"inlineStr\"><is><t>Item {r} field {c} ok</t></is></c>"));
        }
        s.push_str("</row>");
    }
    s.push_str("</sheetData></worksheet>");
    zip(&[
        ("[Content_Types].xml", ct.into()),
        ("_rels/.rels", rels.into()),
        ("xl/workbook.xml", wb.into()),
        ("xl/_rels/workbook.xml.rels", wbrels.into()),
        ("xl/worksheets/sheet1.xml", s),
    ])
}

fn units(doc: &Document) -> usize {
    doc.units.iter().filter(|u| u.translate).count()
}

/// Extract, identity merge, pseudo merge and read back, all under `large`; then the hosted refusal.
fn check(name: &str, bytes: &[u8], min_units: usize) {
    let large = ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph).with_limits(Limits::large());
    let mopts = MergeOptions::new().with_limits(Limits::large());
    let doc = extract(name, bytes, &large).unwrap_or_else(|e| panic!("{name}: large extract failed: {e}"));
    let n = units(&doc);
    assert!(n >= min_units, "{name}: only {n} units, expected at least {min_units}");
    let same = merge_with(&doc, &mopts).unwrap_or_else(|e| panic!("{name}: large identity merge failed: {e}"));
    if !name.ends_with(".xlsx") && !name.ends_with(".docx") {
        assert!(same == bytes, "{name}: the identity merge changed the file");
    }
    let mut t = doc.clone();
    let ids: Vec<String> = t.units.iter().filter(|u| u.translate).flat_map(|u| u.segments.iter().map(|s| s.id.clone())).collect();
    for id in &ids {
        t.set_target(id, vec![Inline::text("Ünïcødé tëxt")]).unwrap_or_else(|e| panic!("{name}: set_target {id}: {e}"));
    }
    let out = merge_with(&t, &mopts).unwrap_or_else(|e| panic!("{name}: large translated merge failed: {e}"));
    let back = extract(name, &out, &large).unwrap_or_else(|e| panic!("{name}: re-extract of the translated file failed: {e}"));
    assert_eq!(units(&back), n, "{name}: the translated file has a different unit count");
    // hosted: refused, and with a limit error rather than any other failure
    let hosted = ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph);
    match extract(name, bytes, &hosted) {
        Err(Error::Limit(_)) => {}
        Err(e) => panic!("{name}: hosted refusal is not a limit error: {e}"),
        Ok(d) => panic!("{name}: hosted limits accepted {} units", units(&d)),
    }
}

#[test]
fn profiles_by_name() {
    assert_eq!(Limits::profile("hosted"), Some(Limits::new()));
    assert_eq!(Limits::profile(" Large "), Some(Limits::large()));
    assert_eq!(Limits::profile("none"), Some(Limits::unbounded()));
    assert_eq!(Limits::profile("huge"), None);
    let l = Limits::large();
    let h = Limits::new();
    assert!(l.max_units > h.max_units && l.max_paragraphs > h.max_paragraphs && l.xlsx_max_cells > h.xlsx_max_cells);
    assert!(l.max_input_bytes > h.max_input_bytes && l.max_zip_bytes > h.max_zip_bytes);
    // the structural guards stay in large
    assert_eq!(l.max_xml_depth, h.max_xml_depth);
    assert!(l.max_zip_ratio <= 1000);
    // budgets derived from a limit cannot overflow under unbounded
    let u = Limits::unbounded();
    assert!(u.max_units.checked_mul(200).is_some());
    assert!(u.max_xml_depth == h.max_xml_depth);
}

#[test]
fn large_profile_takes_a_text_file_over_the_hosted_cap() {
    check("big.txt", &txt(), N);
}

#[test]
#[ignore = "large generated files; run with --release -- --ignored"]
fn large_markup_files() {
    check("big.md", &md(), N);
    check("big.html", &html(), N);
}

#[test]
#[ignore = "large generated files; run with --release -- --ignored"]
fn large_key_value_files() {
    check("big.json", &json(), N);
    check("big.po", &po(), N);
    check("big.yaml", &yaml(), N);
    check("big.properties", &properties(), N);
    check("big.csv", &csv(), N);
}

#[test]
#[ignore = "large generated files; run with --release -- --ignored"]
fn large_bilingual_and_subtitle_files() {
    check("big.xlf", &xliff(), N);
    check("big.srt", &srt(), N);
}

#[test]
#[ignore = "large generated files; run with --release -- --ignored"]
fn large_docx() {
    check("big.docx", &docx(), N);
}

#[test]
#[ignore = "large generated files; run with --release -- --ignored"]
fn large_xlsx() {
    check("big.xlsx", &xlsx(), 500_001);
}

/// A whole-document pass of `set_target` + `set_state` (the CLI's pseudo and merge loops) is linear: it used to
/// scan every segment on each `set_state` call, 440 s for the 260,000 segments of a 12 MB docx (BILI-112).
#[test]
#[ignore = "large generated files; run with --release -- --ignored"]
fn per_segment_edits_are_linear() {
    let big: String = (0..300_000).map(|i| format!("Line {i} here.\n\n")).collect();
    let opts = ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph).with_limits(Limits::large());
    let mut doc = extract("big.txt", big.as_bytes(), &opts).unwrap();
    let ids: Vec<String> = doc.segments().map(|(_, s)| s.id.clone()).collect();
    assert_eq!(ids.len(), 300_000);
    let t = std::time::Instant::now();
    for id in &ids {
        doc.set_target(id, vec![Inline::text("x")]).unwrap();
        doc.set_state(id, SegState::Translated).unwrap();
    }
    for id in ids.iter().step_by(3) {
        doc.set_target_empty(id).unwrap();
    }
    assert!(t.elapsed().as_secs() < 10, "300,000 edits took {:?}", t.elapsed());
}

/// A caller's huge limit (a CLI flag, a Python dict) is clamped, so no derived budget overflows (review of
/// BILI-112): 2^62 units on a file that multiplies `max_units` by 40 must neither panic nor refuse everything.
#[test]
fn huge_caller_limits_are_clamped() {
    let huge = Limits::new().with_max_units(1 << 62).with_max_lines(usize::MAX).with_max_input_bytes(u64::MAX);
    let c = huge.clamped();
    assert!(c.max_units.checked_mul(200).is_some());
    assert_eq!(Limits::new().clamped(), Limits::new());
    assert_eq!(Limits::large().clamped(), Limits::large());
    let opts = ExtractOptions::new().with_source_lang("en").with_limits(huge);
    assert!(opts.limits().max_units.checked_mul(200).is_some());
    let doc = extract("a.txt", b"One line.\n\nTwo lines.\n", &opts).unwrap();
    assert_eq!(doc.units.len(), 2);
}
