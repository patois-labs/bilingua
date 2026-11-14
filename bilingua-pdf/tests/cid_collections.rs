//! Type 0 fonts with an Identity CMap, an Adobe CJK `CIDSystemInfo` and no ToUnicode: the codes are CIDs and read
//! through PDFium's CID to Unicode table of the collection (as Localization OS, through PDFium, reads them).
//! Hand-made files; no font program is embedded (PDFium maps the text without one).

mod common;
use common::*;

/// One page, one Type 0 font /F1 with Identity-H, the content shows `cids`.
fn cid_pdf(ordering: &str, subtype: &str, cids: &[u16]) -> Vec<u8> {
    let hex: String = cids.iter().map(|c| format!("{c:04X}")).collect();
    let content = format!("BT /F1 12 Tf 72 700 Td <{hex}> Tj ET");
    let cid_to_gid = if subtype == "CIDFontType2" { " /CIDToGIDMap 7 0 R" } else { "" };
    let mut objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources <</Font <</F1 5 0 R>>>> /Contents 4 0 R>>".into()),
        O::Stream(String::new(), content.into_bytes()),
        O::Plain("<</Type /Font /Subtype /Type0 /BaseFont /CJK /Encoding /Identity-H /DescendantFonts [6 0 R]>>".into()),
        O::Plain(format!(
            "<</Type /Font /Subtype /{subtype} /BaseFont /CJK /CIDSystemInfo <</Registry (Adobe) /Ordering ({ordering}) /Supplement 4>> \
             /DW 1000 /W [1 95 500]{cid_to_gid}>>"
        )),
    ];
    if subtype == "CIDFontType2" {
        // A CIDToGIDMap stream (the TrueType variant): identity over the first 4096 CIDs.
        let map: Vec<u8> = (0u16..4096).flat_map(|g| g.to_be_bytes()).collect();
        objs.push(O::Stream(String::new(), map));
    }
    pdf(&objs)
}

/// The units' text; the pages are short, so the OCR threshold is lowered to 1 character.
fn one_text(bytes: &[u8]) -> String {
    texts(&extract_short(bytes)).join("|")
}

fn extract_short(bytes: &[u8]) -> bilingua_model::Document {
    use bilingua_model::Filter;
    let mut o = bilingua_model::ExtractOptions::default();
    o.format_options.insert("pdf.ocr_min_chars".into(), "1".into());
    bilingua_pdf::Pdf.extract(bytes, &o).unwrap()
}

#[test]
fn japan1_identity_without_tounicode_reads_latin_and_kana() {
    // Adobe-Japan1: CID 41 = H, 74 = i, 1 = space, 843 = U+3042, 531 = U+304B, 3284 = U+65E5, 3722 = U+672C.
    let cids = [41, 74, 1, 843, 531, 3284, 3722];
    for subtype in ["CIDFontType0", "CIDFontType2"] {
        let t = one_text(&cid_pdf("Japan1", subtype, &cids));
        assert_eq!(t, "Hi \u{3042}\u{304B}\u{65E5}\u{672C}", "{subtype}");
    }
}

#[test]
fn japan1_shifted_latin_is_the_real_word() {
    // The shape of the wild file: Latin set in an Adobe-Japan1 CID font, CID = ASCII - 31.
    let cids: Vec<u16> = "Computer Science".bytes().map(|b| b as u16 - 31).collect();
    assert_eq!(one_text(&cid_pdf("Japan1", "CIDFontType0", &cids)), "Computer Science");
}

#[test]
fn gb1_identity_without_tounicode_reads_hanzi() {
    // Adobe-GB1: CID 41 = H, 74 = i, 4559 = U+4E2D, 3795 = U+6587.
    let t = one_text(&cid_pdf("GB1", "CIDFontType0", &[41, 74, 4559, 3795]));
    assert_eq!(t, "Hi\u{4E2D}\u{6587}");
}

#[test]
fn cns1_and_korea1_identity_read_their_tables() {
    // Adobe-CNS1: 661 = U+4E2D, 726 = U+6587. Adobe-Korea1: 3296 = U+D55C, 1204 = U+AD6D.
    assert_eq!(one_text(&cid_pdf("CNS1", "CIDFontType0", &[661, 726])), "\u{4E2D}\u{6587}");
    assert_eq!(one_text(&cid_pdf("Korea1", "CIDFontType2", &[3296, 1204])), "\u{D55C}\u{AD6D}");
}

#[test]
fn tounicode_comes_first_then_the_table() {
    // ToUnicode maps CID 41 to Z only; CID 74 falls back to the Japan1 table (i).
    let content = "BT /F1 12 Tf 72 700 Td <0029004A> Tj ET";
    let b = pdf(&[
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources <</Font <</F1 5 0 R>>>> /Contents 4 0 R>>".into()),
        O::Stream(String::new(), content.as_bytes().to_vec()),
        O::Plain("<</Type /Font /Subtype /Type0 /BaseFont /CJK /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 7 0 R>>".into()),
        O::Plain("<</Type /Font /Subtype /CIDFontType0 /BaseFont /CJK /CIDSystemInfo <</Registry (Adobe) /Ordering (Japan1) /Supplement 4>> /DW 500>>".into()),
        O::Stream(String::new(), b"1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfchar <0029> <005A> endbfchar".to_vec()),
    ]);
    assert_eq!(one_text(&b), "Zi");
}

#[test]
fn no_table_for_identity_ordering_or_an_embedded_cmap() {
    // Ordering Identity: no collection, the codes have no Unicode (unchanged behaviour: nothing to translate).
    let d = extract_short(&cid_pdf("Identity", "CIDFontType2", &[41, 74]));
    assert!(texts(&d).iter().all(|t| !t.contains("Hi")), "{:?}", texts(&d));
    // An embedded CMap stream is not PDFium's kCID coding: no table either, even with Ordering Japan1.
    let content = "BT /F1 12 Tf 72 700 Td <0029004A> Tj ET";
    let b = pdf(&[
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources <</Font <</F1 5 0 R>>>> /Contents 4 0 R>>".into()),
        O::Stream(String::new(), content.as_bytes().to_vec()),
        O::Plain("<</Type /Font /Subtype /Type0 /BaseFont /CJK /Encoding 7 0 R /DescendantFonts [6 0 R]>>".into()),
        O::Plain("<</Type /Font /Subtype /CIDFontType0 /BaseFont /CJK /CIDSystemInfo <</Registry (Adobe) /Ordering (Japan1) /Supplement 4>> /DW 500>>".into()),
        O::Stream(
            "/Type /CMap /CMapName /Custom".into(),
            b"1 begincodespacerange <0000> <FFFF> endcodespacerange 1 begincidrange <0000> <FFFF> 0 endcidrange".to_vec(),
        ),
    ]);
    let d = extract_short(&b);
    assert!(texts(&d).iter().all(|t| !t.contains("Hi")), "{:?}", texts(&d));
}
