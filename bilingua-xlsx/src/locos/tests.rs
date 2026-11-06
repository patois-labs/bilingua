use super::*;
use crate::fixtures;
use proptest::prelude::*;

const NS_DECL: &str = "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";

/// The cell-mode extract (no column mapping, no comments).
fn extract(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    super::extract(bytes, opts, &crate::XlsxOptions::default())
}

fn opts(lang: Option<&str>) -> ExtractOptions {
    profile::locos().extract_options().with_granularity(Granularity::Paragraph).with_target_lang(lang.map(str::to_string))
}

fn plain(inl: &[Inline]) -> String {
    inl.iter().map(|i| if let Inline::Text(t) = i { t.as_str() } else { "" }).collect()
}

fn unit_texts(doc: &Document) -> Vec<String> {
    doc.units.iter().map(|u| plain(&u.segments[0].source)).collect()
}

fn set(doc: &mut Document, i: usize, text: &str) {
    let id = doc.units[i].segments[0].id.clone();
    profile::locos_set_target(doc, &id, if text.is_empty() { vec![] } else { vec![Inline::text(text)] }).unwrap();
}

fn merged(doc: &Document) -> Vec<u8> {
    merge(doc, &profile::locos().merge_options("xlsx")).unwrap()
}

fn entry(zip: &[u8], name: &str) -> Option<String> {
    let entries = crate::zipio::read_zip(zip, &bilingua_zip::ZipLimits::default()).unwrap();
    entries.into_iter().find(|e| e.name == name).map(|e| String::from_utf8(e.data).unwrap())
}

fn rel(items: &[(&str, &str, &str)]) -> String {
    let body: String = items
        .iter()
        .map(|(id, t, target)| format!("<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{t}\" Target=\"{target}\"/>"))
        .collect();
    format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>")
}

/// A one-sheet workbook (plus `sheets` more, each `(xml, state)`), a shared string table and extra members.
fn book(sheet: &str, sst: &str, more: &[(&str, &str)], extra: &[(&str, String)]) -> Vec<u8> {
    let mut sheet_list = "<sheet name=\"S1\" sheetId=\"1\" r:id=\"rId1\"/>".to_string();
    let mut wrels = vec![("rId1".to_string(), "worksheet", "worksheets/sheet1.xml".to_string()), ("rIdS".to_string(), "sharedStrings", "sharedStrings.xml".to_string())];
    let mut files: Vec<(String, String)> = vec![
        ("[Content_Types].xml".into(), "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/></Types>".into()),
        ("_rels/.rels".into(), rel(&[("rId1", "officeDocument", "xl/workbook.xml")])),
        ("xl/sharedStrings.xml".into(), format!("<sst {NS_DECL}>{sst}</sst>")),
        ("xl/worksheets/sheet1.xml".into(), format!("<worksheet {NS_DECL}>{sheet}</worksheet>")),
    ];
    for (k, (xml, state)) in more.iter().enumerate() {
        let n = k + 2;
        let st = if state.is_empty() { String::new() } else { format!(" state=\"{state}\"") };
        sheet_list.push_str(&format!("<sheet name=\"S{n}\" sheetId=\"{n}\"{st} r:id=\"rId{n}\"/>"));
        wrels.push((format!("rId{n}"), "worksheet", format!("worksheets/sheet{n}.xml")));
        files.push((format!("xl/worksheets/sheet{n}.xml"), format!("<worksheet {NS_DECL}>{xml}</worksheet>")));
    }
    files.push(("xl/workbook.xml".into(), format!("<workbook {NS_DECL}><sheets>{sheet_list}</sheets></workbook>")));
    let w: Vec<(&str, &str, &str)> = wrels.iter().map(|(a, b, c)| (a.as_str(), *b, c.as_str())).collect();
    files.push(("xl/_rels/workbook.xml.rels".into(), rel(&w)));
    for (n, d) in extra {
        match files.iter_mut().find(|(x, _)| x == n) {
            Some(slot) => slot.1 = d.clone(),
            None => files.push((n.to_string(), d.clone())),
        }
    }
    let refs: Vec<(&str, String)> = files.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
    fixtures::zip(&refs)
}

#[test]
fn walks_the_fixture_like_openpyxl() {
    let doc = extract(&fixtures::workbook_zip(), &opts(None)).unwrap();
    let mut want: Vec<String> = vec!["Product name".into(), "Price".into()];
    want.extend(fixtures::GREETINGS.iter().map(|(_, g)| g.to_string()));
    want.extend(
        [
            "Total amount due",          // rich text: the runs joined
            "Inline text",               // B13
            "Subtotal",                  // A14 and B14 share it: two units
            "Subtotal",
            "Secret note",               // a hidden row is walked
            "2024",                      // digits are letters or digits to Python
            "Hidden column text",        // a hidden column too
            "漢字",                      // the phonetic run is left out
            "Tom & Jerry <3 \"quoted\"",
            "Only on the hidden sheet", // a hidden sheet too
        ]
        .map(String::from),
    );
    assert_eq!(unit_texts(&doc), want);
    let ps: Vec<String> = doc.units.iter().map(|u| u.context.extra[profile::P_INDEX].clone()).collect();
    assert_eq!(ps, (0..want.len()).map(|i| i.to_string()).collect::<Vec<_>>());
    assert!(matches!(doc.skeleton, Skeleton::Binary { .. }));
    // a formula's cached text (C14) and a hidden-sheet formula are never units
    assert!(!unit_texts(&doc).iter().any(|t| t == "Product namex"));
}

#[test]
fn without_the_profile_nothing_changes() {
    let x = crate::Xlsx::default();
    let doc = bilingua_model::Filter::extract(&x, &fixtures::workbook_zip(), &ExtractOptions::default()).unwrap();
    assert!(matches!(doc.skeleton, Skeleton::Zip { .. }));
    let loc = bilingua_model::Filter::extract(&x, &fixtures::workbook_zip(), &opts(None)).unwrap();
    assert!(matches!(loc.skeleton, Skeleton::Binary { .. }));
    // a plain build with nothing written is the upload
    assert_eq!(merged(&loc), fixtures::workbook_zip());
    // merge_with_report takes either skeleton
    assert!(crate::merge_with_report(&loc).is_ok() && crate::merge_with_report(&doc).is_ok());
}

#[test]
fn shared_strings_rewritten_in_place_or_appended() {
    let sst = "<si><t>Subtotal</t></si><si><r><rPr><b/></rPr><t>Bold</t></r><r><rPr><b/></rPr><t> text</t></r></si><si><t>Mixed</t><rPh sb=\"0\" eb=\"1\"><t>m</t></rPh><phoneticPr fontId=\"1\"/></si>";
    let sheet = "<sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>0</v></c><c r=\"C1\" t=\"s\"><v>1</v></c><c r=\"D1\" t=\"s\"><v>2</v></c></row></sheetData>";
    let data = book(sheet, "", &[], &[("xl/sharedStrings.xml", format!("<sst {NS_DECL} count=\"4\" uniqueCount=\"3\">{sst}</sst>"))]);
    let mut doc = extract(&data, &opts(None)).unwrap();
    assert_eq!(unit_texts(&doc), ["Subtotal", "Subtotal", "Bold text", "Mixed"]);
    // one of two cells using the string: a new si, the cell repointed, uniqueCount grown
    set(&mut doc, 0, "Zwischensumme");
    // every text run has the same rPr: the run format is kept; phonetic runs go, phoneticPr stays
    set(&mut doc, 2, "Fett");
    set(&mut doc, 3, "Gemischt  ");
    let out = merged(&doc);
    let s = entry(&out, "xl/sharedStrings.xml").unwrap();
    assert!(s.contains("uniqueCount=\"4\""), "{s}");
    assert!(s.contains("<si><t>Subtotal</t></si>"), "{s}");
    assert!(s.contains("<si><r><rPr><b/></rPr><t>Fett</t></r></si>"), "{s}");
    assert!(s.contains("<si><t xml:space=\"preserve\">Gemischt  </t><phoneticPr fontId=\"1\"/></si>"), "{s}");
    assert!(s.ends_with("<si><t>Zwischensumme</t></si></sst>"), "{s}");
    let w = entry(&out, "xl/worksheets/sheet1.xml").unwrap();
    assert!(w.contains("<c r=\"A1\" t=\"s\"><v>3</v></c><c r=\"B1\" t=\"s\"><v>0</v></c>"), "{w}");
    // both cells the same translation: the si itself
    let mut doc = extract(&data, &opts(None)).unwrap();
    set(&mut doc, 0, "Zwischensumme");
    set(&mut doc, 1, "Zwischensumme");
    let s = entry(&merged(&doc), "xl/sharedStrings.xml").unwrap();
    assert!(s.contains("uniqueCount=\"3\"") && s.contains("<si><t>Zwischensumme</t></si><si>"), "{s}");
}

#[test]
fn other_cells_become_inline_strings() {
    // t="str" without a formula and an error literal are strings to openpyxl (LocOS translates "#N/A": copied)
    let sheet = "<sheetData><row r=\"1\"><c r=\"A1\" t=\"str\" cm=\"1\"><v>Plain str</v></c><c r=\"B1\" t=\"e\"><v>#N/A</v></c><c r=\"C1\" t=\"b\"><v>1</v></c><c r=\"D1\"><v>3.5</v></c><c r=\"E1\" t=\"str\"><f>A1</f><v>Plain str</v></c><c r=\"F1\" t=\"inlineStr\"><is><r><rPr><i/></rPr><t>Rich</t></r><r><t> inline</t></r></is><extLst/></c></row></sheetData>";
    let data = book(sheet, "", &[], &[]);
    let mut doc = extract(&data, &opts(None)).unwrap();
    assert_eq!(unit_texts(&doc), ["Plain str", "#N/A", "Rich inline"]);
    set(&mut doc, 0, "Texte");
    set(&mut doc, 1, "#N/D");
    set(&mut doc, 2, "Riche");
    let w = entry(&merged(&doc), "xl/worksheets/sheet1.xml").unwrap();
    assert!(w.contains("<c r=\"A1\" t=\"inlineStr\"><is><t>Texte</t></is></c>"), "{w}");
    assert!(w.contains("<c r=\"B1\" t=\"inlineStr\"><is><t>#N/D</t></is></c>"), "{w}");
    // mixed run formats: one plain t, before the extLst
    assert!(w.contains("<c r=\"F1\" t=\"inlineStr\"><is><t>Riche</t></is><extLst/></c>"), "{w}");
    assert!(w.contains("<c r=\"E1\" t=\"str\"><f>A1</f>"), "{w}");
}

#[test]
fn a_right_to_left_target_stamps_the_language_and_the_views() {
    let sheet = "<sheetPr/><dimension ref=\"A1\"/><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c></row></sheetData>";
    let data = book(sheet, "<si><t>Hello</t></si>", &[("<sheetViews><sheetView workbookViewId=\"0\" rightToLeft=\"1\"/></sheetViews><sheetData/>", "hidden")], &[]);
    let doc = extract(&data, &opts(Some("ar-SA"))).unwrap();
    let out = merged(&doc); // nothing translated: the language is stamped anyway
    let core = entry(&out, "docProps/core.xml").expect("a core part is made");
    assert!(core.contains("<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:language>ar-SA</dc:language></cp:coreProperties>"), "{core}");
    let rels = entry(&out, "_rels/.rels").unwrap();
    assert!(rels.contains("<Relationship Id=\"rIdCore1\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>"), "{rels}");
    let ct = entry(&out, "[Content_Types].xml").unwrap();
    assert!(ct.contains("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>"), "{ct}");
    let w = entry(&out, "xl/worksheets/sheet1.xml").unwrap();
    assert!(w.contains("<dimension ref=\"A1\"/><sheetViews><sheetView workbookViewId=\"0\" rightToLeft=\"1\"/></sheetViews><sheetData>"), "{w}");
    // a sheet already right to left is left as it was
    assert_eq!(entry(&out, "xl/worksheets/sheet2.xml"), entry(&data, "xl/worksheets/sheet2.xml"));
    // a left-to-right target: only the language
    let doc = extract(&data, &opts(Some("de_DE"))).unwrap();
    let out = merged(&doc);
    assert!(entry(&out, "docProps/core.xml").unwrap().contains("<dc:language>de-DE</dc:language>"));
    assert_eq!(entry(&out, "xl/worksheets/sheet1.xml"), entry(&data, "xl/worksheets/sheet1.xml"));
}

const DRAWING: &str = "<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><xdr:sp><xdr:txBody><a:p><a:r><a:rPr sz=\"1100\"/><a:t>Hello </a:t></a:r><a:r><a:rPr b=\"1\"/><a:t>world</a:t></a:r><a:br/><a:r><a:t>again</a:t></a:r><a:endParaRPr lang=\"en-US\"/></a:p><a:p><a:r><a:t>  </a:t></a:r></a:p><a:p><a:r><a:t>Second box</a:t></a:r></a:p></xdr:txBody></xdr:sp></xdr:wsDr>";

fn drawing_book() -> Vec<u8> {
    let sheet = "<sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>Cell</t></is></c></row></sheetData><drawing r:id=\"rId1\"/>";
    book(sheet, "", &[], &[("xl/worksheets/_rels/sheet1.xml.rels", rel(&[("rId1", "drawing", "../drawings/drawing1.xml")])), ("xl/drawings/drawing1.xml", DRAWING.to_string())])
}

#[test]
fn text_boxes_come_after_the_cells_with_run_tokens() {
    let doc = extract(&drawing_book(), &opts(None)).unwrap();
    assert_eq!(doc.units.len(), 3);
    assert_eq!(doc.units[1].context.extra[profile::STYLE], TEXT_BOX);
    let src = &doc.units[1].segments[0].source;
    let ids: Vec<(String, TagKind)> = src.iter().filter_map(|i| if let Inline::Tag { id, kind, .. } = i { Some((id.clone(), *kind)) } else { None }).collect();
    // runs are pairs 1..3, the break is point 4 (numbered after the runs, as LocOS's PowerPoint tokens)
    let want = [("1", TagKind::Open), ("1", TagKind::Close), ("2", TagKind::Open), ("2", TagKind::Close), ("4", TagKind::Standalone), ("3", TagKind::Open), ("3", TagKind::Close)];
    assert_eq!(ids, want.map(|(a, b)| (a.to_string(), b)));
    assert_eq!(plain(src), "Hello worldagain");
    assert_eq!(unit_texts(&doc)[2], "Second box");
    // runs reordered, the break dropped (LocOS keeps it at the end), a new line becomes a break
    let mut doc = extract(&drawing_book(), &opts(Some("he"))).unwrap();
    let id = doc.units[1].segments[0].id.clone();
    let t = vec![Inline::tag("2", TagKind::Open, ""), Inline::text("Welt"), Inline::tag("2", TagKind::Close, ""), Inline::text(" "), Inline::tag("1", TagKind::Open, ""), Inline::text("Hallo\nda"), Inline::tag("1", TagKind::Close, "")];
    doc.set_target(&id, t).unwrap();
    let d = entry(&merged(&doc), "xl/drawings/drawing1.xml").unwrap();
    assert!(
        d.contains("<a:p><a:pPr rtl=\"1\"/><a:r><a:rPr b=\"1\" lang=\"he\"/><a:t>Welt</a:t></a:r><a:r><a:rPr sz=\"1100\" lang=\"he\"/><a:t> </a:t></a:r><a:r><a:rPr sz=\"1100\" lang=\"he\"/><a:t>Hallo</a:t></a:r><a:br><a:rPr sz=\"1100\" lang=\"he\"/></a:br><a:r><a:rPr sz=\"1100\" lang=\"he\"/><a:t>da</a:t></a:r><a:br><a:rPr lang=\"he\"/></a:br><a:endParaRPr lang=\"he\"/></a:p>"),
        "{d}"
    );
    // an untranslated paragraph is left alone
    assert!(d.contains("<a:p><a:r><a:t>Second box</a:t></a:r></a:p>"), "{d}");
}

#[test]
fn hostile_workbooks_are_refused() {
    let ok = "<sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c></row></sheetData>";
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("bad coordinate", book("<sheetData><row><c r=\"1A\" t=\"inlineStr\"><is><t>x</t></is></c></row></sheetData>", "", &[], &[])),
        ("shared index past the table", book("<sheetData><row><c t=\"s\"><v>7</v></c></row></sheetData>", "<si><t>a</t></si>", &[], &[])),
        ("not a number", book("<sheetData><row><c><v>12abc</v></c></row></sheetData>", "", &[], &[])),
        ("bad row number", book("<sheetData><row r=\"1.5\"><c t=\"inlineStr\"><is><t>x</t></is></c></row></sheetData>", "", &[], &[])),
        ("a DOCTYPE", book(ok, "<si><t>a</t></si>", &[], &[("xl/worksheets/sheet1.xml", format!("<!DOCTYPE x [<!ENTITY a \"b\">]><worksheet {NS_DECL}/>"))])),
        ("broken XML", book(ok, "<si><t>a</t></si>", &[], &[("xl/sharedStrings.xml", "<sst><si>".into())])),
        ("every sheet hidden", book(ok, "<si><t>a</t></si>", &[], &[("xl/workbook.xml", format!("<workbook {NS_DECL}><sheets><sheet name=\"S1\" sheetId=\"1\" state=\"hidden\" r:id=\"rId1\"/></sheets></workbook>"))])),
        ("a sheet id with no relationship", book(ok, "<si><t>a</t></si>", &[], &[("xl/workbook.xml", format!("<workbook {NS_DECL}><sheets><sheet name=\"S1\" sheetId=\"1\" r:id=\"rId9\"/></sheets></workbook>"))])),
        ("no workbook part", book(ok, "", &[], &[("_rels/.rels", rel(&[("rId1", "officeDocument", "xl/nothing.xml")]))])),
        ("an unknown Strict name", book(ok, "<si><t>a</t></si>", &[], &[("_rels/.rels", "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>".into()), ("xl/styles.xml", "<a xmlns=\"http://purl.oclc.org/ooxml/unknown\"/>".into())])),
    ];
    for (why, data) in cases {
        assert!(extract(&data, &opts(None)).is_err(), "{why}");
    }
    assert!(extract(b"PK\x03\x04 not a zip", &opts(None)).is_err());
    // a forged document: its skeleton is not a workbook
    let mut doc = extract(&book(ok, "<si><t>a</t></si>", &[], &[]), &opts(None)).unwrap();
    set(&mut doc, 0, "b");
    doc.skeleton = Skeleton::binary(b"nope".to_vec(), parser_info!());
    assert!(merge(&doc, &profile::locos().merge_options("xlsx")).is_err());
}

#[test]
fn hostile_targets_are_written_as_text() {
    let mut doc = extract(&drawing_book(), &opts(None)).unwrap();
    set(&mut doc, 0, "a\u{0}b\u{b}c\u{fffe} ]]> <![CDATA[ & </t>");
    let id = doc.units[1].segments[0].id.clone();
    // unknown pair tokens stay literal text; a duplicate break is used once
    doc.set_target(&id, vec![Inline::text("\u{27e6}x18446744073709551616\u{27e7}\u{27e6}r0\u{27e7}\u{27e6}r9\u{27e7}\u{27e6}x4\u{27e7}\u{27e6}x4\u{27e7}\u{27e6}/x1\u{27e7}z")]).unwrap();
    let out = merged(&doc);
    let w = entry(&out, "xl/worksheets/sheet1.xml").unwrap();
    assert!(w.contains("<t xml:space=\"preserve\">ab\nc ]]&gt; &lt;![CDATA[ &amp; &lt;/t&gt;</t>"), "{w}");
    let d = entry(&out, "xl/drawings/drawing1.xml").unwrap();
    assert!(d.contains("<a:t>\u{27e6}r0\u{27e7}</a:t></a:r><a:r><a:rPr sz=\"1100\"/><a:t>\u{27e6}r9\u{27e7}</a:t>"), "{d}");
    // a point past the paragraph's own (an overflowing number too) is dropped, as Python's int() makes it
    assert!(!d.contains("18446744073709551616") && d.contains("\u{27e6}/x1\u{27e7}"), "{d}");
    assert_eq!(d.matches("<a:br/>").count(), 1, "{d}");
    // the result reads back
    let again = extract(&out, &opts(None)).unwrap();
    assert_eq!(unit_texts(&again)[0], "ab\nc ]]> <![CDATA[ & </t>");
}

#[test]
fn strict_workbooks_come_back_strict() {
    let strict = |s: &str| s.replace("http://schemas.openxmlformats.org/spreadsheetml/2006/main", "http://purl.oclc.org/ooxml/spreadsheetml/main").replace("http://schemas.openxmlformats.org/officeDocument/2006/relationships", "http://purl.oclc.org/ooxml/officeDocument/relationships");
    let sheet = "<sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"inlineStr\"><is><t>Two</t></is></c></row></sheetData>";
    let plain_book = book(sheet, "<si><t>One</t></si>", &[], &[]);
    let entries = crate::zipio::read_zip(&plain_book, &bilingua_zip::ZipLimits::default()).unwrap();
    let files: Vec<(String, String)> = entries.iter().map(|e| (e.name.clone(), strict(&String::from_utf8(e.data.clone()).unwrap()))).collect();
    let refs: Vec<(&str, String)> = files.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
    let data = fixtures::zip(&refs);
    let mut doc = extract(&data, &opts(Some("de-DE"))).unwrap();
    assert_eq!(unit_texts(&doc), ["One", "Two"]);
    set(&mut doc, 1, "Zwei");
    let out = merged(&doc);
    let w = entry(&out, "xl/worksheets/sheet1.xml").unwrap();
    assert!(w.contains("xmlns=\"http://purl.oclc.org/ooxml/spreadsheetml/main\"") && w.contains("<t>Zwei</t>"), "{w}");
    assert_eq!(entry(&out, "xl/sharedStrings.xml"), entry(&data, "xl/sharedStrings.xml"));
    // the new core part's package names are not Strict names
    assert!(entry(&out, "_rels/.rels").unwrap().contains("http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument"));
}

#[test]
fn package_paths_resolve_like_posixpath() {
    assert_eq!(resolve("xl/workbook.xml", "worksheets/sheet1.xml").as_deref(), Some("xl/worksheets/sheet1.xml"));
    assert_eq!(resolve("xl/worksheets/sheet1.xml", "../drawings/d.xml").as_deref(), Some("xl/drawings/d.xml"));
    assert_eq!(resolve("", "/xl/workbook.xml").as_deref(), Some("xl/workbook.xml"));
    assert_eq!(resolve("xl/a.xml", "../../x.xml"), None);
    assert_eq!(resolve("", "."), None);
    assert_eq!(rels_name(""), "_rels/.rels");
    assert_eq!(rels_name("xl/workbook.xml"), "xl/_rels/workbook.xml.rels");
    assert_eq!(coord("$AB$12", "x").unwrap(), (12, 28));
    assert!(coord("ABCD1", "x").is_err() && coord("A", "x").is_err());
    assert_eq!(col_letters(28), "AB");
}

fn cell_text() -> impl Strategy<Value = String> {
    // letters, XML specials, white space, controls and private-use code points (never `x005F_`, which openpyxl
    // removes from shared strings)
    proptest::collection::vec(prop_oneof![Just('a'), Just('Z'), Just('7'), Just('&'), Just('<'), Just('>'), Just('"'), Just('\''), Just(' '), Just('\t'), Just('\n'), Just('\r'), Just('\u{1}'), Just('\u{b}'), Just('\u{e000}'), Just('\u{27e6}'), Just('é'), Just('中')], 0..12)
        .prop_map(|v| v.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn translated_cells_read_back(targets in proptest::collection::vec(cell_text(), 4)) {
        let sheet = "<sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>0</v></c></row><row r=\"2\"><c r=\"A2\" t=\"inlineStr\"><is><t>Inline</t></is></c><c r=\"B2\" t=\"s\"><v>1</v></c></row></sheetData>";
        let data = book(sheet, "<si><t>Shared</t></si><si><r><t>Rich</t></r></si>", &[], &[]);
        let mut doc = extract(&data, &opts(Some("fr-FR"))).unwrap();
        for (i, t) in targets.iter().enumerate() {
            set(&mut doc, i, t);
        }
        let out = merged(&doc);
        let again = extract(&out, &opts(None)).unwrap();
        // each cell holds its translation (XML-illegal characters dropped); one with no letter or digit is no unit.
        // A target of white space only is a translation under the profile (`profile::locos_set_target`): LocOS
        // writes `xml_target_text(tr)` for every target it has (`XlsxFilter._rebuild`), so a lone vertical tab
        // becomes a new line, and a cell of white space is no unit when read back
        let want: Vec<String> = targets.iter().map(|t| xml_target_text(t)).filter(|t| bilingua_model::py_has_alnum(t)).collect();
        prop_assert_eq!(unit_texts(&again), want);
    }

    #[test]
    fn any_paragraph_target_keeps_the_breaks(text in proptest::collection::vec(prop_oneof![Just("\u{27e6}r1\u{27e7}".to_string()), Just("\u{27e6}/r2\u{27e7}".to_string()), Just("\u{27e6}x4\u{27e7}".to_string()), Just("\u{27e6}x9\u{27e7}".to_string()), Just("\u{27e6}".to_string()), Just("ab".to_string()), Just("\n".to_string()), Just("\r\n".to_string())], 0..10)) {
        let mut d = dom::parse(DRAWING).unwrap();
        let p = d.iter_tag(d.root, A, "p")[0];
        write_paragraph(&mut d, p, &text.concat());
        let out = d.serialize();
        let reparsed = dom::parse(&out).unwrap();
        let p2 = reparsed.iter_tag(reparsed.root, A, "p")[0];
        // the paragraph's own break survives, at least one run is there, endParaRPr stays last
        let kids = reparsed.element_children(p2);
        prop_assert!(kids.iter().any(|&k| reparsed.is(k, A, "r")));
        prop_assert!(reparsed.is(*kids.last().unwrap(), A, "endParaRPr"));
        let breaks = kids.iter().filter(|&&k| reparsed.is(k, A, "br")).count();
        prop_assert!(breaks >= 1);
    }
}
