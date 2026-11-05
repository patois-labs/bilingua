//! Hostile input: typed errors and no panics, over random bytes, random structured XML and mutated fixtures.

mod common;

use bilingua_model::*;
use bilingua_ttx::{Limits, Ttx};
use common::*;
use proptest::prelude::*;

const SEEDS: [&[u8]; 7] = [
    include_bytes!("data/locos_suite_sample.ttx"),
    include_bytes!("data/okapi_test01.ttx"),
    include_bytes!("data/lang_ja.ttx"),
    include_bytes!("data/lang_es.ttx"),
    include_bytes!("data/lang_pt.ttx"),
    include_bytes!("data/okapi_issue_164.ttx"),
    include_bytes!("data/lang_fr.ttx"),
];

fn open_opts() -> ExtractOptions {
    opts().with_include_locked(true)
}

/// Extract, and when it works: untouched merge is the same bytes; a pseudo-translated merge reads back with the
/// same unit count.
fn check(data: &[u8]) {
    for f in [&DEFAULT, &LOCOS] {
        let _ = f.sniff(data);
        let Ok(d) = f.extract(data, &open_opts()) else { continue };
        assert!(f.merge(&d).expect("merge") == data, "untouched merge");
        let mut e = d.clone();
        for u in &d.units {
            if !u.translate {
                continue;
            }
            let id = u.segments[0].id.clone();
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            t.push(Inline::text("]"));
            e.set_target(&id, t).expect("set_target");
        }
        let out = f.merge(&e).expect("pseudo merge");
        let back = DEFAULT.extract(&out, &open_opts()).expect("re-extract");
        assert_eq!(back.units.len(), DEFAULT.extract(data, &open_opts()).expect("again").units.len(), "unit count after translation");
    }
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<TRADOStag Version=\"2.0\">".to_string()),
        Just("</TRADOStag>".to_string()),
        Just("<FrontMatter>".to_string()),
        Just("</FrontMatter>".to_string()),
        Just("<UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\"/>".to_string()),
        Just("<UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"\">".to_string()),
        Just("</UserSettings>".to_string()),
        Just("<Body><Raw>".to_string()),
        Just("</Raw></Body>".to_string()),
        Just("<Tu MatchPercent=\"0\">".to_string()),
        Just("<Tu>".to_string()),
        Just("</Tu>".to_string()),
        Just("<Tuv Lang=\"EN-US\">".to_string()),
        Just("<Tuv Lang=\"DE-DE\">".to_string()),
        Just("<Tuv Lang=\"DE-DE\"/>".to_string()),
        Just("</Tuv>".to_string()),
        Just("<ut Type=\"start\" DisplayText=\"b\">&lt;b&gt;</ut>".to_string()),
        Just("<ut Type=\"end\" DisplayText=\"b\">&lt;/b&gt;</ut>".to_string()),
        Just("<ut DisplayText=\"br\">&lt;br&gt;</ut>".to_string()),
        Just("<ut/>".to_string()),
        Just("<df Bold=\"on\">".to_string()),
        Just("</df>".to_string()),
        Just("<df/>".to_string()),
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
        Just("Hello world.".to_string()),
        Just(" caf\u{e9} ".to_string()),
        Just("\u{65e5}\u{672c}".to_string()),
        Just("\u{645}\u{631}".to_string()),
        Just("\u{feff}".to_string()),
        Just("\u{1f600}".to_string()),
    ];
    prop::collection::vec(piece, 0..50).prop_map(|v| v.concat().into_bytes())
}

/// A soup inside a valid skeleton, so the interesting paths (units, targets, tags) are reached often.
fn inside() -> impl Strategy<Value = Vec<u8>> {
    soup().prop_map(|b| {
        let mut v = b"<TRADOStag><FrontMatter><UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\"/></FrontMatter><Body><Raw>".to_vec();
        v.extend(b);
        v.extend(b"</Raw></Body></TRADOStag>");
        v
    })
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
    fn soup_inside_a_ttx_round_trips(data in inside()) {
        check(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(seed in 0usize..7, flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8), cut in prop::option::of(any::<prop::sample::Index>())) {
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
    fn every_prefix_of_a_fixture_is_handled(seed in 0usize..7, cut in any::<prop::sample::Index>()) {
        let data = &SEEDS[seed][..cut.index(SEEDS[seed].len() + 1)];
        check(data);
    }
}

const HEAD: &str = "<TRADOStag><FrontMatter><UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\"/></FrontMatter><Body><Raw>";
const TAIL: &str = "</Raw></Body></TRADOStag>";

fn wrap(inner: &str) -> Vec<u8> {
    format!("{HEAD}{inner}{TAIL}").into_bytes()
}

fn refused(data: &[u8]) {
    assert!(DEFAULT.extract(data, &opts()).is_err(), "{}", String::from_utf8_lossy(&data[..data.len().min(80)]));
}

#[test]
fn entity_bombs_and_external_entities_are_refused() {
    let bomb = "<?xml version=\"1.0\"?><!DOCTYPE lolz [<!ENTITY lol \"lol\"><!ENTITY lol2 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\"><!ENTITY lol3 \"&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;\">]><TRADOStag>&lol3;</TRADOStag>";
    refused(bomb.as_bytes());
    refused(b"<!DOCTYPE TRADOStag SYSTEM \"http://example.com/x.dtd\"><TRADOStag/>");
    refused(b"<?xml version=\"1.0\"?><!DOCTYPE r [<!ENTITY % p SYSTEM \"file:///etc/passwd\"> %p;]><TRADOStag/>");
    // the same in UTF-16 (the usual TTX encoding)
    let mut v = vec![0xFF, 0xFE];
    for u in bomb.encode_utf16() {
        v.extend_from_slice(&u.to_le_bytes());
    }
    refused(&v);
    // an entity used without any DTD is undefined; undefined character references
    refused(&wrap("<Tu><Tuv Lang=\"EN-US\">&lol;</Tuv></Tu>"));
    refused(&wrap("<Tu><Tuv Lang=\"EN-US\">&#0;</Tuv></Tu>"));
    refused(&wrap("<Tu><Tuv Lang=\"EN-US\">&#xD800;</Tuv></Tu>"));
    refused(&wrap("<Tu><Tuv Lang=\"EN-US\" a=\"&#x110000;\">x</Tuv></Tu>"));
}

#[test]
fn deep_nesting_is_a_typed_limit_error() {
    for depth in [300usize, 5_000, 200_000] {
        let doc = format!("<TRADOStag>{}x{}</TRADOStag>", "<a>".repeat(depth), "</a>".repeat(depth));
        match DEFAULT.extract(doc.as_bytes(), &opts()) {
            Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlDepth),
            other => panic!("depth {depth}: {other:?}"),
        }
    }
    // 250 nested still reads
    let ok = format!("<TRADOStag>{}<Tu><Tuv Lang=\"EN-US\">deep</Tuv></Tu>{}</TRADOStag>", "<a>".repeat(250), "</a>".repeat(250));
    assert_eq!(units_with(&DEFAULT, ok.as_bytes()), [(0, "deep".to_string())]);
    // deep inline codes inside one Tuv
    let inline = wrap(&format!("<Tu><Tuv Lang=\"EN-US\">{}x{}</Tuv></Tu>", "<df>".repeat(200), "</df>".repeat(200)));
    let d = DEFAULT.extract(&inline, &opts()).unwrap();
    assert_eq!(d.units.len(), 1);
    assert_eq!(DEFAULT.merge(&d).unwrap(), inline);
}

#[test]
fn malformed_and_truncated_files_are_typed_errors() {
    for s in [
        "",
        "   ",
        "plain text",
        "<",
        "<TRADOStag",
        "<TRADOStag>",
        "<TRADOStag></q>",
        "<TRADOStag/><TRADOStag/>",
        "<TRADOStag/>trailing",
        "<TRADOStag a=1/>",
        "<TRADOStag a=\"1\" a=\"2\"/>",
        "<TRADOStag a=\"<\"/>",
        "<TRADOStag a=\"1\"b=\"2\"/>",
        "<1r/>",
        "<p:TRADOStag/>",
        "<TRADOStag p:a=\"1\"/>",
        "<TRADOStag><!-- a -- b --></TRADOStag>",
        "<TRADOStag>]]></TRADOStag>",
        "<TRADOStag>&amp</TRADOStag>",
        "<TRADOStag>\u{1}</TRADOStag>",
        "<?xml version=\"1.0\"?><?xml version=\"1.0\"?><TRADOStag/>",
        "<TRADOStag><![CDATA[x</TRADOStag>",
        "<TRADOStag><!-- x </TRADOStag>",
        "<xliff version=\"1.2\"/>",
    ] {
        let e = DEFAULT.extract(s.as_bytes(), &opts());
        assert!(matches!(e, Err(Error::Parse { .. })), "{s:?}: {e:?}");
    }
    // a truncated UTF-16 file, an odd byte count, a lone surrogate, a BOM on a BOM
    for s in [&b"\xff\xfe<\x00r\x00"[..], b"\xff\xfe<", b"\xff\xfe\x00\xd8<\x00", b"\xef\xbb\xbf\xff\xfe", b"\xff\xfe\xff\xfe", b"\xff\xfe\x00"] {
        assert!(DEFAULT.extract(s, &opts()).is_err(), "{s:?}");
    }
    // bytes that are not UTF-8 and not declared: refused, never guessed
    assert!(matches!(DEFAULT.extract(b"<TRADOStag>caf\xe9</TRADOStag>", &opts()), Err(Error::Encoding(_))));
}

#[test]
fn limits_give_typed_errors() {
    let small = Limits::new().with_max_input_bytes(100);
    let e = DEFAULT.extract_with(&[b'a'; 200], &opts(), &small).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::InputBytes));
    let few = Limits::new().with_max_xml_elements(10);
    let doc = wrap(&"<a/>".repeat(100));
    assert!(matches!(DEFAULT.extract_with(&doc, &opts(), &few), Err(Error::Limit(_))));
    let lines = Limits::new().with_max_lines(5);
    assert!(matches!(DEFAULT.extract_with(&wrap(&"\n".repeat(50)), &opts(), &lines), Err(Error::Limit(_))));
    let chars = Limits::new().with_max_unit_chars(10);
    let long = wrap("<Tu><Tuv Lang=\"EN-US\">this segment is far too long</Tuv></Tu>");
    assert!(matches!(DEFAULT.extract_with(&long, &opts(), &chars), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
}

#[test]
fn big_and_odd_inputs_are_handled() {
    // a very long segment, many attributes, many comments, many tiny segments
    let long = wrap(&format!("<Tu><Tuv Lang=\"EN-US\">{}</Tuv></Tu>", "a".repeat(90_000)));
    assert!(DEFAULT.extract(&long, &opts()).is_ok());
    let attrs: String = (0..5000).map(|i| format!(" a{i}=\"v\"")).collect();
    assert!(DEFAULT.extract(&wrap(&format!("<Tu{attrs}><Tuv Lang=\"EN-US\">x</Tuv></Tu>")), &opts()).is_ok());
    assert!(DEFAULT.extract(&wrap(&"<!-- c -->".repeat(10_000)), &opts()).is_ok());
    let many = wrap(&"<Tu><Tuv Lang=\"EN-US\">word</Tuv></Tu>".repeat(20_000));
    let d = DEFAULT.extract(&many, &opts()).unwrap();
    assert_eq!(d.units.len(), 20_000);
    assert_eq!(DEFAULT.merge(&d).unwrap(), many);
    // the segment cap: 50 001 segments
    let over = wrap(&"<Tu><Tuv Lang=\"EN-US\">w</Tuv></Tu>".repeat(50_001));
    assert!(matches!(DEFAULT.extract(&over, &opts()), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
}

#[test]
fn a_damaged_document_is_refused_at_merge_not_panicked_on() {
    let mut d = DEFAULT.extract(include_bytes!("data/locos_suite_sample.ttx"), &opts()).unwrap();
    d.units[0].hidden.remove("raw");
    assert!(matches!(DEFAULT.merge(&d), Err(Error::Write(_))));
    let mut d = DEFAULT.extract(include_bytes!("data/locos_suite_sample.ttx"), &opts()).unwrap();
    d.units[0].set_hidden("meta", "n\u{1f}999\u{1f}1\u{1f}2\u{1f}Tuv\u{1f}\u{1f}\u{1f}");
    d.units[0].segments[0].target = vec![Inline::text("x")];
    assert!(matches!(DEFAULT.merge(&d), Err(Error::Write(_))));
    let mut d = DEFAULT.extract(include_bytes!("data/locos_suite_sample.ttx"), &opts()).unwrap();
    d.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(DEFAULT.merge(&d).is_err());
    let mut d = DEFAULT.extract(include_bytes!("data/locos_suite_sample.ttx"), &opts()).unwrap();
    if let Skeleton::Text { parts, .. } = &mut d.skeleton {
        parts.push(SkelPart::Unit(999));
    }
    assert!(DEFAULT.merge(&d).is_err());
    let _ = Ttx::default();
}
