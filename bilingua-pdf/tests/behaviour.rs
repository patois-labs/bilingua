//! The Bilingua contract for PDF: ids, paragraph mode, untouched merges, the skeleton stamp, options, limits,
//! encryption and the damaged-file cases LocOS reads or refuses.

mod common;
use bilingua_model::*;
use bilingua_pdf::{page_texts, Pdf};
use common::*;

#[test]
fn ids_are_stable_and_name_the_page() {
    let b = data("typst_pagination.pdf");
    let a = ex(&b).unwrap();
    let again = ex(&b).unwrap();
    let ids: Vec<&str> = a.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, again.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>());
    assert_eq!(ids[0], "p1u1");
    assert_eq!(a.units[1].id, "p2u1");
    assert_eq!(a.units[1].context.extra.get("page").map(String::as_str), Some("2"));
    assert_eq!(a.units[0].context.kind.as_deref(), Some("paragraph"));
}

#[test]
fn paragraph_mode_and_untouched_merge_on_every_fixture() {
    let g = golden();
    for (name, want) in g.as_object().unwrap() {
        if want.get("error").is_some() {
            continue;
        }
        let b = data(name);
        let o = ExtractOptions::new().with_target_lang("ar-EG").with_granularity(Granularity::Paragraph);
        let d = Pdf.extract(&b, &o).unwrap();
        assert!(d.units.iter().all(|u| u.segments.len() == 1 && u.segments[0].id == u.id), "{name}");
        assert_eq!(Pdf.merge(&d).unwrap(), b, "{name}: untouched merge changed the file");
        // The JSON form of the document (a binary skeleton in base64) merges to the same bytes.
        let back: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(Pdf.merge(&back).unwrap(), b, "{name}");
    }
}

#[test]
fn a_translated_or_stamped_merge_is_unsupported() {
    let b = data("typst_digital.pdf");
    let d = Pdf.extract(&b, &ExtractOptions::new().with_target_lang("fr-FR")).unwrap();
    let mut stamp = MergeOptions::new();
    stamp.stamp_target_always = true;
    assert!(matches!(Pdf.merge_with(&d, &stamp), Err(Error::Unsupported(_))));
    let mut t = d.clone();
    let id = t.units[0].id.clone();
    t.set_target(&id, vec![Inline::text("Bonjour")]).unwrap();
    assert!(matches!(Pdf.merge(&t), Err(Error::Unsupported(_))));
}

#[test]
fn skeleton_is_binary_and_stamped() {
    let d = ex(&data("typst_digital.pdf")).unwrap();
    assert!(matches!(d.skeleton, Skeleton::Binary { .. }));
    assert_eq!(d.skeleton.parser().unwrap().name, "bilingua-pdf");
    let mut foreign = d.clone();
    if let Skeleton::Binary { parser, .. } = &mut foreign.skeleton {
        parser.as_mut().unwrap().name = "bilingua-docx".into();
    }
    assert!(matches!(Pdf.merge(&foreign), Err(Error::ParserVersion { .. })));
}

#[test]
fn sniff_finds_the_header_in_the_first_kilobyte() {
    assert!(Pdf.sniff(&data("typst_digital.pdf")));
    assert!(Pdf.sniff(&data("raw_junk_prefix.pdf")));
    assert!(!Pdf.sniff(b"%!PS-Adobe-3.0"));
    let mut late = vec![b' '; 1100];
    late.extend_from_slice(b"%PDF-1.4");
    assert!(!Pdf.sniff(&late));
}

#[test]
fn the_ocr_threshold_is_twenty_stripped_characters() {
    let short = pages(&[text_page(&["  nineteen chars xxxx "])], false);
    let p = page_texts(&short, &ExtractOptions::default()).unwrap();
    assert_eq!(bilingua_model::py_strip(&p[0].text).chars().count(), 19);
    assert!(p[0].ocr);
    let ok = pages(&[text_page(&["twenty characters ok"])], false);
    assert!(!page_texts(&ok, &ExtractOptions::default()).unwrap()[0].ocr);
    let mut o = ExtractOptions::default();
    o.format_options.insert("pdf.ocr_min_chars".into(), "5".into());
    assert!(!page_texts(&short, &o).unwrap()[0].ocr);
}

#[test]
fn page_limit_and_unit_limit() {
    let many = pages(&vec![text_page(&["A line that is long enough for a text page."]); 12], true);
    let mut o = ExtractOptions::default();
    o.format_options.insert("pdf.max_pages".into(), "10".into());
    assert!(matches!(Pdf.extract(&many, &o), Err(Error::Limit(_))));
    assert_eq!(ex(&many).unwrap().units.len(), 12);
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    assert!(matches!(Pdf.extract(&many, &o), Err(Error::Limit(_))));
}

#[test]
fn ocr_pixel_budget_is_refused_before_any_page() {
    // Three huge empty pages: each renders at the 4000 px cap, 16 M pixels; a budget of 40 M refuses the third.
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 3000 3000]>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R>>".into()),
    ];
    let b = pdf(&objs);
    let mut o = ExtractOptions::default();
    o.format_options.insert("pdf.ocr_max_total_pixels".into(), "40000000".into());
    assert!(matches!(Pdf.extract(&b, &o), Err(Error::Limit(_))));
    assert_eq!(ex(&b).unwrap().warnings.len(), 3);
}

#[test]
fn format_option_keys_are_declared() {
    let keys: Vec<&str> = Pdf.format_option_keys().iter().map(|k| k.key).collect();
    for k in ["pdf.max_pages", "pdf.max_stream_bytes", "pdf.ocr_min_chars", "pdf.ocr_max_pixel_dim", "pdf.ocr_max_total_pixels"] {
        assert!(keys.contains(&k), "{k}");
    }
    assert!(Pdf.format_option_keys().iter().all(|k| k.kind == OptionKind::Int));
    // Called directly, the filter refuses a misspelt key of its own family, a value that is not an integer and a
    // negative one; another format's key is left for the facade.
    let b = data("typst_digital.pdf");
    for (k, v) in [("pdf.max_page", "10"), ("pdf.max_pages", "ten"), ("pdf.max_pages", "-1"), ("pdf.ocr_max_pixel_dim", "4000.5")] {
        let o = ExtractOptions::new().with_format_option(k, v);
        assert!(matches!(Pdf.extract(&b, &o), Err(Error::FormatOption { .. })), "{k}={v}");
        assert!(matches!(page_texts(&b, &o), Err(Error::FormatOption { .. })), "{k}={v}");
    }
    let o = ExtractOptions::new().with_format_option("docx.translate_comments", "true").with_format_option("pdf.max_pages", " 5 ");
    assert!(Pdf.extract(&b, &o).is_ok());
    let d = Pdf.extract(&b, &ExtractOptions::default()).unwrap();
    let m = MergeOptions::new().with_format_option("pdf.nope", "1");
    assert!(matches!(Pdf.merge_with(&d, &m), Err(Error::FormatOption { .. })));
}

#[test]
fn owner_password_files_open_and_user_password_files_are_refused() {
    for f in ["raw_encrypted_rc440.pdf", "raw_encrypted_rc4128.pdf", "raw_encrypted_aes128.pdf", "raw_encrypted_aes256.pdf"] {
        let d = ex(&data(f)).unwrap();
        assert_eq!(texts(&d), vec!["Owner password only, the text still opens.".to_string()], "{f}");
    }
    for f in ["raw_encrypted_userpw.pdf", "raw_encrypted_userpw_aes256.pdf"] {
        assert!(matches!(ex(&data(f)), Err(Error::Unsupported(_))), "{f}");
    }
}

#[test]
fn damaged_cross_references_are_repaired_as_pdfium_does() {
    for f in ["raw_bad_offsets.pdf", "raw_no_xref.pdf", "raw_junk_prefix.pdf", "raw_truncated_tail.pdf"] {
        assert_eq!(texts(&ex(&data(f)).unwrap()), vec!["Repaired cross reference table text.".to_string()], "{f}");
    }
    assert_eq!(texts(&ex(&data("raw_incremental.pdf")).unwrap()), vec!["Second revision replaces the first.".to_string()]);
    assert_eq!(texts(&ex(&data("raw_prev_loop.pdf")).unwrap()), vec!["Second revision replaces the first.".to_string()]);
    for f in ["raw_truncated_half.pdf", "raw_not_pdf.pdf", "raw_empty.pdf"] {
        assert!(matches!(ex(&data(f)), Err(Error::Parse { .. })), "{f}");
    }
}

#[test]
fn legacy_encodings_and_cmaps() {
    let d = ex(&data("raw_cjk_legacy.pdf")).unwrap();
    assert_eq!(texts(&d), vec!["日本語のテキスト ABC 简体中文文本 繁體中文文字 한국어 텍스트 ユニコード".to_string()]);
    let d = ex(&data("raw_encodings.pdf")).unwrap();
    let t = texts(&d).join(" ");
    for want in ["find flow", "MacRoman: été", "Standard: “quoted” Æ æ", "αβχ"] {
        assert!(t.contains(want), "{want} in {t}");
    }
}

#[test]
fn line_end_hyphens_join_visible_hyphens_stay_and_unmapped_glyphs_are_dropped() {
    // #808. PDFium marks a hyphen that breaks a word at a line end with U+FFFE; units join the word. A hyphen inside
    // a line, or at a line end after a digit, is not marked and stays. page_texts keeps PDFium's text as it is.
    let b = pages(&[text_page(&["A line ending with a hyphen-", "ated word, then a well-known page 12-", "14 of the book."])], false);
    let p = page_texts(&b, &ExtractOptions::default()).unwrap();
    assert!(p[0].text.contains("hyphen\u{fffe}ated"), "{:?}", p[0].text);
    assert_eq!(texts(&ex(&b).unwrap()), vec!["A line ending with a hyphenated word, then a well-known page 12- 14 of the book.".to_string()]);

    // A font whose ToUnicode maps `x` to U+0000: PDFium writes U+FFFE for it. Units drop the glyph (no `-` is
    // invented) and keep the line break after it.
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources <</Font <</F1 5 0 R>>>> /Contents 4 0 R>>".into()),
        O::Stream(String::new(), b"BT /F1 11 Tf 14 TL 72 740 Td (The oxce has an unmapped glyph x) Tj T* (and the following line ends it.) Tj ET".to_vec()),
        O::Plain("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding /ToUnicode 6 0 R>>".into()),
        O::Stream(String::new(), b"1 beginbfchar <78> <0000> endbfchar".to_vec()),
    ];
    let b = pdf(&objs);
    let p = page_texts(&b, &ExtractOptions::default()).unwrap();
    assert!(p[0].text.contains("o\u{fffe}ce"), "{:?}", p[0].text);
    assert_eq!(texts(&ex(&b).unwrap()), vec!["The oce has an unmapped glyph and the following line ends it.".to_string()]);

    // A page of unmapped glyphs only has no printable text: it goes to OCR.
    let objs2: Vec<O> = objs
        .into_iter()
        .enumerate()
        .map(|(i, o)| if i == 3 { O::Stream(String::new(), b"BT /F1 11 Tf 72 740 Td (xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx) Tj ET".to_vec()) } else { o })
        .collect();
    let p = page_texts(&pdf(&objs2), &ExtractOptions::default()).unwrap();
    assert_eq!(p[0].text.chars().filter(|&c| c == '\u{fffe}').count(), 30);
    assert!(p[0].ocr);
}
