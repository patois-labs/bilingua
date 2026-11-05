//! Hostile input: every failure is a typed error, nothing panics, nothing runs away in time or memory.

mod common;
use bilingua_model::*;
use bilingua_pdf::{page_texts, Pdf};
use common::*;
use proptest::prelude::*;

fn opts() -> ExtractOptions {
    ExtractOptions::default()
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
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

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
