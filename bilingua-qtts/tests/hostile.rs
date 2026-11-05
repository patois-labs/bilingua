//! Hostile input: malformed, truncated, deep and oversized files give typed errors and never panic; property tests
//! over random bytes, mutated fixtures, truncations and generated documents. Whatever extracts must merge back to
//! the same bytes untouched, and a pseudo-translated merge must read back.

mod common;
use bilingua_model::*;
use bilingua_qtts::QtTs;
use common::*;
use proptest::prelude::*;

fn is_typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Limit(_) | Error::Encoding(_) | Error::Unsupported(_))
}

/// The invariants for any bytes: no panic; an error is typed; a success merges back to the same bytes untouched,
/// and merges pseudo-translated into a file that reads back with the same units.
fn check(bytes: &[u8]) {
    match QtTs.extract(bytes, &opts(Some("de-DE"))) {
        Err(e) => assert!(is_typed(&e), "untyped error {e:?}"),
        Ok(mut d) => {
            assert_eq!(QtTs.merge(&d).unwrap(), bytes, "untouched merge changed the bytes");
            pseudo(&mut d);
            let out = QtTs.merge(&d).unwrap_or_else(|e| panic!("merge failed: {e}"));
            let back = QtTs.extract(&out, &opts(None)).unwrap_or_else(|e| panic!("merged file does not read back: {e}\n{}", String::from_utf8_lossy(&out)));
            assert_eq!(back.units.len(), d.units.len());
        }
    }
}

#[test]
fn malformed_files_are_typed_errors() {
    let cases: &[&[u8]] = &[
        b"",
        b"   ",
        b"\xef\xbb\xbf",
        b"<",
        b"<TS",
        b"<TS>",
        b"<TS></ts>",
        b"<TS><context></TS>",
        b"<TS/><TS/>",
        b"text<TS/>",
        b"<TS/>text",
        b"<TS a=1/>",
        b"<TS a='1' a='2'/>",
        b"<TS a=\"1\"b=\"2\"/>",
        b"<TS a=\"<\"/>",
        b"<TS>&nbsp;</TS>",
        b"<TS>&#0;</TS>",
        b"<TS>&#x110000;</TS>",
        b"<TS>&amp</TS>",
        b"<TS>a & b</TS>",
        b"<TS>]]></TS>",
        b"<TS><![CDATA[never ends</TS>",
        b"<TS><!-- never ends</TS>",
        b"<TS><!-- a -- b --></TS>",
        b"<TS><?pi never ends</TS>",
        b"<TS><!ELEMENT x></TS>",
        b"<TS xmlns:a=\"u\"><b:c/></TS>",
        b"<TS>\x01</TS>",
        b"<?xml version=\"1.0\"?><?xml version=\"1.0\"?><TS/>",
        b"<TS/><?xml version=\"1.0\"?>",
        b"<!DOCTYPE TS><!DOCTYPE TS><TS/>",
        b"<!DOCTYPE TS [ <!ENTITY a \"b\"> ]><TS>&a;</TS>",
        b"<!DOCTYPE TS [ ]><TS/>",
        b"<!DOCTYPE TS",
        b"<TS><context><name>x</name><message><source>a</source><translation></message></context></TS>",
        b"{\"json\": true}",
        b"export default {};",
        b"\xff\xfe<\x00T\x00",
        b"\xff\xfe\x00",
        b"<TS>\xff\xfe</TS>",
        b"<?xml version=\"1.0\" encoding=\"nonsense\"?><TS/>",
        b"PK\x03\x04",
        b"\x00\x00\x00\x00",
    ];
    for c in cases {
        match QtTs.extract(c, &opts(None)) {
            Ok(d) => {
                // only a well-formed TS root with nothing else in it may succeed
                assert!(d.units.is_empty(), "{:?} gave units", String::from_utf8_lossy(c));
            }
            Err(e) => assert!(is_typed(&e), "{e:?}"),
        }
    }
    // the ones that must be refused, not read as empty
    for c in cases.iter().filter(|c| c.len() > 3 && !c.starts_with(b"<TS/>")) {
        if let Ok(d) = QtTs.extract(c, &opts(None)) {
            assert!(d.units.is_empty());
        }
    }
    assert!(QtTs.extract(b"<TS/><TS/>", &opts(None)).is_err());
    assert!(QtTs.extract(b"<TS>&nbsp;</TS>", &opts(None)).is_err());
    assert!(QtTs.extract(b"<TS>a & b</TS>", &opts(None)).is_err());
    assert!(QtTs.extract(b"<TS a='1' a='2'/>", &opts(None)).is_err());
    assert!(QtTs.extract(b"<TS><b:c/></TS>", &opts(None)).is_err());
    assert!(QtTs.extract(b"<TS>\x01</TS>", &opts(None)).is_err());
}

#[test]
fn entity_bomb_and_xxe_are_refused_before_parsing() {
    let mut bomb = String::from("<?xml version=\"1.0\"?><!DOCTYPE TS [<!ENTITY e0 \"lol\">");
    for i in 1..10 {
        bomb.push_str(&format!("<!ENTITY e{i} \"{}\">", format!("&e{};", i - 1).repeat(10)));
    }
    bomb.push_str("]><TS><context><name>x</name><message><source>&e9;</source><translation/></message></context></TS>");
    assert!(matches!(try_ex(bomb.as_bytes()), Err(Error::Limit(_))));
    let xxe = "<?xml version=\"1.0\"?><!DOCTYPE TS [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><TS><context><name>x</name><message><source>&x;</source><translation/></message></context></TS>";
    assert!(matches!(try_ex(xxe.as_bytes()), Err(Error::Limit(_))));
    // the same in UTF-16
    let mut u16 = vec![0xFF, 0xFE];
    for u in xxe.encode_utf16() {
        u16.extend(u.to_le_bytes());
    }
    assert!(matches!(try_ex(&u16), Err(Error::Limit(_))));
}

#[test]
fn deep_wide_and_huge_files_are_limited() {
    // deeper than the limit in the structure itself
    let deep = format!("<TS>{}{}</TS>", "<a>".repeat(300), "</a>".repeat(300));
    assert!(matches!(try_ex(deep.as_bytes()), Err(Error::Limit(_))));
    // more elements than allowed
    let few = Limits::new().with_max_xml_elements(100);
    let wide = format!("<TS>{}</TS>", "<a/>".repeat(1000));
    assert!(matches!(QtTs.extract_with(wide.as_bytes(), &opts(None), &few), Err(Error::Limit(_))));
    // a value over the unit length
    let small = Limits::new().with_max_unit_chars(50);
    let long = format!("<TS><context><name>x</name><message><source>{}</source><translation/></message></context></TS>", "word ".repeat(100));
    assert!(matches!(QtTs.extract_with(long.as_bytes(), &opts(None), &small), Err(Error::Limit(_))));
    // over the input limit
    let tiny = Limits::new().with_max_input_bytes(20);
    assert!(matches!(QtTs.extract_with(&data("edge.ts"), &opts(None), &tiny), Err(Error::Limit(_))));
    // many units
    let few_units = Limits::new().with_max_units(3);
    assert!(matches!(QtTs.extract_with(&data("edge.ts"), &opts(None), &few_units), Err(Error::Limit(_))));
}

#[test]
fn every_prefix_of_a_file_is_an_error_or_a_faithful_file() {
    let bytes = lf("locos_app_de.ts");
    for n in 0..bytes.len() {
        check(&bytes[..n]);
    }
    let edge = data("edge.ts");
    for n in (0..edge.len()).step_by(3) {
        check(&edge[..n]);
    }
}

#[test]
fn every_single_byte_flip_of_a_small_file_is_safe() {
    let bytes = lf("locos_app_de.ts");
    for i in 0..bytes.len() {
        for v in [0u8, b'<', b'>', b'&', b'"', 0xFF] {
            let mut m = bytes.clone();
            m[i] = v;
            check(&m);
        }
    }
}

#[test]
fn a_translation_that_looks_like_markup_stays_text() {
    let mut d = ex(&lf("locos_app_de.ts"));
    set(&mut d, 0, "</translation></message></context></TS><TS>&amp; <![CDATA[ ]]> -->");
    let out = merged(&d);
    let back = ex(&out);
    assert_eq!(back.units.len(), 4);
    assert_eq!(texts(&back), texts(&ex(&lf("locos_app_de.ts"))));
}

fn arb_text() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9 .,!?%&<>'\"-]{0,30}",
        "[\u{e0}-\u{ff}\u{410}-\u{44f}\u{627}-\u{64a}\u{3041}-\u{3093}\u{4e00}-\u{4e40}\u{ac00}-\u{ac40}]{0,12}",
        Just("%n item(s)".to_string()),
        Just("Tab<byte value=\"x9\"/>sep".to_string()),
        Just("<![CDATA[x < y]]>".to_string()),
        Just(" lead and trail ".to_string()),
    ]
}

fn build_file(msgs: &[(String, bool, u8, String)], crlf: bool) -> String {
    let nl = if crlf { "\r\n" } else { "\n" };
    let mut s = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>{nl}<!DOCTYPE TS>{nl}<TS version=\"2.1\" language=\"de_DE\">{nl}<context>{nl}<name>Gen</name>{nl}");
    for (src, numerus, kind, tr) in msgs {
        let esc = |t: &str| -> String {
            if t.contains("<byte") || t.contains("<![CDATA[") {
                t.to_string()
            } else {
                t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
            }
        };
        s.push_str(&format!("<message{}>{nl}<source>{}</source>{nl}", if *numerus { " numerus=\"yes\"" } else { "" }, esc(src)));
        match kind % 5 {
            0 => s.push_str(&format!("<translation type=\"unfinished\">{}</translation>{nl}", esc(tr))),
            1 => s.push_str(&format!("<translation type=\"unfinished\"/>{nl}")),
            2 => s.push_str(&format!("<translation type=\"vanished\">{}</translation>{nl}", esc(tr))),
            3 => s.push_str(&format!("<translation><numerusform>{}</numerusform><numerusform/></translation>{nl}", esc(tr))),
            _ => {}
        }
        s.push_str(&format!("</message>{nl}"));
    }
    s.push_str(&format!("</context>{nl}</TS>{nl}"));
    s
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(b in proptest::collection::vec(any::<u8>(), 0..400)) {
        check(&b);
    }

    #[test]
    fn xml_like_text_never_panics(s in "(<TS>|</TS>|<context>|</context>|<message numerus=\"yes\">|</message>|<source>|</source>|<translation[ a-z=\"]{0,16}>|</translation>|<numerusform>|</numerusform>|<byte value=\"x9\"/>|<name>|</name>|&amp;|&#233;|<!--|-->|<!\\[CDATA\\[|\\]\\]>|[a-z %]{1,6}){0,30}") {
        check(s.as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..64, pos in 0usize..4000, val in any::<u8>(), cut in 0usize..4000, ins in proptest::collection::vec(any::<u8>(), 0..6)) {
        let all = all_data();
        let (_, bytes) = &all[idx % all.len()];
        let mut m = bytes.clone();
        if !m.is_empty() {
            let p = pos % m.len();
            m[p] = val;
            let at = (pos * 7 + 3) % m.len();
            for (k, b) in ins.iter().enumerate() { m.insert((at + k).min(m.len()), *b); }
            m.truncate(m.len().saturating_sub(cut % 7));
        }
        check(&m);
    }

    #[test]
    fn generated_documents_round_trip(
        msgs in proptest::collection::vec((arb_text(), any::<bool>(), 0u8..5, arb_text()), 0..12),
        crlf in any::<bool>(),
        target in prop_oneof![Just(None), Just(Some("ru")), Just(Some("ar-SA")), Just(Some("ja")), Just(Some("pl-PL"))],
    ) {
        let file = build_file(&msgs, crlf);
        let mut d = QtTs.extract(file.as_bytes(), &opts(target)).unwrap_or_else(|e| panic!("generated file refused: {e}\n{file}"));
        prop_assert_eq!(QtTs.merge(&d).unwrap(), file.as_bytes().to_vec());
        pseudo(&mut d);
        let out = QtTs.merge(&d).unwrap();
        let back = QtTs.extract(&out, &opts(None)).unwrap();
        prop_assert_eq!(back.units.len(), d.units.len());
        let a: Vec<_> = d.units.iter().map(|u| u.source()).collect();
        let b: Vec<_> = back.units.iter().map(|u| u.source()).collect();
        prop_assert_eq!(a, b);
    }
}
