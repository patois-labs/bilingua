//! Hostile input: typed errors and no panics, over random bytes, random structured XML and mutated fixtures.

mod common;

use bilingua_model::*;
use bilingua_xaml::{Limits, Xaml};
use common::*;
use proptest::prelude::*;

const SEEDS: [&[u8]; 6] = [
    include_bytes!("data/locos_suite_wpf.xaml"),
    include_bytes!("data/locos_suite_view.xaml"),
    include_bytes!("data/edge.xaml"),
    include_bytes!("data/lang_ja_shiftjis.xaml"),
    include_bytes!("data/lang_ar_utf16le.xaml"),
    include_bytes!("data/view_attributes.xaml"),
];

/// Extract, and when it works: untouched merge is the same bytes; a pseudo-translated merge reads back with the
/// same unit count.
fn check(data: &[u8]) {
    for f in [&OFF, &ON] {
        let _ = f.sniff(data);
        let Ok(d) = f.extract(data, &opts()) else { continue };
        assert!(f.merge(&d).expect("merge") == data, "untouched merge");
        let mut e = d.clone();
        for u in &d.units {
            let id = u.segments[0].id.clone();
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            t.push(Inline::text("]"));
            e.set_target(&id, t).expect("set_target");
        }
        let out = f.merge(&e).expect("pseudo merge");
        let back = f.extract(&out, &opts()).expect("re-extract");
        assert_eq!(back.units.len(), d.units.len(), "unit count after translation");
    }
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<ResourceDictionary xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\" xmlns:s=\"clr-namespace:System\">".to_string()),
        Just("</ResourceDictionary>".to_string()),
        Just("<x:String x:Key=\"k\">".to_string()),
        Just("<s:String>".to_string()),
        Just("</x:String>".to_string()),
        Just("</s:String>".to_string()),
        Just("<Button Content=\"Go now\" Title='{Binding X}' Text=\"{}{0} a\"/>".to_string()),
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
        Just("{0}".to_string()),
        Just("{Binding A}".to_string()),
        Just("{".to_string()),
        Just("%s".to_string()),
        Just("Hello".to_string()),
        Just("caf\u{e9}".to_string()),
        Just("\u{65e5}\u{672c}".to_string()),
        Just("\u{645}\u{631}".to_string()),
        Just("\u{feff}".to_string()),
        Just("\u{1f600}".to_string()),
    ];
    prop::collection::vec(piece, 0..40).prop_map(|v| v.concat().into_bytes())
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
}

const NS: &str = "xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\" xmlns:s=\"clr-namespace:System\"";

fn refused(data: &[u8]) {
    assert!(Xaml::default().extract(data, &opts()).is_err(), "{}", String::from_utf8_lossy(&data[..data.len().min(80)]));
}

#[test]
fn entity_bombs_and_external_entities_are_refused() {
    let bomb = "<?xml version=\"1.0\"?><!DOCTYPE lolz [<!ENTITY lol \"lol\"><!ENTITY lol2 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\"><!ENTITY lol3 \"&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;\">]><r>&lol3;</r>";
    refused(bomb.as_bytes());
    refused(format!("<!DOCTYPE r SYSTEM \"http://example.com/x.dtd\"><r {NS}/>").as_bytes());
    refused(format!("<?xml version=\"1.0\"?><!DOCTYPE r [<!ENTITY % p SYSTEM \"file:///etc/passwd\"> %p;]><r {NS}/>").as_bytes());
    // an entity used without any DTD is undefined
    refused(format!("<r {NS}><s:String>&lol;</s:String></r>").as_bytes());
    // undefined character references
    refused(format!("<r {NS}><s:String>&#0;</s:String></r>").as_bytes());
    refused(format!("<r {NS}><s:String>&#xD800;</s:String></r>").as_bytes());
    refused(format!("<r {NS}><s:String a=\"&#x110000;\">x</s:String></r>").as_bytes());
}

#[test]
fn deep_nesting_is_a_typed_limit_error() {
    for depth in [300usize, 5_000, 200_000] {
        let doc = format!("<r {NS}>{}x{}</r>", "<a>".repeat(depth), "</a>".repeat(depth));
        match Xaml::default().extract(doc.as_bytes(), &opts()) {
            Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlDepth),
            other => panic!("depth {depth}: {other:?}"),
        }
    }
    // 250 nested still reads
    let ok = format!("<r {NS}>{}<s:String>deep</s:String>{}</r>", "<a>".repeat(250), "</a>".repeat(250));
    assert_eq!(units(ok.as_bytes()), ["deep"]);
}

#[test]
fn malformed_and_truncated_files_are_typed_errors() {
    for s in [
        "",
        "   ",
        "plain text",
        "<",
        "<r",
        "<r>",
        "<r></q>",
        "<r/><r/>",
        "<r/>trailing",
        "<r a=1/>",
        "<r a=\"1\" a=\"2\"/>",
        "<r a=\"<\"/>",
        "<1r/>",
        "<p:r/>",
        "<r p:a=\"1\"/>",
        "<r><!-- a -- b --></r>",
        "<r>]]></r>",
        "<r>&amp</r>",
        "<r>\u{1}</r>",
        "<?xml version=\"1.0\"?><?xml version=\"1.0\"?><r/>",
        "<r><![CDATA[x</r>",
        "<r><!-- x </r>",
    ] {
        let e = Xaml::default().extract(s.as_bytes(), &opts());
        assert!(matches!(e, Err(Error::Parse { .. })), "{s:?}: {e:?}");
    }
    // a truncated UTF-16 file, an odd byte count, a lone surrogate
    for s in [&b"\xff\xfe<\x00r\x00"[..], b"\xff\xfe<", b"\xff\xfe\x00\xd8<\x00", b"\xef\xbb\xbf\xff\xfe"] {
        assert!(Xaml::default().extract(s, &opts()).is_err());
    }
    // a declared charset the filter does not know, on a file that is plain ASCII, reads as UTF-8
    assert!(Xaml::default().extract(b"<?xml version=\"1.0\" encoding=\"utf-32\"?><r/>", &opts()).is_ok());
}

#[test]
fn limits_give_typed_errors() {
    let small = Limits { max_input_bytes: 100, ..Limits::default() };
    let e = Xaml::default().extract_with(&[b'a'; 200], &opts(), &small).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::InputBytes));
    let few = Limits { max_xml_elements: 10, ..Limits::default() };
    let doc = format!("<r {NS}>{}</r>", "<a/>".repeat(100));
    assert!(matches!(Xaml::default().extract_with(doc.as_bytes(), &opts(), &few), Err(Error::Limit(_))));
}

#[test]
fn big_and_odd_inputs_are_handled() {
    // a very long string, a long attribute list, many comments
    let long = format!("<r {NS}><s:String>{}</s:String></r>", "a".repeat(90_000));
    assert!(Xaml::default().extract(long.as_bytes(), &opts()).is_ok());
    let attrs: String = (0..20_000).map(|i| format!(" a{i}=\"v\"")).collect();
    check(format!("<r {NS}{attrs}><s:String>x</s:String></r>").as_bytes());
    refused(format!("<r {NS} a=\"1\" a=\"2\"/>").as_bytes());
    check(format!("<r {NS}>{}</r>", "<!-- c -->".repeat(50_000)).as_bytes());
    check(b"");
    check(b"\xEF\xBB\xBF");
    check(b"\n\n\n");
}
