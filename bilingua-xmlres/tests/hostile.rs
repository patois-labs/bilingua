//! Hostile input: typed errors and no panics, over random bytes, resource-shaped XML soup and mutated fixtures.
//! Whatever opens must merge untouched to the same bytes and read back after a pseudo-translated merge.

use bilingua_model::*;
use bilingua_xmlres::{AndroidStrings, JavaXmlProperties};
use proptest::prelude::*;

const SEEDS: [&[u8]; 2] = [include_bytes!("data/strings.xml"), include_bytes!("data/messages.xml")];

fn pseudo(d: &Document) -> Document {
    let mut t = d.clone();
    let todo: Vec<(String, Vec<Inline>)> = t
        .units
        .iter()
        .filter(|u| u.translate)
        .flat_map(|u| u.segments.iter().map(|s| (s.id.clone(), [vec![Inline::text("Zq'\"&<")], s.source.clone(), vec![Inline::text("\\")]].concat())))
        .collect();
    for (id, v) in todo {
        let _ = t.set_target(&id, v);
    }
    t
}

fn check(f: &dyn Filter, data: &[u8]) {
    // no target language: the plural set stays as it is, so the unit count must come back the same
    let o = ExtractOptions::new();
    let Ok(d) = f.extract(data, &o) else { return };
    assert_eq!(f.merge(&d).expect("untouched merge"), data, "untouched merge");
    let out = f.merge(&pseudo(&d)).expect("pseudo merge");
    let back = f.extract(&out, &o).expect("re-extract");
    assert_eq!(back.units.iter().filter(|u| u.translate).count(), d.units.iter().filter(|u| u.translate).count(), "unit count after translation");
    // with a target language the plural fill runs too (es-ES adds `many` and drops `few`): it must still read back
    let mut d = d;
    d.target_lang = Some("es-ES".into());
    let out = f.merge(&pseudo(&d)).expect("localized pseudo merge");
    f.extract(&out, &o).expect("re-extract after the plural fill");
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<resources>"),
        Just("</resources>"),
        Just("<properties>"),
        Just("</properties>"),
        Just("<string name=\"a\">"),
        Just("<string name='b' translatable=\"false\">"),
        Just("</string>"),
        Just("<plurals name=\"p\">"),
        Just("</plurals>"),
        Just("<item quantity=\"one\">"),
        Just("<item quantity=\"other\">"),
        Just("<item>"),
        Just("</item>"),
        Just("<string-array name=\"s\">"),
        Just("</string-array>"),
        Just("<entry key=\"k\">"),
        Just("</entry>"),
        Just("<b>"),
        Just("</b>"),
        Just("<br/>"),
        Just("<xliff:g id=\"x\">"),
        Just("</xliff:g>"),
        Just("<![CDATA["),
        Just("]]>"),
        Just("<!-- c -->"),
        Just("<?pi x?>"),
        Just("&amp;"),
        Just("&#13;"),
        Just("&bogus;"),
        Just("\\'"),
        Just("\\"),
        Just("\""),
        Just("@"),
        Just("{0}"),
        Just("''"),
        Just("Hello world"),
        Just("\u{65e5}\u{672c}"),
        Just(" "),
        Just("\n"),
        Just("\r\n"),
    ];
    proptest::collection::vec(piece, 0..40).prop_map(|v| v.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        check(&AndroidStrings, &bytes);
        check(&JavaXmlProperties, &bytes);
    }

    #[test]
    fn soup_never_panics_and_round_trips(bytes in soup()) {
        check(&AndroidStrings, &bytes);
        check(&JavaXmlProperties, &bytes);
        let _ = AndroidStrings.sniff(&bytes);
    }

    #[test]
    fn mutated_fixtures_never_panic(seed in 0usize..2, at in 0usize..4096, val in any::<u8>(), cut in any::<bool>()) {
        let mut b = SEEDS[seed].to_vec();
        let i = at % b.len();
        if cut { b.truncate(i) } else { b[i] = val }
        check(&AndroidStrings, &b);
        check(&JavaXmlProperties, &b);
    }
}

#[test]
fn every_error_is_typed() {
    for data in [&b"\xff\xfe\x00"[..], b"<resources>\x01</resources>", b"<resources><string>&#0;</string></resources>", b"<resources><string name=\"a\" b=&>x</string></resources>"] {
        match AndroidStrings.extract(data, &ExtractOptions::default()) {
            Err(Error::Parse { .. }) | Err(Error::Limit(_)) | Err(Error::Encoding(_)) | Ok(_) => {}
            Err(e) => panic!("{e:?}"),
        }
    }
}
