//! Hostile input: every failure is a typed error, nothing panics, nothing runs away in time or memory.

mod common;
use bilingua_model::*;
use bilingua_pdf::{page_texts, Pdf};
use common::*;
use proptest::prelude::*;

fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

fn fuzz_cases() -> u32 {
    std::env::var("BILINGUA_PDF_FUZZ_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(400)
}

/// One content-stream token: operators, odd numbers, names of the resources below, strings, arrays, dicts.
fn token() -> impl Strategy<Value = String> {
    let ops = [
        "q", "Q", "cm", "BT", "ET", "Tf", "Td", "TD", "Tm", "T*", "Tj", "TJ", "'", "\"", "Tc", "Tw", "Tz", "TL", "Ts", "Tr", "Do",
        "BMC", "BDC", "EMC", "gs", "m", "l", "re", "f", "S", "n", "BI", "ID", "EI", "d0", "d1", "sh", "w",
    ];
    let nums = ["0", "-0", "1", "-1", "0.5", "12", "1000", "1e38", "99999999999", "-2147483649", ".", "-.", "3.4028236e38", "1000000000"];
    let names = ["/F1", "/F2", "/F3", "/F4", "/X1", "/X2", "/G1", "/P1", "/Span", "/Nope"];
    let strs = ["(abc)", "(\\101\\376)", "<0041 0042>", "<00410042004300440045>", "()", "(\\)", "<>", "(a\\(b)"];
    let mixed = ["[(a) -250 (b) 1e30 (c)]", "[1 2 3 4 5 6]", "<</ActualText (x)>>", "<</ActualText <feff0041>>>", "[]", "<<>>"];
    prop_oneof![
        proptest::sample::select(ops.to_vec()).prop_map(String::from),
        proptest::sample::select(nums.to_vec()).prop_map(String::from),
        proptest::sample::select(names.to_vec()).prop_map(String::from),
        proptest::sample::select(strs.to_vec()).prop_map(String::from),
        proptest::sample::select(mixed.to_vec()).prop_map(String::from),
        any::<i32>().prop_map(|v| v.to_string()),
    ]
}

/// A page with every kind of font and form the interpreter knows, around the given content.
fn fuzz_page(content: &[u8]) -> Vec<u8> {
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain(
            "<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources <</Font <</F1 5 0 R /F2 6 0 R /F3 8 0 R /F4 10 0 R>> \
             /XObject <</X1 11 0 R /X2 12 0 R>> /ExtGState <</G1 <</Font [5 0 R 9] /LW 2>>>> /Properties <</P1 <</ActualText (P)>>>>>> /Contents 4 0 R>>"
                .into(),
        ),
        O::Stream(String::new(), content.to_vec()),
        O::Plain("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding <</Differences [32 /space 65 /A /B /fi]>>>>".into()),
        O::Plain("<</Type /Font /Subtype /Type0 /BaseFont /X /Encoding /Identity-H /DescendantFonts [7 0 R] /ToUnicode 13 0 R>>".into()),
        O::Plain("<</Type /Font /Subtype /CIDFontType2 /BaseFont /X /W [1 [500 600] 3 9 700 0 65535 1] /DW 1000>>".into()),
        O::Plain(
            "<</Type /Font /Subtype /Type3 /FontBBox [0 0 0 0] /FontMatrix [0.001 0 0 0.001 0 0] /FirstChar 0 /LastChar 255 \
             /Encoding <</Differences [97 /a /b /c]>> /CharProcs <</a 14 0 R /b 14 0 R>> /Resources <</Font <</F3 8 0 R>>>>>>"
                .into(),
        ),
        O::Plain("<</Type /Font /Subtype /Type0 /BaseFont /Y /Encoding /90ms-RKSJ-V /DescendantFonts [7 0 R]>>".into()),
        O::Plain("<</Type /Font /Subtype /TrueType /BaseFont /Arial /FirstChar 32 /Widths [250 300]>>".into()),
        O::Stream("/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources <</XObject <</X1 11 0 R /X2 12 0 R>> /Font <</F1 5 0 R>>>>".into(), b"/X2 Do BT /F1 9 Tf (in form) Tj ET /X1 Do".to_vec()),
        O::Stream("/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Matrix [0 1 -1 0 5 5]".into(), b"BT /F1 9 Tf (rotated form) Tj ET".to_vec()),
        O::Stream(String::new(), b"2 beginbfrange <0001> <0003> <0041> <0004> <0004> [<00660069>] endbfrange 1 beginbfchar <0005> <D83DDE00> endbfchar".to_vec()),
        O::Stream(String::new(), b"0 0 d0 BT /F3 1 Tf (ab) Tj ET 0 0 m 500 700 l S".to_vec()),
    ];
    pdf(&objs)
}

#[test]
fn decompression_bomb_is_a_limit_error() {
    // 300 MB of spaces packs to about 300 KB; the default stream budget is 256 MB.
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let chunk = vec![b' '; 1 << 20];
    for _ in 0..300 {
        std::io::Write::write_all(&mut e, &chunk).unwrap();
    }
    let z = e.finish().unwrap();
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R>>".into()),
        O::Stream("/Filter /FlateDecode".into(), z),
    ];
    let b = pdf(&objs);
    let t = std::time::Instant::now();
    assert!(matches!(Pdf.extract(&b, &opts()), Err(Error::Limit(_))));
    assert!(t.elapsed().as_secs() < 60);
    // A smaller budget catches a smaller bomb.
    let mut o = opts();
    o.format_options.insert("pdf.max_stream_bytes".into(), "1000000".into());
    let b2 = pages(&[vec![b' '; 2_000_000]], true);
    assert!(matches!(Pdf.extract(&b2, &o), Err(Error::Limit(_))));
}

#[test]
fn deep_nesting_does_not_overflow() {
    let deep = format!("<</Type /Catalog /Pages 2 0 R /X {}>>", "[".repeat(200_000));
    let objs = vec![
        O::Plain(deep),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /Contents 4 0 R>>".into()),
        O::Stream(String::new(), format!("BT /F1 12 Tf {} ET", "[".repeat(100_000)).into_bytes()),
    ];
    let _ = Pdf.extract(&pdf(&objs), &opts());
    let dicts = format!("<</Type /Catalog /Pages 2 0 R /X {}>>", "<</A ".repeat(100_000));
    let objs = vec![O::Plain(dicts), O::Plain("<</Type /Pages /Kids [] /Count 0>>".into())];
    // PDFium opens it with no pages (the catalog ends at endobj), so LocOS reads zero units.
    assert!(Pdf.extract(&pdf(&objs), &opts()).is_ok_and(|d| d.units.is_empty()));
}

#[test]
fn reference_and_form_loops_end() {
    // /Length points at itself, a page tree that contains itself, a form that draws itself, an object stream
    // that claims to hold its own number.
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R 2 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /Resources <</XObject <</X 5 0 R>> /Font <</F1 6 0 R>>>> /Contents 4 0 R>>".into()),
        O::Stream(String::new(), b"/X Do BT /F1 12 Tf 72 700 Td (After the looping form, real text.) Tj ET".to_vec()),
        O::Stream("/Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources <</XObject <</X 5 0 R>>>>".into(), b"/X Do".to_vec()),
        O::Plain("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica>>".into()),
        O::Plain("7 0 R".into()),
    ];
    let d = Pdf.extract(&pdf(&objs), &opts()).unwrap();
    assert_eq!(texts(&d), vec!["After the looping form, real text.".to_string()]);
    let mut b = b"%PDF-1.5\n1 0 obj <</Type /Catalog /Pages 2 0 R>> endobj\n2 0 obj <</Type /Pages /Kids [3 0 R] /Count 1>> endobj\n".to_vec();
    b.extend_from_slice(b"3 0 obj <</Type /Page /Parent 2 0 R /Contents 4 0 R>> endobj\n4 0 obj <</Length 4 0 R>> stream\nBT ET\nendstream endobj\n");
    b.extend_from_slice(b"trailer <</Root 1 0 R>>\n%%EOF\n");
    let _ = Pdf.extract(&b, &opts());
}

#[test]
fn too_many_text_objects_is_a_limit_error() {
    let mut c = String::from("BT /F1 1 Tf\n");
    for i in 0..60_000 {
        c.push_str(&format!("{} 0 Td (x) Tj\n", i % 3));
    }
    c.push_str("ET");
    let b = pages(&[c.into_bytes()], true);
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(500));
    assert!(matches!(Pdf.extract(&b, &o), Err(Error::Limit(_))));
}

#[test]
fn a_huge_page_count_is_refused_without_walking_it() {
    let objs = vec![O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()), O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1000000>>".into()), O::Plain("<</Type /Page /Parent 2 0 R>>".into())];
    assert!(matches!(Pdf.extract(&pdf(&objs), &opts()), Err(Error::Limit(_))));
}

#[test]
fn broken_fonts_and_tounicode_maps_still_read() {
    // A ToUnicode with a wrong count is dropped (the codes fall back), a Type0 font with no descendant does not
    // load (the default font stays), a font that is not a dictionary is the stock Helvetica.
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /Resources <</Font <</F1 5 0 R /F2 6 0 R /F3 9 0 R>>>> /Contents 4 0 R>>".into()),
        O::Stream(String::new(), b"BT /F1 12 Tf 72 700 Td (Plain text with a broken map) Tj /F2 12 Tf ( and more) Tj /F3 12 Tf ( and the end of it.) Tj ET".to_vec()),
        O::Plain("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica /ToUnicode 7 0 R>>".into()),
        O::Plain("<</Type /Font /Subtype /Type0 /BaseFont /X /Encoding /Identity-H>>".into()),
        O::Stream(String::new(), b"99 beginbfchar <0050> <0051> endbfchar 3 beginbfrange <00> <ff> [ endbfrange garbage".to_vec()),
        O::Plain("null".into()),
        O::Plain("42".into()),
    ];
    let d = Pdf.extract(&pdf(&objs), &opts()).unwrap();
    assert_eq!(texts(&d), vec!["Plain text with a broken map and more and the end of it.".to_string()]);
}

proptest! {
    // `BILINGUA_PDF_FUZZ_CASES=5000` for a longer run (release).
    #![proptest_config(ProptestConfig { cases: fuzz_cases(), ..ProptestConfig::default() })]

    #[test]
    fn random_content_streams_never_panic(ops in proptest::collection::vec(token(), 0..400)) {
        let content = ops.join(" ");
        let b = fuzz_page(content.as_bytes());
        let _ = page_texts(&b, &opts());
    }

    #[test]
    fn random_bytes_never_panic(b in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = Pdf.extract(&b, &opts());
        let mut p = b"%PDF-1.4\n".to_vec();
        p.extend_from_slice(&b);
        let _ = Pdf.extract(&p, &opts());
    }

    #[test]
    fn mutated_fixtures_never_panic(seed in 0usize..1000, muts in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..24), cut in any::<usize>()) {
        let files = ["typst_digital.pdf", "raw_objstm.pdf", "raw_type0_tounicode.pdf", "raw_forms.pdf", "raw_filters.pdf", "raw_encrypted_aes128.pdf", "typst_lang_hi.pdf", "raw_actualtext_type3.pdf", "raw_encodings.pdf", "raw_cjk_legacy.pdf"];
        let mut b = data(files[seed % files.len()]);
        for (at, v) in muts {
            let n = b.len();
            b[at % n] = v;
        }
        let _ = page_texts(&b, &opts());
        let n = b.len();
        b.truncate(cut % n);
        let _ = Pdf.extract(&b, &opts());
    }
}
