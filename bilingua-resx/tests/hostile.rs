//! Hostile input: typed errors and no panics, over random bytes, structured XML soup and mutated fixtures.

mod common;

use bilingua_model::*;
use bilingua_resx::{Limits, Resx};
use common::*;
use proptest::prelude::*;

const SEEDS: [&[u8]; 4] = [
    include_bytes!("data/Strings.resx"),
    include_bytes!("data/okapi_Test01.resx"),
    include_bytes!("data/okapi_test_with_placeholders.resx"),
    include_bytes!("../../oracle/corpus/resx/lumen_ui.resx"),
];

/// Extract, and when it works: untouched merge is the same bytes; a pseudo-translated merge reads back with the
/// same unit count and tag count.
fn check(data: &[u8]) {
    let Ok(d) = Resx.extract(data, &opts()) else { return };
    assert_eq!(Resx.merge(&d).expect("merge"), data, "untouched merge");
    let out = Resx.merge(&pseudo(&d)).expect("pseudo merge");
    let back = Resx.extract(&out, &opts()).expect("re-extract");
    assert_eq!(back.units.len(), d.units.len(), "unit count after translation");
    let tags = |d: &Document| d.units.iter().map(|u| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum::<usize>();
    assert_eq!(tags(&back), tags(&d), "tag count after translation");
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<root>".to_string()),
        Just("</root>".to_string()),
        Just("<data name=\"a\">".to_string()),
        Just("<data name='b' xml:space=\"preserve\">".to_string()),
        Just("<data name=\"t\" type=\"System.Int32\">".to_string()),
        Just("</data>".to_string()),
        Just("<value>".to_string()),
        Just("</value>".to_string()),
        Just("<value/>".to_string()),
        Just("<comment>".to_string()),
        Just("</comment>".to_string()),
        Just("<![CDATA[".to_string()),
        Just("]]>".to_string()),
        Just("<!--".to_string()),
        Just("-->".to_string()),
        Just("<?pi x?>".to_string()),
        Just("&amp;".to_string()),
        Just("&lt;".to_string()),
        Just("&#13;".to_string()),
        Just("&#x1F600;".to_string()),
        Just("&#0;".to_string()),
        Just("&bogus;".to_string()),
        Just("{0}".to_string()),
        Just("{{x}}".to_string()),
        Just("{name:N2}".to_string()),
        Just("Hello world".to_string()),
        Just("\u{65e5}\u{672c}\u{8a9e}".to_string()),
        Just("  ".to_string()),
        Just("\n".to_string()),
        Just("\r\n".to_string()),
        Just("\r".to_string()),
        Just("<b>".to_string()),
        Just("</b>".to_string()),
        Just("<x/>".to_string()),
        Just("<".to_string()),
        Just(">".to_string()),
        Just("&".to_string()),
        Just("\"".to_string()),
    ];
    prop::collection::vec(piece, 0..60).prop_map(|v| {
        let mut s = String::from("<root>");
        s.extend(v);
        s.push_str("</root>");
        s.into_bytes()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..600)) {
        let _ = Resx.extract(&data, &opts());
        let _ = Resx.sniff(&data);
        let _ = Resx.records(&data);
        check(&data);
    }

    #[test]
    fn structured_soup_round_trips(data in soup()) {
        check(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(seed in 0usize..4, flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8), cut in prop::option::of(any::<prop::sample::Index>())) {
        let mut data = SEEDS[seed].to_vec();
        for (i, b) in flips {
            let at = i.index(data.len());
            data[at] = b;
        }
        if let Some(c) = cut {
            data.truncate(c.index(data.len() + 1));
        }
        check(&data);
    }

    #[test]
    fn every_prefix_of_a_fixture_is_handled(seed in 0usize..4, cut in any::<prop::sample::Index>()) {
        let data = &SEEDS[seed][..cut.index(SEEDS[seed].len() + 1)];
        check(data);
    }
}

fn is_err(data: &[u8]) -> bool {
    Resx.extract(data, &opts()).is_err()
}

#[test]
fn malformed_xml_is_a_typed_error() {
    let cases: &[&[u8]] = &[
        b"",
        b"   ",
        b"\xEF\xBB\xBF",
        b"<root>",
        b"<root><data name=\"a\"><value>x</value></data>",
        b"<root><data name=\"a\"><value>x</data></value></root>",
        b"<root></rooot>",
        b"</root>",
        b"<root/><root/>",
        b"text before <root/>",
        b"<root/> text after",
        b"<root><data name=\"a\" name=\"b\"><value>x</value></data></root>",
        b"<root><data name=a><value>x</value></data></root>",
        b"<root><data name=\"a\"><value>&bogus;</value></data></root>",
        b"<root><data name=\"a\"><value>&#0;</value></data></root>",
        b"<root><data name=\"a\"><value>&#xD800;</value></data></root>",
        b"<root><data name=\"a\"><value>&#99999999;</value></data></root>",
        b"<root><data name=\"a&bogus;\"><value>x</value></data></root>",
        b"<root><data name=\"a\"><value>\x01</value></data></root>",
        b"<root><data name=\"a\"><value>x</value></data><!-- unterminated",
        b"<root><data name=\"a\"><value><![CDATA[unterminated</value></data></root>",
        b"<?xml version=\"1.0\"?>",
        b"<!DOCTYPE root [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><root/>",
        b"\xFF\xFE\x00\x00<\x00\x00\x00",
        b"\xFF\xFE<\x00r",
        b"\xFF\xFE\x00\xD8",
        b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff\x00\x00",
    ];
    for c in cases {
        match Resx.extract(c, &opts()) {
            Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Encoding(_) | Error::Limit(_)) => {}
            other => panic!("{:?}: {:?}", String::from_utf8_lossy(c), other.map(|d| d.units.len())),
        }
    }
}

#[test]
fn entity_bombs_and_external_entities_are_refused_not_expanded() {
    let mut bomb = String::from("<?xml version=\"1.0\"?><!DOCTYPE root [<!ENTITY l0 \"lol\">");
    for i in 1..10 {
        let prev = format!("&l{};", i - 1).repeat(10);
        bomb.push_str(&format!("<!ENTITY l{i} \"{prev}\">"));
    }
    bomb.push_str("]><root><data name=\"a\"><value>&l9;</value></data></root>");
    let e = Resx.extract(bomb.as_bytes(), &opts()).unwrap_err().to_string();
    assert!(e.to_lowercase().contains("dtd") || e.to_lowercase().contains("entit"), "{e}");
    let xxe = b"<?xml version=\"1.0\"?><!DOCTYPE root [<!ENTITY x SYSTEM \"file:///C:/Windows/win.ini\">]><root><data name=\"a\"><value>&x;</value></data></root>";
    assert!(is_err(xxe));
    // an entity declared in an attribute-only use and a parameter entity
    assert!(is_err(b"<!DOCTYPE root [<!ENTITY % p \"x\">]><root/>"));
    // a DOCTYPE without an internal subset is harmless (Microsoft files never have one, but it is well formed)
    assert!(!is_err(b"<!DOCTYPE root SYSTEM \"root.dtd\"><root><data name=\"a\"><value>Hello</value></data></root>"));
    // entity-like text inside a comment, CDATA or PI is not a declaration
    assert!(!is_err(b"<root><!-- <!ENTITY x \"y\"> --><data name=\"a\"><value><![CDATA[<!ENTITY x \"y\">]]></value></data><?p <!ENTITY?></root>"));
}

#[test]
fn deep_and_wide_input_is_bounded() {
    let mut deep = b"<root>".to_vec();
    deep.extend(b"<a>".repeat(100_000));
    assert!(is_err(&deep));
    let l = Limits::new().with_max_xml_depth(10);
    let mut ok = b"<root>".to_vec();
    ok.extend(b"<a>".repeat(8));
    ok.extend(b"</a>".repeat(8));
    ok.extend_from_slice(b"</root>");
    assert!(Resx.extract_with(&ok, &opts(), &l).is_ok());
    let mut bad = b"<root>".to_vec();
    bad.extend(b"<a>".repeat(11));
    bad.extend(b"</a>".repeat(11));
    bad.extend_from_slice(b"</root>");
    assert!(matches!(Resx.extract_with(&bad, &opts(), &l), Err(Error::Limit(e)) if e.kind == LimitKind::XmlDepth));
    let wide = format!("<root>{}</root>", "<data name=\"k\"><value>Word</value></data>".repeat(2000));
    let l = Limits::new().with_max_units(1000);
    assert!(matches!(Resx.extract_with(wide.as_bytes(), &opts(), &l), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
}

#[test]
fn size_limits_give_typed_errors() {
    let small = Limits::new().with_max_input_bytes(100);
    let err = Resx.extract_with(&[b'a'; 200], &opts(), &small).unwrap_err();
    assert!(matches!(err, Error::Limit(e) if e.kind == LimitKind::InputBytes));
    let l = Limits::new().with_max_unit_chars(50);
    let long = format!("<root><data name=\"a\"><value>{}</value></data></root>", "word ".repeat(100));
    assert!(matches!(Resx.extract_with(long.as_bytes(), &opts(), &l), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
}

#[test]
fn long_values_attributes_and_many_references_are_handled() {
    let big = format!("<root><data name=\"{0}\"><value>{1}</value></data></root>", "n".repeat(100_000), "&amp;".repeat(100_000));
    let d = Resx.extract_with(big.as_bytes(), &opts(), &Limits::new().with_max_unit_chars(1_000_000));
    // "&&&&" has no letter or digit: no unit, but no trouble either
    assert_eq!(d.unwrap().units.len(), 0);
    let text = format!("<root><data name=\"a\"><value>{}</value></data></root>", "Word &amp; ".repeat(10_000));
    let d = Resx.extract_with(text.as_bytes(), &opts(), &Limits::new().with_max_unit_chars(1_000_000)).unwrap();
    assert_eq!(d.units.len(), 1);
    assert_eq!(Resx.merge(&d).unwrap(), text.as_bytes());
}

#[test]
fn translations_cannot_break_the_xml() {
    let data = b"<root><data name=\"a\"><value>Hello</value></data></root>";
    for evil in ["</value></data><data name=\"x\"><value>pwned", "]]>", "<![CDATA[", "&bogus;", "<!--", "\u{0}\u{1}\u{fffe}", "x\u{b}\u{c}y"] {
        let out = translate(data, &[(0, evil)]);
        let back = Resx.extract(&out, &opts());
        let back = back.unwrap_or_else(|e| panic!("{evil:?}: {e}"));
        assert!(back.units.len() <= 1, "{evil:?}: no injected entries");
    }
}

/// B-side 9 (verify_xml_resource_filters:410, MAX_DOCUMENT_SEGMENTS=3): LocOS `check_xml_budget` refuses a file
/// with more than 40 x MAX_DOCUMENT_SEGMENTS `<` before parsing, code `limit`.
#[test]
fn xml_budget_before_parsing() {
    let mut o = ExtractOptions::default();
    o.limits = Some(bilingua_model::Limits::new().with_max_units(3));
    let flood = format!("<root>{}</root>", "<x/>".repeat(300));
    let e = Resx.extract(flood.as_bytes(), &o).unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::Units && l.msg.contains("far more entries")), "{e:?}");
    assert!(!matches!(Resx.extract(flood.as_bytes(), &ExtractOptions::default()), Err(Error::Limit(_))));
}
