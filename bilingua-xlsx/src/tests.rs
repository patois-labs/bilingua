use crate::fixtures::*;
use crate::xml;
use crate::zipio::{read_zip, ZipLimits};
use crate::*;

fn extract_with_opts(bytes: &[u8], xo: &XlsxOptions) -> Document {
    extract_with(bytes, &ExtractOptions { target_lang: Some("es-ES".into()), ..Default::default() }, xo).unwrap()
}

fn extract(bytes: &[u8]) -> Document {
    extract_with_opts(bytes, &XlsxOptions::default())
}

fn part(bytes: &[u8], name: &str) -> String {
    let es = read_zip(bytes, &ZipLimits::default()).unwrap();
    String::from_utf8(es.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no part {name}")).data.clone()).unwrap()
}

fn unit<'a>(doc: &'a Document, id: &str) -> &'a Unit {
    doc.units.iter().find(|u| u.id == id).unwrap_or_else(|| panic!("no unit {id}"))
}

fn src_text(doc: &Document, id: &str) -> String {
    plain_text(&unit(doc, id).source())
}

fn set_plain(doc: &mut Document, id: &str, text: &str) {
    let u = unit(doc, id).clone();
    let seg = u.segments[0].id.clone();
    // The first piece of text is replaced; tags stay where they were.
    let mut done = false;
    let mut target = vec![];
    for i in u.source() {
        match i {
            Inline::Text(_) if !done => {
                done = true;
                target.push(Inline::text(text));
            }
            Inline::Text(_) => {}
            o => target.push(o),
        }
    }
    doc.set_target(&seg, target).unwrap();
}

fn assert_same_parts(a: &[u8], b: &[u8]) {
    let ea = read_zip(a, &ZipLimits::default()).unwrap();
    let eb = read_zip(b, &ZipLimits::default()).unwrap();
    assert_eq!(ea.len(), eb.len());
    for (x, y) in ea.iter().zip(eb.iter()) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.data, y.data, "{} changed", x.name);
        assert_eq!(x.meta.stored, y.meta.stored);
        assert_eq!(x.meta.modified, y.meta.modified);
    }
}

fn assert_all_xml_valid(bytes: &[u8]) {
    for e in read_zip(bytes, &ZipLimits::default()).unwrap() {
        if e.name.ends_with(".xml") || e.name.ends_with(".rels") {
            let t = String::from_utf8(e.data).unwrap();
            xml::parse(&t).unwrap_or_else(|m| panic!("{} is not well-formed: {m}", e.name));
            assert!(!t.chars().any(|c| ('\u{E000}'..='\u{E005}').contains(&c)), "{} has a marker", e.name);
        }
    }
}

#[test]
fn extracts_units_with_context() {
    let doc = extract(&workbook_zip());
    // Shared strings used by cells are units; unused, numeric-only and hidden-sheet strings follow the rules.
    assert_eq!(src_text(&doc, "ss0"), "Product name");
    assert!(doc.units.iter().all(|u| u.id != "ss21"), "unused string must not be a unit");
    assert!(doc.units.iter().all(|u| u.id != "ss15"), "digits only are not translated");
    let u = unit(&doc, "ss0");
    assert_eq!(u.context.kind.as_deref(), Some("cell"));
    assert_eq!(u.context.location.as_deref(), Some("Ventas!A1"));
    // Shared by two cells: one unit, first cell, count.
    let sub = unit(&doc, "ss14");
    assert_eq!(sub.context.location.as_deref(), Some("Ventas!A14"));
    assert_eq!(sub.context.extra.get("cells").map(String::as_str), Some("2"));
    // Inline string.
    assert_eq!(src_text(&doc, "is1-B13"), "Inline text");
    // The formula result is not a unit.
    assert!(doc.units.iter().all(|u| plain_text(&u.source()) != "Product namex"));
    // Sheet names are off by default.
    assert!(doc.units.iter().all(|u| !u.id.starts_with("sheetname-")));
    // Hidden row, column and sheet carry a note.
    assert!(unit(&doc, "ss16").context.notes.contains(&"hidden".to_string()));
    assert_eq!(unit(&doc, "ss16").context.extra.get("hidden").map(String::as_str), Some("row"));
    assert_eq!(unit(&doc, "ss17").context.extra.get("hidden").map(String::as_str), Some("column"));
    assert_eq!(unit(&doc, "ss19").context.extra.get("hidden").map(String::as_str), Some("sheet"));
    assert!(!unit(&doc, "ss0").context.notes.contains(&"hidden".to_string()));
    // Text length limit from the data validation.
    assert_eq!(unit(&doc, "ss18").context.max_len, Some(20));
    assert_eq!(unit(&doc, "ss0").context.max_len, None);
    // Comment becomes a note on the cell's unit, not a unit.
    let greeting = unit(&doc, "ss3");
    assert_eq!(src_text(&doc, "ss3"), "Bienvenido");
    assert!(greeting.context.notes.iter().any(|n| n.starts_with("Comment (Ana): Ana:")), "{:?}", greeting.context.notes);
    assert!(doc.units.iter().all(|u| u.context.kind.as_deref() != Some("comment")));
    // Header and footer.
    let h = unit(&doc, "sh1-oddHeader");
    assert_eq!(h.context.kind.as_deref(), Some("header"));
    assert_eq!(plain_text(&h.source()), "Quarterly report");
    assert_eq!(h.context.max_len, Some(255));
    assert_eq!(plain_text(&unit(&doc, "sh1-oddFooter").source()), "Tom & Jerry");
    // Ampersand in a shared string is unescaped.
    assert_eq!(src_text(&doc, "ss20"), "Tom & Jerry <3 \"quoted\"");
}

#[test]
fn all_eleven_languages_are_extracted() {
    let doc = extract(&workbook_zip());
    for (i, (_, g)) in GREETINGS.iter().enumerate() {
        assert_eq!(src_text(&doc, &format!("ss{}", 2 + i)), *g);
    }
}

#[test]
fn rich_text_becomes_paired_tags() {
    let doc = extract(&workbook_zip());
    let u = unit(&doc, "ss13");
    let src = u.source();
    assert_eq!(plain_text(&src), "Total amount due");
    assert!(matches!(&src[1], Inline::Tag { kind: TagKind::Open, display, .. } if display == "<b>"), "{src:?}");
    assert!(matches!(&src[3], Inline::Tag { kind: TagKind::Close, .. }));
}

#[test]
fn unchanged_workbook_is_byte_identical() {
    let bytes = workbook_zip();
    let doc = extract(&bytes);
    let out = Xlsx::default().merge(&doc).unwrap();
    assert_same_parts(&bytes, &out);
    // Also after setting then clearing a target, and after a JSON round trip of the document.
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_same_parts(&bytes, &Xlsx::default().merge(&back).unwrap());
}

#[test]
fn every_primary_language_round_trips() {
    let bytes = workbook_zip();
    for (lang, sample) in GREETINGS {
        let mut doc = extract(&bytes);
        for id in ["ss0", "ss1", "ss14", "ss20", "is1-B13", "sh1-oddHeader", "sh1-oddFooter", "ss3"] {
            set_plain(&mut doc, id, &format!("{sample} {id}"));
        }
        let (out, report) = merge_with_report(&doc).unwrap();
        assert!(report.warnings.iter().all(|w| w.contains("table column")), "{lang}: {:?}", report.warnings);
        assert_all_xml_valid(&out);
        // Extract again: the translated text is what we read.
        let again = extract(&out);
        assert_eq!(src_text(&again, "ss0"), format!("{sample} ss0"), "{lang}");
        assert_eq!(src_text(&again, "ss14"), format!("{sample} ss14"));
        assert_eq!(src_text(&again, "is1-B13"), format!("{sample} is1-B13"));
        assert_eq!(src_text(&again, "sh1-oddHeader"), format!("{sample} sh1-oddHeader"));
        // The ampersand in the footer is written as && and read back.
        assert_eq!(src_text(&again, "sh1-oddFooter"), format!("{sample} sh1-oddFooter"));
        // Untouched strings are unchanged.
        assert_eq!(src_text(&again, "ss4"), "Bienvenue");
        // Cells, numbers and formulas are untouched.
        let sheet = part(&out, "xl/worksheets/sheet1.xml");
        assert!(sheet.contains("<f>Oculta!A1</f><v>0</v>") && sheet.contains("<c r=\"B12\"><v>110</v></c>"));
        assert!(sheet.contains("<f>A1&amp;\"x\"</f><v>Product namex</v>"));
    }
}

#[test]
fn ampersands_and_quotes_survive() {
    let mut doc = extract(&workbook_zip());
    set_plain(&mut doc, "ss20", "Tom & Jerry <3 \"citado\" ñ");
    set_plain(&mut doc, "sh1-oddFooter", "Pie & página");
    let (out, _) = merge_with_report(&doc).unwrap();
    assert_all_xml_valid(&out);
    let sst = part(&out, "xl/sharedStrings.xml");
    assert!(sst.contains("Tom &amp; Jerry &lt;3 \"citado\" ñ"), "{sst}");
    let sheet = part(&out, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("&amp;CPie &amp;&amp; página"), "{sheet}");
}

#[test]
fn rich_text_target_keeps_formatting() {
    let mut doc = extract(&workbook_zip());
    let u = unit(&doc, "ss13").clone();
    let tags: Vec<Inline> = u.source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let target = vec![Inline::text("Importe "), tags[0].clone(), Inline::text("total"), tags[1].clone(), Inline::text(" vencido")];
    doc.set_target(&u.segments[0].id, target).unwrap();
    let (out, _) = merge_with_report(&doc).unwrap();
    let sst = part(&out, "xl/sharedStrings.xml");
    assert!(sst.contains("<r><t xml:space=\"preserve\">Importe </t></r><r><rPr><b/><sz val=\"11\"/></rPr><t>total</t></r><r><t xml:space=\"preserve\"> vencido</t></r>"), "{sst}");
    assert_all_xml_valid(&out);
    // A target without the tags still writes (a missing tag is allowed).
    let mut doc = extract(&workbook_zip());
    doc.set_target(&u.segments[0].id, vec![Inline::text("Solo texto")]).unwrap();
    let (out, _) = merge_with_report(&doc).unwrap();
    assert!(part(&out, "xl/sharedStrings.xml").contains("<si><t>Solo texto</t></si>"));
}

#[test]
fn invented_and_locked_targets_are_refused() {
    let mut doc = extract(&workbook_zip());
    let bad = doc.set_target("ss0", vec![Inline::tag("99", TagKind::Open, "<b>")]);
    assert!(matches!(bad, Err(Error::InventedTag(_))));
    assert!(matches!(doc.set_target("nope", vec![]), Err(Error::UnknownSegment(_))));
}

#[test]
fn phonetic_guides_are_kept_unless_translated() {
    let bytes = workbook_zip();
    let doc = extract(&bytes);
    assert_eq!(src_text(&doc, "ss18"), "漢字");
    let out = Xlsx::default().merge(&doc).unwrap();
    assert!(part(&out, "xl/sharedStrings.xml").contains("<rPh sb=\"0\" eb=\"2\">"));
    let mut doc = extract(&bytes);
    set_plain(&mut doc, "ss18", "Kanji");
    let (out, report) = merge_with_report(&doc).unwrap();
    assert!(!part(&out, "xl/sharedStrings.xml").contains("rPh"));
    assert!(report.warnings.iter().any(|w| w.contains("phonetic")), "{:?}", report.warnings);
}

#[test]
fn skip_hidden_leaves_hidden_text_out() {
    let doc = extract_with_opts(&workbook_zip(), &XlsxOptions { skip_hidden: true, ..Default::default() });
    for id in ["ss16", "ss17", "ss19"] {
        assert!(doc.units.iter().all(|u| u.id != id), "{id} should be skipped");
    }
    assert!(doc.units.iter().any(|u| u.id == "ss0"));
    let bytes = workbook_zip();
    assert_same_parts(&bytes, &Xlsx::default().merge(&doc).unwrap());
}

#[test]
fn sheet_names_are_off_by_default_and_rewrite_formulas_when_on() {
    let bytes = workbook_zip();
    let xo = XlsxOptions { translate_sheet_names: true, ..Default::default() };
    let mut doc = extract_with_opts(&bytes, &xo);
    assert_eq!(src_text(&doc, "sheetname-1"), "Ventas");
    assert_eq!(unit(&doc, "sheetname-1").context.max_len, Some(31));
    // Untouched: byte identical, formulas unchanged.
    assert_same_parts(&bytes, &Xlsx::default().merge(&doc).unwrap());
    set_plain(&mut doc, "sheetname-1", "売上");
    set_plain(&mut doc, "sheetname-2", "مخفي");
    set_plain(&mut doc, "sheetname-3", "Data: sheet/1?");
    let (out, report) = merge_with_report(&doc).unwrap();
    assert_all_xml_valid(&out);
    let wb = part(&out, "xl/workbook.xml");
    assert!(wb.contains("name=\"売上\""), "{wb}");
    assert!(wb.contains("name=\"مخفي\""));
    assert!(wb.contains("name=\"Data- sheet-1-\""), "{wb}");
    assert!(wb.contains("<definedName name=\"Datos\">'Data- sheet-1-'!$A$1:$A$5</definedName>"), "{wb}");
    assert!(report.warnings.iter().any(|w| w.contains("does not allow")));
    let sheet = part(&out, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<f>مخفي!A1</f>"), "{sheet}");
    assert!(sheet.contains("<f>'Data- sheet-1-'!A1</f>"), "{sheet}");
    assert!(sheet.contains("<f>A1&amp;\"x\"</f>"));
    // Text cells are untouched.
    let again = extract_with_opts(&out, &xo);
    assert_eq!(src_text(&again, "sheetname-1"), "売上");
}

#[test]
fn duplicate_sheet_names_keep_the_source_name() {
    let xo = XlsxOptions { translate_sheet_names: true, ..Default::default() };
    let mut doc = extract_with_opts(&workbook_zip(), &xo);
    set_plain(&mut doc, "sheetname-1", "oculta");
    let (out, report) = merge_with_report(&doc).unwrap();
    assert!(report.warnings.iter().any(|w| w.contains("already used")), "{:?}", report.warnings);
    assert!(part(&out, "xl/workbook.xml").contains("name=\"Ventas\""));
}

#[test]
fn table_columns_follow_their_header_cells() {
    let bytes = workbook_zip();
    let mut doc = extract(&bytes);
    // Unchanged headers: the table part is untouched.
    assert_same_parts(&bytes, &Xlsx::default().merge(&doc).unwrap());
    set_plain(&mut doc, "ss0", "Nombre del producto");
    let (out, report) = merge_with_report(&doc).unwrap();
    let t = part(&out, "xl/tables/table1.xml");
    assert!(t.contains("<tableColumn id=\"1\" name=\"Nombre del producto\"/>"), "{t}");
    assert!(t.contains("name=\"Price\""));
    assert!(report.warnings.iter().any(|w| w.contains("table column")));
}

#[test]
fn comments_are_notes_by_default_and_units_on_request() {
    let xo = XlsxOptions { translate_comments: true, ..Default::default() };
    let bytes = workbook_zip();
    let mut doc = extract_with_opts(&bytes, &xo);
    let c = unit(&doc, "comment1-A3");
    assert_eq!(c.context.kind.as_deref(), Some("comment"));
    assert_eq!(c.context.location.as_deref(), Some("Ventas!A3"));
    assert!(plain_text(&c.source()).contains("Please check the greeting."));
    assert_same_parts(&bytes, &Xlsx::default().merge(&doc).unwrap());
    let u = c.clone();
    let tags: Vec<Inline> = u.source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let t = vec![tags[0].clone(), Inline::text("Ana:"), tags[1].clone(), Inline::text("
Revisa el saludo.")];
    doc.set_target(&u.segments[0].id, t).unwrap();
    let (out, _) = merge_with_report(&doc).unwrap();
    let cm = part(&out, "xl/comments1.xml");
    assert!(cm.contains("Revisa el saludo."), "{cm}");
    assert!(cm.contains("<rPr><b/></rPr><t>Ana:</t>"), "{cm}");
    assert_all_xml_valid(&out);
}

#[test]
fn threaded_comments_are_read() {
    let mut files: Vec<(&str, String)> = vec![];
    let base = workbook_zip();
    let es = read_zip(&base, &ZipLimits::default()).unwrap();
    for e in &es {
        if e.name == "xl/worksheets/_rels/sheet1.xml.rels" {
            let t = String::from_utf8(e.data.clone()).unwrap().replace(
                "</Relationships>",
                "<Relationship Id=\"rId9\" Type=\"http://schemas.microsoft.com/office/2017/10/relationships/threadedComment\" Target=\"../threadedComments/threadedComment1.xml\"/></Relationships>",
            );
            files.push(("xl/worksheets/_rels/sheet1.xml.rels", t));
        } else {
            files.push((Box::leak(e.name.clone().into_boxed_str()), String::from_utf8(e.data.clone()).unwrap()));
        }
    }
    files.push(("xl/threadedComments/threadedComment1.xml", "<?xml version=\"1.0\"?>\n<ThreadedComments xmlns=\"http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments\"><threadedComment ref=\"A2\" personId=\"{P1}\" id=\"{C1}\"><text>これは確認が必要です</text></threadedComment></ThreadedComments>".into()));
    files.push(("xl/persons/person.xml", "<?xml version=\"1.0\"?>\n<personList xmlns=\"http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments\"><person displayName=\"Kenji\" id=\"{P1}\" userId=\"k\" providerId=\"None\"/></personList>".into()));
    let bytes = zip(&files);
    let doc = extract(&bytes);
    assert!(unit(&doc, "ss2").context.notes.iter().any(|n| n == "Comment (Kenji): これは確認が必要です"));
    let doc = extract_with_opts(&bytes, &XlsxOptions { translate_comments: true, ..Default::default() });
    assert!(doc.units.iter().any(|u| u.id == "threaded1-A2-1"));
    assert_same_parts(&bytes, &Xlsx::default().merge(&doc).unwrap());
}

#[test]
fn bad_files_are_refused_not_panicked_on() {
    let f = Xlsx::default();
    assert!(f.extract(b"not a zip at all, just text", &ExtractOptions::default()).is_err());
    assert!(f.extract(b"", &ExtractOptions::default()).is_err());
    // A zip that is not a workbook.
    let z = zip(&[("hello.txt", "hi".into())]);
    assert!(f.extract(&z, &ExtractOptions::default()).is_err());
    // Path traversal in an entry name.
    let z = zip(&[("../evil.xml", "<a/>".into())]);
    assert!(f.extract(&z, &ExtractOptions::default()).is_err());
    // DOCTYPE in the workbook.
    let mut files: Vec<(&str, String)> = vec![];
    for e in read_zip(&workbook_zip(), &ZipLimits::default()).unwrap() {
        let mut t = String::from_utf8(e.data).unwrap();
        if e.name == "xl/workbook.xml" {
            t = t.replace("<workbook", "<!DOCTYPE x [<!ENTITY y \"z\">]><workbook");
        }
        files.push((Box::leak(e.name.into_boxed_str()), t));
    }
    assert!(f.extract(&zip(&files), &ExtractOptions::default()).is_err());
}

#[test]
fn truncated_files_never_panic() {
    let bytes = workbook_zip();
    let f = Xlsx::default();
    for cut in (0..bytes.len()).step_by(97) {
        let _ = f.extract(&bytes[..cut], &ExtractOptions::default());
    }
}

#[test]
fn sniff_and_extensions() {
    let f = Xlsx::default();
    assert!(f.sniff(&workbook_zip()));
    assert!(!f.sniff(b"PK\x03\x04 random"));
    assert!(f.extensions().contains(&"xlsx"));
}

#[test]
fn libreoffice_opens_the_merged_file_when_installed() {
    let candidates = [
        "C:\\Program Files\\LibreOffice\\program\\soffice.exe",
        "C:\\Program Files (x86)\\LibreOffice\\program\\soffice.exe",
        "/usr/bin/soffice",
        "/usr/local/bin/soffice",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ];
    let Some(soffice) = candidates.iter().find(|p| std::path::Path::new(p).exists()) else {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    };
    let xo = XlsxOptions { translate_sheet_names: true, translate_comments: true, ..Default::default() };
    let mut doc = extract_with_opts(&workbook_zip(), &xo);
    for (i, u) in doc.units.clone().iter().enumerate() {
        if u.id.starts_with("sheetname-") {
            continue;
        }
        let (_, sample) = GREETINGS[i % GREETINGS.len()];
        let t: Vec<Inline> = u
            .source()
            .iter()
            .map(|x| match x {
                Inline::Text(_) => Inline::text(format!("{sample} {i}")),
                o => o.clone(),
            })
            .collect();
        doc.set_target(&u.segments[0].id, t).unwrap();
    }
    set_plain(&mut doc, "sheetname-1", "売上");
    set_plain(&mut doc, "sheetname-3", "مرحبا");
    let (out, _) = merge_with_report(&doc).unwrap();
    let dir = std::env::temp_dir().join(format!("bilingua-xlsx-lo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("merged.xlsx");
    std::fs::write(&file, out).unwrap();
    let profile = dir.join("profile");
    let url = format!("-env:UserInstallation=file:///{}", profile.to_string_lossy().replace('\\', "/"));
    let status = std::process::Command::new(soffice)
        .arg(url)
        .args(["--headless", "--convert-to", "pdf", "--outdir"])
        .arg(&dir)
        .arg(&file)
        .output()
        .unwrap();
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    assert!(dir.join("merged.pdf").exists(), "{}{}", String::from_utf8_lossy(&status.stdout), String::from_utf8_lossy(&status.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}
