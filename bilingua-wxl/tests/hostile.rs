//! Hostile input: malformed, truncated, deep and oversized files give typed errors and never panic; property
//! tests over random bytes, XML-like text, byte-mutated fixtures, and generated documents.

mod common;
use bilingua_model::*;
use bilingua_wxl::Wxl;
use common::*;
use proptest::prelude::*;

fn is_typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_))
}

fn root(inner: &str) -> Vec<u8> {
    format!("<WixLocalization Culture=\"en-us\">{inner}</WixLocalization>").into_bytes()
}

#[test]
fn malformed_files_are_parse_errors() {
    let cases: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"   ".to_vec(),
        b"<!-- only a comment -->".to_vec(),
        b"<?xml version=\"1.0\"?>".to_vec(),
        b"<".to_vec(),
        b"<WixLocalization".to_vec(),
        b"<WixLocalization>".to_vec(),
        b"</WixLocalization>".to_vec(),
        b"<WixLocalization></Other>".to_vec(),
        b"<WixLocalization><String Id=\"a\"></WixLocalization>".to_vec(),
        b"<WixLocalization/><WixLocalization/>".to_vec(),
        b"<WixLocalization/>text".to_vec(),
        b"text<WixLocalization/>".to_vec(),
        b"<WixLocalization/>&amp;".to_vec(),
        b"<WixLocalization>&nbsp;</WixLocalization>".to_vec(),
        b"<WixLocalization>&#0;</WixLocalization>".to_vec(),
        b"<WixLocalization>&#xD800;</WixLocalization>".to_vec(),
        b"<WixLocalization>&#99999999999;</WixLocalization>".to_vec(),
        b"<WixLocalization>&amp</WixLocalization>".to_vec(),
        b"<WixLocalization>&;</WixLocalization>".to_vec(),
        b"<WixLocalization>a ]]> b</WixLocalization>".to_vec(),
        b"<WixLocalization><![CDATA[never closed</WixLocalization>".to_vec(),
        b"<WixLocalization><!-- never closed </WixLocalization>".to_vec(),
        root("<String Id=\"a\" Value=\"x\" Value=\"y\"/>"),
        root("<String Id=\"a\" Value=x/>"),
        root("<String Id=\"a\" Value/>"),
        root("<String Id=\"a\" Value=\"a<b\"/>"),
        root("<String Id=\"a\" Value=\"&bad;\"/>"),
        root("<String Id=\"a\" Value=\"&amp\"/>"),
        root("<String Id=\"a\" Value=\"never closed/>"),
        root("<String Id=\"a\">a\u{1}b</String>").to_vec(),
        root("<String Id=\"a\" Value=\"a\u{2}b\"/>").to_vec(),
        root("<String Id=\"a\">a\u{fffe}b</String>").to_vec(),
        root("<String Id=\"a\">&#1;</String>"),
        root("<String Id=\"a\">x</Strin>"),
        root("<String Id=\"a\"><b></String>"),
        b"<?xml version=\"1.0\"?><?xml version=\"1.0\"?><WixLocalization/>".to_vec(),
        b"<WixLocalization/><?xml version=\"1.0\"?>".to_vec(),
        b"<WixLocalization/><!DOCTYPE x>".to_vec(),
        b"\xef\xbb\xbf".to_vec(),
        b"\x00\x00\x00".to_vec(),
        (0u8..=255).collect(),
    ];
    for c in &cases {
        match try_ex(c) {
            Err(e) => assert!(is_typed(&e), "{e:?} for {:?}", String::from_utf8_lossy(&c[..c.len().min(80)])),
            Ok(_) => panic!("must be refused: {:?}", String::from_utf8_lossy(&c[..c.len().min(80)])),
        }
    }
}

#[test]
fn encoding_errors_are_typed() {
    for data in [
        &b"<WixLocalization><String Id=\"a\">caf\xe9 \x81</String></WixLocalization>"[..],
        &b"\xff\xfe\x00\x00<\x00\x00\x00"[..],
        &b"\x00\x00\xfe\xff"[..],
        &b"\xff\xfe<\x00/\x00>"[..],
        &b"\xff\xfe<\x00/\x00>\x00\x00"[..],
        &b"\xfe\xff\x00<\xd8\x00\x00>"[..],
        &b"\xef\xbb\xbf<WixLocalization>\xc3</WixLocalization>"[..],
    ] {
        let e = try_ex(data).unwrap_err();
        assert!(is_typed(&e), "{e:?}");
    }
}

#[test]
fn entity_bombs_and_external_entities_are_refused_before_parsing() {
    // The classic billion laughs, ten levels.
    let mut dtd = String::from("<!DOCTYPE WixLocalization [<!ENTITY l0 \"lol\">");
    for i in 1..10 {
        let prev = format!("&l{};", i - 1).repeat(10);
        dtd.push_str(&format!("<!ENTITY l{i} \"{prev}\">"));
    }
    dtd.push_str("]>");
    let bomb = format!("<?xml version=\"1.0\"?>{dtd}<WixLocalization><String Id=\"a\">&l9;</String></WixLocalization>");
    let e = try_ex(bomb.as_bytes()).unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::XmlEntities), "{e:?}");
    // The quadratic blow-up and a parameter entity.
    for dtd in [
        "<!DOCTYPE WixLocalization [<!ENTITY a \"xxxxxxxxxx\"><!ENTITY b \"&a;&a;&a;&a;\">]>",
        "<!DOCTYPE WixLocalization [<!ENTITY % p \"x\">]>",
        "<!DOCTYPE WixLocalization [<!ENTITY x SYSTEM \"http://example.invalid/x\">]>",
        "<!DOCTYPE WixLocalization PUBLIC \"-//x\" \"x.dtd\" [<!ENTITY x SYSTEM \"file:///c:/windows/win.ini\">]>",
        "<!doctype WixLocalization [<!entity x \"y\">]>",
    ] {
        let data = format!("<?xml version=\"1.0\"?>{dtd}<WixLocalization><String Id=\"a\" Value=\"&x;\"/></WixLocalization>");
        let e = try_ex(data.as_bytes()).unwrap_err();
        assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::XmlEntities), "{dtd}: {e:?}");
    }
    // The same in UTF-16 (the guard reads inside the encoding).
    let utf16: Vec<u8> = std::iter::once(0xFEFFu16).chain(bomb.encode_utf16()).flat_map(|u| u.to_le_bytes()).collect();
    let e = try_ex(&utf16).unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::XmlEntities), "{e:?}");
    // A DOCTYPE with no entity declaration is kept and harmless.
    let ok = b"<?xml version=\"1.0\"?><!DOCTYPE WixLocalization SYSTEM \"x.dtd\"><WixLocalization><String Id=\"a\" Value=\"Hello\"/></WixLocalization>";
    let d = ex(ok);
    assert_eq!(merged(&d), ok);
    // Entity references: only the five predefined ones and character references.
    let d = ex(&root("<String Id=\"a\">&lt;&gt;&amp;&quot;&apos;&#65;&#x42;</String>"));
    assert_eq!(texts(&d), ["<>&\"'AB"]);
}

#[test]
fn deep_nesting_and_huge_files_are_limited() {
    let deep = format!("<WixLocalization>{}{}</WixLocalization>", "<a>".repeat(300), "</a>".repeat(300));
    let e = try_ex(deep.as_bytes()).unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::XmlDepth), "{e:?}");
    // Nesting just inside the limit parses (and the String that holds it is skipped, it has children).
    let ok = format!("<WixLocalization><String Id=\"a\">{}x{}</String></WixLocalization>", "<a>".repeat(200), "</a>".repeat(200));
    assert!(try_ex(ok.as_bytes()).unwrap().units.is_empty());
    // Too many elements.
    let small = ExtractOptions { limits: Some(Limits { max_xml_elements: 1000, ..Limits::default() }), ..opts(false) };
    let many = root(&"<x/>".repeat(1001));
    assert!(matches!(Wxl.extract(&many, &small), Err(Error::Limit(l)) if l.kind == LimitKind::XmlElements));
    // The input size limit.
    let small = ExtractOptions { limits: Some(Limits { max_input_bytes: 100, ..Limits::default() }), ..opts(false) };
    assert!(matches!(Wxl.extract(LOCOS_PACKAGE, &small), Err(Error::Limit(l)) if l.kind == LimitKind::InputBytes));
    // A value longer than the unit limit.
    let small = ExtractOptions { limits: Some(Limits { max_unit_chars: 50, ..Limits::default() }), ..opts(false) };
    let long = root(&format!("<String Id=\"a\" Value=\"{}\"/>", "word ".repeat(30)));
    assert!(matches!(Wxl.extract(&long, &small), Err(Error::Limit(l)) if l.kind == LimitKind::UnitChars));
    // Exactly at the unit cap is fine, one over is not.
    let strings: String = (0..30).map(|i| format!("<String Id=\"k{i}\" Value=\"Value number {i}\"/>")).collect();
    let data = root(&strings);
    let at = ExtractOptions { limits: Some(Limits { max_units: 30, ..Limits::default() }), ..opts(false) };
    assert_eq!(Wxl.extract(&data, &at).unwrap().units.len(), 30);
    let over = ExtractOptions { limits: Some(Limits { max_units: 29, ..Limits::default() }), ..opts(false) };
    assert!(matches!(Wxl.extract(&data, &over), Err(Error::Limit(l)) if l.kind == LimitKind::Units));
    // A deeply nested file that is only comments, PIs and CDATA with markup-looking text is not counted as nesting.
    let tricky = root("<!-- <a><a><a> --><?pi <a><a> ?><String Id=\"s\"><![CDATA[<a><a><a><a>]]></String>");
    assert_eq!(texts(&try_ex(&tricky).unwrap()), ["<a><a><a><a>"]);
}

#[test]
fn every_truncation_of_every_fixture_is_typed_or_ok() {
    for (name, bytes) in fixtures() {
        for cut in 0..bytes.len() {
            let r = Wxl.extract(&bytes[..cut], &opts(true));
            if let Err(e) = &r {
                assert!(is_typed(e), "{name} cut at {cut}: {e:?}");
            }
        }
    }
}

#[test]
fn every_single_byte_flip_of_the_small_fixtures_is_typed_or_ok() {
    for (name, bytes) in [("locos_package", LOCOS_PACKAGE), ("legacy_1251", LEGACY_1251)] {
        for i in 0..bytes.len() {
            for flip in [0x00u8, 0xFF, b'<', b'>', b'&', b'"', b']'] {
                let mut b = bytes.to_vec();
                b[i] = flip;
                match Wxl.extract(&b, &opts(true)) {
                    Err(e) => assert!(is_typed(&e), "{name} byte {i}: {e:?}"),
                    Ok(d) => {
                        // Whatever parsed must merge, and merge untouched to the same bytes.
                        assert_eq!(Wxl.merge(&d).unwrap(), b, "{name} byte {i} -> {flip:#x}");
                    }
                }
            }
        }
    }
}

fn arb_value() -> impl Strategy<Value = String> {
    // Text from several scripts, markup characters, references, white space and the edge characters of XML.
    proptest::collection::vec(
        prop_oneof![
            "[A-Za-z0-9 ]{1,8}",
            Just("&".to_string()),
            Just("<".to_string()),
            Just(">".to_string()),
            Just("\"".to_string()),
            Just("'".to_string()),
            Just("\n".to_string()),
            Just("\t".to_string()),
            Just("\r".to_string()),
            Just("[ProductName]".to_string()),
            Just("[#File]".to_string()),
            Just("[".to_string()),
            Just("]".to_string()),
            Just("]]>".to_string()),
            Just("\u{e9}\u{4e2d}\u{627}\u{1F600}".to_string()),
        ],
        0..8,
    )
    .prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        if let Err(e) = Wxl.extract(&bytes, &opts(true)) {
            prop_assert!(is_typed(&e), "{e:?}");
        }
        let _ = Wxl.sniff(&bytes);
    }

    #[test]
    fn xml_like_text_never_panics(parts in proptest::collection::vec(prop_oneof![
        Just("<WixLocalization>".to_string()), Just("</WixLocalization>".to_string()),
        Just("<String Id=\"a\" Value=\"v\"/>".to_string()), Just("<String Id='b'>".to_string()), Just("</String>".to_string()),
        Just("<UI Dialog=\"d\" Text=\"t\"/>".to_string()), Just("<![CDATA[".to_string()), Just("]]>".to_string()),
        Just("<!--".to_string()), Just("-->".to_string()), Just("<?".to_string()), Just("?>".to_string()),
        Just("&amp;".to_string()), Just("&".to_string()), Just(";".to_string()), Just("<".to_string()), Just(">".to_string()),
        Just("\"".to_string()), Just("'".to_string()), Just("=".to_string()), Just(" ".to_string()), Just("\r\n".to_string()),
        "[a-zA-Z]{1,6}",
    ], 0..40)) {
        let text = parts.concat();
        match Wxl.extract(text.as_bytes(), &opts(true)) {
            Err(e) => prop_assert!(is_typed(&e), "{e:?}"),
            Ok(d) => prop_assert_eq!(Wxl.merge(&d).unwrap(), text.as_bytes()),
        }
    }

    #[test]
    fn mutated_fixtures_never_panic(which in 0usize..12, edits in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..6), cut in proptest::option::of(any::<usize>())) {
        let fx = fixtures();
        let (_, base) = fx[which % fx.len()];
        let mut b = base.to_vec();
        for (pos, val) in edits {
            let n = b.len();
            b[pos % n] = val;
        }
        if let Some(c) = cut {
            let n = b.len();
            b.truncate(c % (n + 1));
        }
        match Wxl.extract(&b, &opts(true)) {
            Err(e) => prop_assert!(is_typed(&e), "{e:?}"),
            Ok(d) => {
                prop_assert_eq!(Wxl.merge(&d).unwrap(), b);
            }
        }
    }

    #[test]
    fn generated_documents_round_trip(values in proptest::collection::vec((arb_value(), 0u8..4), 1..12), crlf in any::<bool>()) {
        let nl = if crlf { "\r\n" } else { "\n" };
        let mut xml = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>{nl}<WixLocalization Culture=\"en-us\">{nl}");
        let esc = |s: &str, attr: bool| {
            let mut o = s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            if attr { o = o.replace('"', "&quot;").replace('\n', "&#10;").replace('\t', "&#9;").replace('\r', "&#13;"); }
            o
        };
        for (i, (v, form)) in values.iter().enumerate() {
            match form {
                0 => xml.push_str(&format!("  <String Id=\"a{i}\" Value=\"{}\" />{nl}", esc(v, true))),
                1 => xml.push_str(&format!("  <String Id=\"t{i}\">{}</String>{nl}", esc(v, false))),
                2 => xml.push_str(&format!("  <String Id=\"c{i}\"><![CDATA[{}]]></String>{nl}", v.replace("]]>", "]] >"))),
                _ => xml.push_str(&format!("  <UI Dialog=\"D\" Control=\"C{i}\" Text=\"{}\" />{nl}", esc(v, true))),
            }
        }
        xml.push_str("</WixLocalization>");
        xml.push_str(nl);
        let bytes = xml.into_bytes();
        let mut d = Wxl.extract(&bytes, &opts(false)).unwrap();
        prop_assert_eq!(Wxl.merge(&d).unwrap(), bytes.clone());
        let before = texts(&d);
        let n = d.units.len();
        pseudo(&mut d);
        let out = Wxl.merge(&d).unwrap();
        let back = Wxl.extract(&out, &opts(false)).unwrap();
        prop_assert_eq!(back.units.len(), n);
        for (b, a) in texts(&back).iter().zip(&before) {
            // Line ends read back as the parser normalises them (a lone CR in element content is a LF).
            let norm = |s: &str| s.replace("\r\n", "\n").replace('\r', "\n");
            let sp = |s: &str| s.trim_matches(|c: char| c.is_whitespace()).to_string();
            prop_assert!(norm(b).contains(&sp(&norm(a))), "{b:?} vs {a:?}");
        }
    }
}

/// A byte flipped to a Shift_JIS lead byte (237 and others) in the declared legacy fixture: the file is either
/// refused with a typed error or an untouched merge returns exactly the bytes that came in.
#[test]
fn legacy_byte_mutations_refuse_or_round_trip_exactly() {
    let seed_pos = 2032160318471024793usize % LEGACY_SJIS.len();
    let mut seeded = LEGACY_SJIS.to_vec();
    seeded[seed_pos] = 237;
    let mut refused = 0;
    let mut check = |b: Vec<u8>| match Wxl.extract(&b, &opts(true)) {
        Err(e) => {
            assert!(is_typed(&e), "{e:?}");
            refused += 1;
        }
        Ok(d) => assert_eq!(Wxl.merge(&d).unwrap(), b),
    };
    check(seeded);
    for pos in 0..LEGACY_SJIS.len() {
        for val in [0x81u8, 0x85, 0xed, 0xee, 0xfa, 0xfc, 0xa0, 0xff] {
            let mut b = LEGACY_SJIS.to_vec();
            b[pos] = val;
            check(b);
        }
    }
    assert!(refused > 0, "some mutation must be refused (double-mapped Shift_JIS sequences)");
}
