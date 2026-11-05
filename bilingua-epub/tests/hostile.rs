//! Hostile input: bombs, XXE, deep nesting, path escapes, duplicate members, damaged and truncated files, a spine
//! that points outside the package, undeclared encodings. Every case is a typed error or a clean read; none panics.
mod common;
use bilingua_epub::Epub;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;
use std::io::Write;

fn kind(e: &Error) -> Option<LimitKind> {
    match e {
        Error::Limit(l) => Some(l.kind),
        _ => None,
    }
}

fn zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    w.start_file("mimetype", stored).unwrap();
    w.write_all(b"application/epub+zip").unwrap();
    for (n, d) in members {
        w.start_file(*n, stored).unwrap();
        w.write_all(d).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[test]
fn entity_bombs_in_every_package_part() {
    let dtd = "<!DOCTYPE x [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\">]>";
    let with = |s: &str| s.replacen("?>", &format!("?>{dtd}"), 1);
    for (name, body) in [
        ("META-INF/container.xml", with(CONTAINER)),
        ("OEBPS/toc.ncx", with(NCX).replace("<!DOCTYPE ncx PUBLIC \"-//NISO//DTD ncx 2005-1//EN\" \"http://www.daisy.org/z3986/2005/ncx-2005-1.dtd\">", "")),
        ("OEBPS/text/ch1.xhtml", with(CH1)),
        ("OEBPS/nav.xhtml", with(NAV)),
    ] {
        let e = extract_err(&epub_with(&[(name, body.as_bytes())]));
        assert_eq!(kind(&e), Some(LimitKind::XmlEntities), "{name}: {e}");
    }
    // UTF-16 does not hide a declaration
    let mut v = vec![0xFF, 0xFE];
    for u in with(CH2).encode_utf16() {
        v.extend_from_slice(&u.to_le_bytes());
    }
    let e = extract_err(&epub_with(&[("OEBPS/text/chapter 2.xhtml", &v)]));
    assert_eq!(kind(&e), Some(LimitKind::XmlEntities), "{e}");
}

#[test]
fn deep_nesting_is_a_typed_error() {
    let n = 100_000;
    // the nesting compresses far past the zip ratio limit; raise that limit so the XML guard is what answers
    let extract_err = |b: &[u8]| {
        let o = ExtractOptions::new().with_limits(Limits::new().with_max_zip_ratio(1_000_000));
        Epub.extract(b, &o).expect_err("expected a refusal")
    };
    let deep = |open: &str, close: &str| format!("{}x{}", open.repeat(n), close.repeat(n));
    // OPF: nested elements inside metadata
    let opf = OPF.replace("<dc:creator>Ada Writer</dc:creator>", &deep("<dc:creator>", "</dc:creator>"));
    let e = extract_err(&epub_with(&[("OEBPS/content.opf", opf.as_bytes())]));
    assert_eq!(kind(&e), Some(LimitKind::XmlDepth), "{e}");
    let ncx = NCX.replace("<navMap>", &format!("<navMap>{}", deep("<navPoint>", "</navPoint>")));
    let e = extract_err(&epub_with(&[("OEBPS/toc.ncx", ncx.as_bytes())]));
    assert_eq!(kind(&e), Some(LimitKind::XmlDepth), "{e}");
    let ch = CH2.replace("<p>Second chapter text.</p>", &deep("<div>", "</div>"));
    let e = extract_err(&epub_with(&[("OEBPS/text/chapter 2.xhtml", ch.as_bytes())]));
    assert_eq!(kind(&e), Some(LimitKind::XmlDepth), "{e}");
    // a custom limit is honoured
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_xml_depth(3));
    assert_eq!(kind(&Epub.extract(&epub(), &o).expect_err("depth 3")), Some(LimitKind::XmlDepth));
}

#[test]
fn path_escapes_and_odd_names_in_the_zip_are_refused() {
    for name in ["../evil.xhtml", "/abs/evil.xhtml", "OEBPS/../../evil", "C:/evil", "a/b:stream"] {
        let bytes = zip_of(&[("META-INF/container.xml", CONTAINER.as_bytes()), (name, b"x")]);
        let e = extract_err(&bytes);
        assert!(matches!(e, Error::Parse { .. }), "{name}: {e}");
    }
}

#[test]
fn duplicate_members_are_refused() {
    let mut bytes = zip_of(&[("OEBPS/aaaa.txt", b"1"), ("OEBPS/bbbb.txt", b"2")]);
    let a = b"OEBPS/bbbb.txt";
    let mut i = 0;
    while let Some(p) = bytes[i..].windows(a.len()).position(|w| w == a) {
        bytes[i + p..i + p + 10].copy_from_slice(b"OEBPS/aaaa");
        i += p + a.len();
    }
    let e = extract_err(&bytes);
    assert!(e.to_string().contains("twice"), "{e}");
    // names that differ only by case collide on Windows and macOS: refused too
    let bytes = zip_of(&[("OEBPS/A.txt", b"1"), ("OEBPS/a.txt", b"2")]);
    assert!(Epub.extract(&bytes, &ExtractOptions::default()).is_err());
}

#[test]
fn a_spine_pointing_outside_the_package_is_skipped() {
    let cases = [
        "../../../secret.xhtml",
        "/etc/passwd",
        "http://example.invalid/x.xhtml",
        "file:///c:/windows/win.ini",
        "missing/not-there.xhtml",
        "%2e%2e/%2e%2e/secret.xhtml",
        "",
    ];
    for href in cases {
        let opf = OPF.replace("href=\"text/ch1.xhtml\"", &format!("href=\"{href}\""));
        let d = extract(&epub_with(&[("OEBPS/content.opf", opf.as_bytes())]));
        assert!(!loc_texts(&d).iter().any(|t| t.starts_with("Hello")), "{href}");
        // the rest of the book still reads, and an untouched merge is the original file
        let src = epub_with(&[("OEBPS/content.opf", opf.as_bytes())]);
        assert_eq!(merge(&extract(&src)), src);
    }
    // an itemref to an id that is not in the manifest, and a manifest item that is not XHTML
    let opf = OPF.replace("<itemref idref=\"c1\"/>", "<itemref idref=\"nope\"/><itemref idref=\"css\"/>");
    let d = extract(&epub_with(&[("OEBPS/content.opf", opf.as_bytes())]));
    assert!(!loc_texts(&d).iter().any(|t| t.starts_with("Hello")));
}

#[test]
fn undeclared_non_utf8_xml_is_refused_and_declared_legacy_is_read() {
    let ch = "<?xml version=\"1.0\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title></head><body><p>caf\u{e9} cr\u{e8}me</p></body></html>";
    let latin1: Vec<u8> = ch.chars().map(|c| c as u32 as u8).collect();
    let e = extract_err(&epub_with(&[("OEBPS/text/chapter 2.xhtml", &latin1)]));
    assert!(matches!(e, Error::Encoding(_)), "{e}");
    let opf_latin1: Vec<u8> = OPF.replace("The Little Guide", "Le Caf\u{e9}").chars().map(|c| c as u32 as u8).collect();
    assert!(matches!(extract_err(&epub_with(&[("OEBPS/content.opf", &opf_latin1)])), Error::Encoding(_)));
    // declared: read, and written back in the same code page with the same declaration
    let declared = ch.replace("<?xml version=\"1.0\"?>", "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>");
    let bytes: Vec<u8> = declared.chars().map(|c| c as u32 as u8).collect();
    let src = epub_with(&[("OEBPS/text/chapter 2.xhtml", &bytes)]);
    let mut d = extract(&src);
    assert!(loc_texts(&d).contains(&"caf\u{e9} cr\u{e8}me".to_string()));
    assert_eq!(merge(&d), src);
    let i = loc_texts(&d).iter().position(|t| t.starts_with("caf")).unwrap();
    set(&mut d, i, "th\u{e9} \u{65e5}\u{672c}");
    let (m, _) = members(&merge(&d));
    let out = &m["OEBPS/text/chapter 2.xhtml"];
    assert!(out.windows(14).any(|w| w == b"ISO-8859-1\"?>" || w.starts_with(b"ISO-8859-1")));
    assert!(out.contains(&0xE9), "the code page is kept");
    let t = String::from_utf8_lossy(out);
    assert!(t.contains("&#26085;&#26412;"), "characters the code page lacks become references: {t}");
    // odd-length UTF-16 and a UTF-16 declaration on 8-bit text
    let e = extract_err(&epub_with(&[("OEBPS/text/chapter 2.xhtml", &[0xFF, 0xFE, b'<', 0, b'a'])]));
    assert!(matches!(e, Error::Encoding(_)), "{e}");
    let wrong = ch.replace("<?xml version=\"1.0\"?>", "<?xml version=\"1.0\" encoding=\"UTF-16\"?>");
    assert!(matches!(extract_err(&epub_with(&[("OEBPS/text/chapter 2.xhtml", wrong.as_bytes())])), Error::Encoding(_)));
}

#[test]
fn damaged_package_parts_are_typed_errors() {
    for (name, body) in [
        ("OEBPS/content.opf", "<package xmlns=\"http://www.idpf.org/2007/opf\"><metadata></package>".to_string()),
        ("OEBPS/content.opf", "<notpackage/>".to_string()),
        ("OEBPS/content.opf", OPF.replace("</package>", "")),
        ("OEBPS/content.opf", OPF.replace("<dc:title", "<dc:titl").to_string()),
        ("OEBPS/content.opf", OPF.replace("&amp;", "&nbsp;")),
        ("OEBPS/toc.ncx", NCX.replace("</ncx>", "")),
        ("META-INF/container.xml", "<container/>".to_string()),
        ("META-INF/container.xml", String::new()),
        ("META-INF/container.xml", "\u{0}\u{0}\u{0}".to_string()),
    ] {
        let e = extract_err(&epub_with(&[(name, body.as_bytes())]));
        assert!(matches!(e, Error::Parse { .. } | Error::Encoding(_) | Error::Limit(_)), "{name}: {e}");
    }
}

#[test]
fn truncated_files_never_panic() {
    let full = epub();
    for cut in (0..full.len()).step_by(61) {
        let _ = Epub.extract(&full[..cut], &ExtractOptions::default());
    }
    let _ = Epub.extract(&[], &ExtractOptions::default());
}

#[test]
fn many_unclosed_protected_elements_stay_fast() {
    // an unclosed inline element makes a naive reader scan to the end of the chapter for each one
    let body = "<code>x ".repeat(20_000);
    let ch = CH2.replace("<p>Second chapter text.</p>", &format!("<p>{body}</p><p>after</p>"));
    let t = std::time::Instant::now();
    let r = Epub.extract(&epub_with(&[("OEBPS/text/chapter 2.xhtml", ch.as_bytes())]), &ExtractOptions::default());
    let el = t.elapsed();
    assert!(r.is_ok() || r.is_err());
    assert!(el.as_secs() < 20, "took {el:?}");
}

#[test]
fn an_oversized_input_and_zip_entry_count_are_limits() {
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_input_bytes(100));
    assert_eq!(kind(&Epub.extract(&epub(), &o).expect_err("big")), Some(LimitKind::InputBytes));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_zip_entries(3));
    assert_eq!(kind(&Epub.extract(&epub(), &o).expect_err("many")), Some(LimitKind::ZipEntries));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_unit_chars(4));
    assert_eq!(kind(&Epub.extract(&epub(), &o).expect_err("long")), Some(LimitKind::UnitChars));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = Epub.extract(&bytes, &ExtractOptions::default());
        let mut zipish = b"PK\x03\x04".to_vec();
        zipish.extend(bytes);
        let _ = Epub.extract(&zipish, &ExtractOptions::default());
    }

    #[test]
    fn mutated_fixture_never_panics(edits in proptest::collection::vec((any::<proptest::sample::Index>(), any::<u8>()), 1..12)) {
        let mut b = epub();
        for (i, v) in &edits {
            let at = i.index(b.len());
            b[at] = *v;
        }
        if let Ok(d) = Epub.extract(&b, &ExtractOptions::default()) {
            let _ = Epub.merge(&d);
        }
    }

    #[test]
    fn mutated_members_never_panic(
        which in 0usize..6,
        edits in proptest::collection::vec((any::<proptest::sample::Index>(), any::<u8>()), 1..8),
        cut in proptest::option::of(any::<proptest::sample::Index>()),
    ) {
        let names = ["OEBPS/content.opf", "OEBPS/text/ch1.xhtml", "OEBPS/nav.xhtml", "OEBPS/toc.ncx", "META-INF/container.xml", "OEBPS/text/chapter 2.xhtml"];
        let (m, _) = members(&epub());
        let mut part = m[names[which]].clone();
        for (i, v) in &edits {
            let at = i.index(part.len());
            part[at] = *v;
        }
        if let Some(c) = cut {
            part.truncate(c.index(part.len() + 1));
        }
        let bytes = epub_with(&[(names[which], &part)]);
        if let Ok(mut d) = Epub.extract(&bytes, &ExtractOptions::default()) {
            d.target_lang = Some("ar".into());
            for i in 0..d.units.len() {
                let t = format!("FR {}", loc_text(&d.units[i]));
                let u = &d.units[i];
                let (_, map) = bilingua_model::locos::to_locos_text(&u.source());
                let id = u.segments[0].id.clone();
                let _ = d.set_target(&id, bilingua_model::locos::from_locos_text(&t, &map));
            }
            if let Ok(out) = Epub.merge(&d) {
                // what we write we can read again
                prop_assert!(Epub.extract(&out, &ExtractOptions::default()).is_ok());
            }
        }
    }
}
