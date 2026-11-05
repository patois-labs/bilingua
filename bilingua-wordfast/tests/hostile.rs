//! Hostile input: typed errors and no panics, over random bytes, random structured XML and mutated fixtures.

mod common;

use bilingua_model::*;
use common::*;
use proptest::prelude::*;

const SEEDS: [&[u8]; 8] = [
    include_bytes!("data/locos_sample.txml"),
    include_bytes!("data/locos_sample.txlf"),
    include_bytes!("data/edge_inline.txml"),
    include_bytes!("data/edge_segsource.txlf"),
    include_bytes!("data/lang_ja_utf8bom_crlf.txml"),
    include_bytes!("data/lang_ar_utf16le.txlf"),
    include_bytes!("data/Test02.html.txml"),
    include_bytes!("data/lang_ko_utf16be.txml"),
];

/// Extract with every filter and option; when it works: an untouched merge is the same bytes and a pseudo
/// translated merge reads back with the same unit count.
fn check(data: &[u8]) {
    let filters: [(&dyn Filter, bool); 4] = [(&TXML, false), (&TXLF, false), (&TXML_T, true), (&TXLF_T, true)];
    for (f, loads) in filters {
        let _ = f.sniff(data);
        for locked in [false, true] {
            let o = ExtractOptions { include_locked: locked, ..opts() };
            let Ok(d) = f.extract(data, &o) else { continue };
            assert!(f.merge(&d).expect("merge") == data, "untouched merge");
            if !loads {
                continue; // segments written to a file leave this list once they hold a translation
            }
            let mut e = d.clone();
            for u in e.units.iter_mut().filter(|u| u.translate) {
                let mut t = vec![Inline::text("[")];
                t.extend(u.source());
                t.push(Inline::text("]"));
                u.segments[0].target = t;
            }
            let out = f.merge(&e).expect("pseudo merge");
            let back = f.extract(&out, &o).expect("re-extract");
            assert_eq!(back.units.len(), d.units.len(), "unit count after translation");
        }
    }
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<txml locale=\"en\" targetlocale=\"fr\">".to_string()),
        Just("</txml>".to_string()),
        Just("<xliff version=\"1.2\" xmlns:gs4tr=\"http://www.gs4tr.org/schema/xliff-ext\">".to_string()),
        Just("</xliff>".to_string()),
        Just("<file source-language=\"en\">".to_string()),
        Just("<body>".to_string()),
        Just("<trans-unit id=\"1\" gs4tr:locked=\"true\">".to_string()),
        Just("<trans-unit id=\"2\">".to_string()),
        Just("</trans-unit>".to_string()),
        Just("<seg-source>".to_string()),
        Just("<mrk mtype=\"seg\" mid=\"1\">".to_string()),
        Just("</mrk>".to_string()),
        Just("<translatable blockId=\"1\">".to_string()),
        Just("</translatable>".to_string()),
        Just("<localizable>".to_string()),
        Just("<segment segmentId=\"1\" locked=\"true\">".to_string()),
        Just("<segment gtmt=\"true\">".to_string()),
        Just("</segment>".to_string()),
        Just("<source>".to_string()),
        Just("</source>".to_string()),
        Just("<target score=\"50\">".to_string()),
        Just("<target/>".to_string()),
        Just("</target>".to_string()),
        Just("<ws>".to_string()),
        Just("</ws>".to_string()),
        Just("<ut type=\"start\" x=\"1\">".to_string()),
        Just("<ut type=\"end\" x=\"1\">".to_string()),
        Just("<ut x=\"2\"/>".to_string()),
        Just("</ut>".to_string()),
        Just("<g id=\"1\">".to_string()),
        Just("</g>".to_string()),
        Just("<x id=\"1\"/>".to_string()),
        Just("<ph id=\"2\">{0}</ph>".to_string()),
        Just("<!--".to_string()),
        Just("-->".to_string()),
        Just("<![CDATA[".to_string()),
        Just("]]>".to_string()),
        Just("<?pi x?>".to_string()),
        Just("<?xml version=\"1.0\"?>".to_string()),
        Just("<!DOCTYPE a [<!ENTITY e \"x\">]>".to_string()),
        Just("&amp;".to_string()),
        Just("&#x41;".to_string()),
        Just("&#0;".to_string()),
        Just("&e;".to_string()),
        Just("&".to_string()),
        Just("<".to_string()),
        Just(">".to_string()),
        Just("\"".to_string()),
        Just("=".to_string()),
        Just(" ".to_string()),
        Just("\r\n".to_string()),
        Just("Hello".to_string()),
        Just("caf\u{e9}".to_string()),
        Just("\u{65e5}\u{672c}".to_string()),
        Just("\u{645}\u{631}".to_string()),
        Just("\u{feff}".to_string()),
        Just("\u{1f600}".to_string()),
    ];
    prop::collection::vec(piece, 0..60).prop_map(|v| v.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..600)) {
        check(&data);
    }

    #[test]
    fn structured_soup_round_trips(data in soup()) {
        check(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(seed in 0usize..8, flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8), cut in prop::option::of(any::<prop::sample::Index>())) {
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
    fn every_prefix_of_a_fixture_is_handled(seed in 0usize..8, cut in any::<prop::sample::Index>()) {
        let data = &SEEDS[seed][..cut.index(SEEDS[seed].len() + 1)];
        check(data);
    }
}

fn refused(f: &dyn Filter, data: &[u8]) {
    assert!(f.extract(data, &opts()).is_err(), "{}", String::from_utf8_lossy(&data[..data.len().min(80)]));
}

#[test]
fn entity_bombs_and_external_entities_are_refused() {
    let bomb = "<?xml version=\"1.0\"?><!DOCTYPE lolz [<!ENTITY lol \"lol\"><!ENTITY lol2 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\">]><txml><translatable><segment><source>&lol2;</source></segment></translatable></txml>";
    refused(&TXML, bomb.as_bytes());
    refused(&TXLF, bomb.as_bytes());
    // a plain DOCTYPE is refused too (LocOS refuses every DOCTYPE)
    refused(&TXML, b"<!DOCTYPE txml SYSTEM \"http://example.com/x.dtd\"><txml/>");
    refused(&TXML, b"<?xml version=\"1.0\"?><!DOCTYPE r [<!ENTITY % p SYSTEM \"file:///etc/passwd\"> %p;]><txml/>");
    // an entity used without any DTD is undefined; so are bad character references
    refused(&TXML, b"<txml><translatable><segment><source>&lol;</source></segment></translatable></txml>");
    refused(&TXML, b"<txml><translatable><segment><source>&#0;</source></segment></translatable></txml>");
    refused(&TXML, b"<txml><translatable><segment><source>&#xD800;</source></segment></translatable></txml>");
    refused(&TXML, b"<txml><translatable><segment a=\"&#x110000;\"><source>x</source></segment></translatable></txml>");
}

#[test]
fn deep_nesting_is_a_typed_limit_error() {
    for depth in [300usize, 5_000, 200_000] {
        let doc = format!("<txml>{}x{}</txml>", "<a>".repeat(depth), "</a>".repeat(depth));
        match TXML.extract(doc.as_bytes(), &opts()) {
            Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlDepth),
            other => panic!("depth {depth}: {other:?}"),
        }
    }
    // 250 nested still reads
    let ok = format!("<txml><translatable>{}<segment><source>deep</source></segment>{}</translatable></txml>", "<a>".repeat(250), "</a>".repeat(250));
    let d = TXML.extract(ok.as_bytes(), &opts()).unwrap();
    // the segment is not directly under a block: no units, no error
    assert_eq!(d.units.len(), 0);
    // a deeply nested inline element is one point
    let inline = format!("<txml><translatable><segment><source>a <ut>{}x{}</ut></source></segment></translatable></txml>", "<b>".repeat(250), "</b>".repeat(250));
    assert_eq!(TXML.extract(inline.as_bytes(), &opts()).unwrap().units.len(), 1);
    let g = format!("<xliff version=\"1.2\"><file><body><trans-unit id=\"1\"><source>{}x{}</source></trans-unit></body></file></xliff>", "<g id=\"1\">".repeat(250), "</g>".repeat(250));
    let d = TXLF.extract(g.as_bytes(), &opts()).unwrap();
    assert_eq!(d.units[0].segments[0].source.len(), 501);
    assert_eq!(TXLF.merge(&d).unwrap(), g.as_bytes());
}

#[test]
fn malformed_and_truncated_files_are_typed_errors() {
    for s in [
        "",
        "   ",
        "plain text",
        "<",
        "<txml",
        "<txml>",
        "<txml></q>",
        "<txml/><txml/>",
        "<txml/>trailing",
        "<txml a=1/>",
        "<txml a=\"1\" a=\"2\"/>",
        "<txml a=\"<\"/>",
        "<1txml/>",
        "<p:txml/>",
        "<txml p:a=\"1\"/>",
        "<txml><!-- a -- b --></txml>",
        "<txml>]]></txml>",
        "<txml>&amp</txml>",
        "<txml>\u{1}</txml>",
        "<?xml version=\"1.0\"?><?xml version=\"1.0\"?><txml/>",
        "<txml><![CDATA[x</txml>",
        "<txml><!-- x </txml>",
        "<txml><translatable><segment><source>a</segment>",
    ] {
        let e = TXML.extract(s.as_bytes(), &opts());
        assert!(matches!(e, Err(Error::Parse { .. }) | Err(Error::Encoding(_))), "{s:?}: {e:?}");
        assert!(TXLF.extract(s.as_bytes(), &opts()).is_err(), "{s:?}");
    }
    // a truncated UTF-16 file, an odd byte count, a lone surrogate
    for s in [&b"\xff\xfe<\x00t\x00"[..], b"\xff\xfe<", b"\xff\xfe\x00\xd8<\x00", b"\xef\xbb\xbf\xff\xfe"] {
        assert!(TXML.extract(s, &opts()).is_err());
    }
    // a declared charset the filter does not support is refused, even on a plain ASCII file
    assert!(matches!(TXML.extract(b"<?xml version=\"1.0\" encoding=\"utf-32\"?><txml/>", &opts()), Err(Error::Encoding(_))));
    // a doubled byte order mark
    assert!(TXML.extract(b"\xef\xbb\xbf\xef\xbb\xbf<txml/>", &opts()).is_err());
    assert!(TXLF.extract(b"\xef\xbb\xbf\xef\xbb\xbf<xliff version=\"1.2\"/>", &opts()).is_err());
    // XML 1.0 rules: no control characters, no U+FFFE
    assert!(TXML.extract("<txml>\u{fffe}</txml>".as_bytes(), &opts()).is_err());
    // a TXLF 2.0 file is not a TXLF 1.2 file
    assert!(TXLF.extract(b"<xliff version=\"2.0\" xmlns=\"urn:oasis:names:tc:xliff:document:2.0\"><file/></xliff>", &opts()).is_err());
}

#[test]
fn limits_give_typed_errors() {
    let small = ExtractOptions { limits: Some(Limits { max_input_bytes: 100, ..Limits::default() }), ..opts() };
    let e = TXML.extract(&[b'a'; 200], &small).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::InputBytes));
    let few = ExtractOptions { limits: Some(Limits { max_xml_elements: 10, ..Limits::default() }), ..opts() };
    let doc = format!("<txml>{}</txml>", "<a/>".repeat(100));
    assert!(matches!(TXML.extract(doc.as_bytes(), &few), Err(Error::Limit(_))));
    let chars = ExtractOptions { limits: Some(Limits { max_unit_chars: 10, ..Limits::default() }), ..opts() };
    let doc = "<txml><translatable><segment><source>this one is far too long</source></segment></translatable></txml>";
    assert!(matches!(TXML.extract(doc.as_bytes(), &chars), Err(Error::Limit(ref l)) if l.kind == LimitKind::UnitChars));
}

#[test]
fn big_and_odd_inputs_are_handled() {
    let long = format!("<txml><translatable><segment><source>{}</source></segment></translatable></txml>", "a".repeat(90_000));
    let d = TXML.extract(long.as_bytes(), &opts()).unwrap();
    assert_eq!(TXML.merge(&d).unwrap(), long.as_bytes());
    // many attributes, many comments, many inline codes
    let attrs: String = (0..500).map(|i| format!(" a{i}=\"{i}\"")).collect();
    let doc = format!("<txml><translatable><segment{attrs}><source>x</source></segment></translatable></txml>");
    assert_eq!(TXML.extract(doc.as_bytes(), &opts()).unwrap().units.len(), 1);
    let codes: String = (0..2000).map(|i| format!("a<ut x=\"{i}\">&lt;i&gt;</ut>")).collect();
    let doc = format!("<txml><translatable><segment><source>{codes}</source></segment></translatable></txml>");
    let d = TXML.extract(doc.as_bytes(), &opts()).unwrap();
    assert_eq!(d.units[0].segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count(), 2000);
    assert_eq!(TXML.merge(&d).unwrap(), doc.as_bytes());
}
