//! Hostile input: typed errors and no panics, over random bytes, random structured XML and mutated fixtures.

mod common;

use bilingua_model::*;
use bilingua_ttml::Ttml;
use common::*;
use proptest::prelude::*;

const SEEDS: [&[u8]; 6] = [
    include_bytes!("data/locos_suite_tt.ttml"),
    include_bytes!("data/edge.ttml"),
    include_bytes!("data/lang_ja_shiftjis.ttml"),
    include_bytes!("data/lang_ar_utf16le.ttml"),
    include_bytes!("data/dfxp_2006_10.dfxp"),
    include_bytes!("data/lang_ar.ttml"),
];

/// Extract, and when it works: untouched merge is the same bytes; a pseudo-translated merge (plain and stamped for
/// Arabic) reads back with the same unit count.
fn check(data: &[u8]) {
    let _ = F.sniff(data);
    let Ok(d) = F.extract(data, &opts()) else { return };
    assert!(F.merge(&d).expect("merge") == data, "untouched merge");
    let mut e = d.clone();
    for u in &mut e.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    for (lang, o) in [(None, MergeOptions::default()), (Some("ar"), bilingua_model::profile::locos().merge_options("xml")), (Some("en-US"), MergeOptions::default())] {
        let mut e = e.clone();
        e.target_lang = lang.map(String::from);
        let out = F.merge_with(&e, &o).expect("pseudo merge");
        let back = F.extract(&out, &opts()).expect("re-extract");
        assert_eq!(back.units.len(), d.units.len(), "unit count after translation");
    }
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:tts=\"http://www.w3.org/ns/ttml#styling\" xml:lang=\"en\">".to_string()),
        Just("</tt>".to_string()),
        Just("<body>".to_string()),
        Just("</body>".to_string()),
        Just("<div xml:space=\"preserve\">".to_string()),
        Just("</div>".to_string()),
        Just("<p begin=\"1s\" end=\"2s\" tts:direction=\"ltr\">".to_string()),
        Just("</p>".to_string()),
        Just("<span xml:lang=\"en\" tts:color=\"red\">".to_string()),
        Just("</span>".to_string()),
        Just("<br/>".to_string()),
        Just("<br>".to_string()),
        Just("</br>".to_string()),
        Just("<set begin=\"1s\"/>".to_string()),
        Just("<metadata><p>x</p></metadata>".to_string()),
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
        Just("'".to_string()),
        Just("=".to_string()),
        Just(" ".to_string()),
        Just("\r\n".to_string()),
        Just("\n".to_string()),
        Just("{0}".to_string()),
        Just("Hello".to_string()),
        Just("caf\u{e9}".to_string()),
        Just("\u{65e5}\u{672c}".to_string()),
        Just("\u{645}\u{631}".to_string()),
        Just("\u{feff}".to_string()),
        Just("\u{a0}".to_string()),
        Just("\u{1f600}".to_string()),
    ];
    // a fixed valid frame around random pieces keeps most cases past the scanner
    (any::<bool>(), prop::collection::vec(piece, 0..40)).prop_map(|(framed, v)| {
        let body = v.concat();
        if framed {
            format!("<tt xmlns=\"http://www.w3.org/ns/ttml\" xml:lang=\"en\"><body><div><p>{body}</p></div></body></tt>").into_bytes()
        } else {
            body.into_bytes()
        }
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
    fn mutated_fixtures_never_panic(seed in 0usize..6, flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8), cut in prop::option::of(any::<prop::sample::Index>())) {
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
    fn every_prefix_of_a_fixture_is_handled(seed in 0usize..6, cut in any::<prop::sample::Index>()) {
        let data = &SEEDS[seed][..cut.index(SEEDS[seed].len() + 1)];
        check(data);
    }

    /// Whatever a translator types, the merge never adds a caption, a style override or a control attribute:
    /// markup characters are text, and the file still reads back with the same units.
    #[test]
    fn a_translation_cannot_inject_markup(t in "\\PC{0,60}") {
        let data = SEEDS[0];
        let mut d = extract(data);
        d.units[0].segments[0].target = vec![Inline::text(format!("x{t}"))];
        d.target_lang = Some("de".into());
        let out = F.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).unwrap();
        let back = F.extract(&out, &opts()).unwrap();
        prop_assert_eq!(back.units.len(), 3);
        let s = String::from_utf8(out).unwrap();
        prop_assert_eq!(s.matches("<p ").count(), 4);
        prop_assert_eq!(s.matches("<span").count(), 1);
    }
}

const NS: &str = "xmlns=\"http://www.w3.org/ns/ttml\"";

fn refused(data: &[u8]) {
    assert!(Ttml.extract(data, &opts()).is_err(), "{}", String::from_utf8_lossy(&data[..data.len().min(80)]));
}

#[test]
fn entity_bombs_and_external_entities_are_refused() {
    let bomb = "<?xml version=\"1.0\"?><!DOCTYPE lolz [<!ENTITY lol \"lol\"><!ENTITY lol2 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\"><!ENTITY lol3 \"&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;\">]><tt xmlns=\"http://www.w3.org/ns/ttml\"><body><div><p>&lol3;</p></div></body></tt>";
    refused(bomb.as_bytes());
    refused(format!("<!DOCTYPE tt SYSTEM \"http://example.com/x.dtd\"><tt {NS}/>").as_bytes());
    refused(format!("<?xml version=\"1.0\"?><!DOCTYPE tt [<!ENTITY % p SYSTEM \"file:///etc/passwd\"> %p;]><tt {NS}/>").as_bytes());
    refused(format!("<tt {NS}><body><div><p>&lol;</p></div></body></tt>").as_bytes());
    refused(format!("<tt {NS}><body><div><p>&#0;</p></div></body></tt>").as_bytes());
    refused(format!("<tt {NS}><body><div><p>&#xD800;</p></div></body></tt>").as_bytes());
    refused(format!("<tt {NS} a=\"&#x110000;\"/>").as_bytes());
}

#[test]
fn deep_nesting_is_a_typed_limit_error() {
    for depth in [300usize, 5_000, 200_000] {
        let doc = format!("<tt {NS}><body><div><p>{}x{}</p></div></body></tt>", "<span>".repeat(depth), "</span>".repeat(depth));
        match Ttml.extract(doc.as_bytes(), &opts()) {
            Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlDepth),
            other => panic!("depth {depth}: {other:?}"),
        }
    }
    let ok = format!("<tt {NS}><body><div><p>{}deep{}</p></div></body></tt>", "<span>".repeat(200), "</span>".repeat(200));
    assert_eq!(texts(ok.as_bytes()).len(), 1);
}

#[test]
fn malformed_and_truncated_files_are_typed_errors() {
    for s in [
        "",
        "   ",
        "plain text",
        "<",
        "<tt",
        "<tt>",
        "<tt></q>",
        "<tt/><tt/>",
        "<tt/>trailing",
        "<tt a=1/>",
        "<tt a=\"1\" a=\"2\"/>",
        "<tt a=\"<\"/>",
        "<1tt/>",
        "<p:tt/>",
        "<tt p:a=\"1\"/>",
        "<tt><!-- a -- b --></tt>",
        "<tt>]]></tt>",
        "<tt>&amp</tt>",
        "<tt>\u{1}</tt>",
        "<?xml version=\"1.0\"?><?xml version=\"1.0\"?><tt/>",
        "<tt><![CDATA[x</tt>",
        "<tt><!-- x </tt>",
        "<tt xmlns:a=\"\"/>",
        "<tt xmlns:xmlns=\"urn:x\"/>",
        "<root xmlns=\"http://www.w3.org/ns/ttml\"/>",
        "<tt xmlns=\"urn:other\"/>",
    ] {
        let e = Ttml.extract(s.as_bytes(), &opts());
        assert!(matches!(e, Err(Error::Parse { .. })), "{s:?}: {e:?}");
    }
    let dup = "<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:a=\"urn:a\" xmlns:b=\"urn:a\" a:x=\"1\" b:x=\"2\"/>";
    assert!(matches!(Ttml.extract(dup.as_bytes(), &opts()), Err(Error::Parse { .. })));
    for s in [&b"\xff\xfe<\x00t\x00"[..], b"\xff\xfe<", b"\xff\xfe\x00\xd8<\x00", b"\xef\xbb\xbf\xff\xfe"] {
        assert!(Ttml.extract(s, &opts()).is_err());
    }
}

#[test]
fn limits_give_typed_errors() {
    let small = Limits::new().with_max_input_bytes(100);
    let e = Ttml.extract_with(&[b'a'; 200], &opts(), &small).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::InputBytes));
    let few = Limits::new().with_max_xml_elements(10);
    let doc = format!("<tt {NS}><body><div>{}</div></body></tt>", "<p>a</p>".repeat(100));
    assert!(matches!(Ttml.extract_with(doc.as_bytes(), &opts(), &few), Err(Error::Limit(_))));
    let fewer = Limits::new().with_max_units(3);
    assert!(matches!(Ttml.extract_with(doc.as_bytes(), &opts(), &fewer), Err(Error::Limit(_))));
    let many_ns: String = (0..5000).map(|i| format!(" xmlns:n{i}=\"urn:n{i}\"")).collect();
    assert!(matches!(Ttml.extract(format!("<tt {NS}{many_ns}/>").as_bytes(), &opts()), Err(Error::Limit(_))));
}

#[test]
fn big_and_odd_inputs_are_handled() {
    let long = format!("<tt {NS}><body><div><p>{}</p></div></body></tt>", "a".repeat(90_000));
    assert!(Ttml.extract(long.as_bytes(), &opts()).is_ok());
    let attrs: String = (0..20_000).map(|i| format!(" a{i}=\"v\"")).collect();
    check(format!("<tt {NS}{attrs}><body><div><p>x</p></div></body></tt>").as_bytes());
    refused(format!("<tt {NS} a=\"1\" a=\"2\"/>").as_bytes());
    check(format!("<tt {NS}><body><div><p>x{}</p></div></body></tt>", "<!-- c -->".repeat(50_000)).as_bytes());
    check(b"");
    check(b"\xEF\xBB\xBF");
    check(b"\n\n\n");
}

#[test]
fn doubled_bom_is_a_typed_error() {
    for bom in [2usize, 3] {
        let mut data = b"\xEF\xBB\xBF".repeat(bom);
        data.extend_from_slice(format!("<tt {NS}><body><div><p>Hello</p></div></body></tt>").as_bytes());
        assert!(Ttml.extract(&data, &opts()).is_err());
    }
    let mut data = b"\xEF\xBB\xBF".to_vec();
    data.extend_from_slice(format!("<tt {NS}><body><div><p>Hello</p></div></body></tt>").as_bytes());
    assert!(Ttml.extract(&data, &opts()).is_ok());
}

/// A foreign skeleton or a tampered one is a typed error, never a panic.
#[test]
fn tampered_skeletons_are_typed_errors() {
    let mut d = extract(SEEDS[0]);
    d.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(F.merge(&d).is_err());
    let mut d = extract(SEEDS[0]);
    if let Skeleton::Text { parts, .. } = &mut d.skeleton {
        parts.push(SkelPart::Unit(99));
    }
    assert!(F.merge(&d).is_err());
    let mut d = extract(SEEDS[0]);
    d.units[0].set_hidden("meta", "zzz");
    d.units[0].segments[0].target = vec![Inline::text("x")];
    assert!(F.merge(&d).is_err());
    let mut d = extract(SEEDS[0]);
    d.target_lang = Some("ar".into());
    d.units[0].set_hidden("meta", "99999|0|br");
    d.units[0].segments[0].target = vec![Inline::text("x")];
    assert!(F.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).is_err());
}
