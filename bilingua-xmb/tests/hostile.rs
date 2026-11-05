//! Hostile input: malformed, truncated, deep and oversized files give typed errors and never panic; property tests
//! over random bytes, byte-mutated fixtures and generated bundles.

mod common;
use bilingua_model::*;
use bilingua_xmb::{Xmb, Xtb};
use common::*;
use proptest::prelude::*;

fn is_typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Limit(_) | Error::Encoding(_) | Error::Unsupported(_))
}

fn check(bytes: &[u8]) {
    match Xmb.extract(bytes, &opts()) {
        Err(e) => assert!(is_typed(&e), "untyped error: {e:?}"),
        Ok(d) => {
            // whatever is accepted must merge back to its own bytes untouched, and merge when translated
            assert_eq!(merged(&d), bytes, "untouched merge changed an accepted file");
            let mut d = d;
            pseudo(&mut d);
            let out = Xmb.merge(&d).or_else(|_| Xtb.merge(&d));
            assert!(out.is_ok(), "{out:?}");
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
        b"<?xml",
        b"<?xml version='1.0'?>",
        b"<messagebundle",
        b"<messagebundle>",
        b"<messagebundle></messagebundle",
        b"<messagebundle></translationbundle>",
        b"<messagebundle><msg id='1'>x</msg>",
        b"<messagebundle><msg id=1>x</msg></messagebundle>",
        b"<messagebundle><msg id='1' id='2'>x</msg></messagebundle>",
        b"<messagebundle><msg id='1>x</msg></messagebundle>",
        b"<messagebundle><msg id='<'>x</msg></messagebundle>",
        b"<messagebundle><msg id='&bogus;'>x</msg></messagebundle>",
        b"<messagebundle><msg id='1'>&#0;</msg></messagebundle>",
        b"<messagebundle><msg id='1'>&#xD800;</msg></messagebundle>",
        b"<messagebundle><msg id='1'>&#99999999999;</msg></messagebundle>",
        b"<messagebundle><msg id='1'>&amp</msg></messagebundle>",
        b"<messagebundle><msg id='1'>a ]]> b</msg></messagebundle>",
        b"<messagebundle><msg id='1'><![CDATA[never closed</msg></messagebundle>",
        b"<messagebundle><msg id='1'><!-- never closed</msg></messagebundle>",
        b"<messagebundle><msg id='1'><!-- a -- b --></msg></messagebundle>",
        b"<messagebundle><msg id='1'><? never closed</msg></messagebundle>",
        b"<messagebundle><msg id='1'>x</msg></messagebundle>trailing",
        b"<messagebundle/><messagebundle/>",
        b"<messagebundle><msg id='1'>\x01</msg></messagebundle>",
        b"<!DOCTYPE messagebundle [ <!ELEMENT a (b)> ",
        b"<!DOCTYPE messagebundle><!DOCTYPE x><messagebundle/>",
        b"<messagebundle><!DOCTYPE x></messagebundle>",
        b"<messagebundle><1 id='a'/></messagebundle>",
        b"<messagebundle><msg id = 'a' ></ msg></messagebundle>",
        b"\x00\x00\x00\x00",
        b"PK\x03\x04 not an xml file at all",
        b"{\"json\": true}",
    ];
    for c in cases {
        match Xmb.extract(c, &opts()) {
            Err(e) => assert!(is_typed(&e), "{:?}: {e:?}", String::from_utf8_lossy(c)),
            Ok(_) => panic!("accepted {:?}", String::from_utf8_lossy(c)),
        }
    }
}

#[test]
fn entity_bombs_and_xxe_are_refused_before_parsing() {
    let bomb = "<?xml version=\"1.0\"?><!DOCTYPE messagebundle [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\"><!ENTITY c \"&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;\">]><messagebundle><msg id=\"1\">&c;</msg></messagebundle>";
    let e = Xmb.extract(bomb.as_bytes(), &opts()).err().unwrap();
    assert!(matches!(e, Error::Limit(LimitError { kind: LimitKind::XmlEntities, .. })), "{e:?}");
    for xxe in [
        "<!DOCTYPE messagebundle [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><messagebundle><msg id=\"1\">&x;</msg></messagebundle>",
        "<!DOCTYPE messagebundle [<!ENTITY % x SYSTEM \"http://127.0.0.1/e.dtd\">%x;]><messagebundle/>",
        "<!DOCTYPE messagebundle [<!entity x 'y'>]><messagebundle/>",
        "<messagebundle><!ENTITY x 'y'></messagebundle>",
    ] {
        let e = Xmb.extract(xxe.as_bytes(), &opts()).err().unwrap();
        assert!(is_typed(&e), "{xxe}: {e:?}");
    }
    // the same inside UTF-16
    let mut u16: Vec<u8> = vec![0xFF, 0xFE];
    for u in "<!DOCTYPE messagebundle [<!ENTITY a \"b\">]><messagebundle/>".encode_utf16() {
        u16.extend_from_slice(&u.to_le_bytes());
    }
    assert!(matches!(Xmb.extract(&u16, &opts()), Err(Error::Limit(_))));
    // an undeclared entity in a file with no DTD is just a malformed file
    assert!(matches!(Xmb.extract(b"<messagebundle><msg id='1'>&xxe;</msg></messagebundle>", &opts()), Err(Error::Parse { .. })));
    // external DTD ids, in both XMB and XTB
    for root in ["messagebundle", "translationbundle"] {
        let ext = format!("<!DOCTYPE {root} SYSTEM \"http://127.0.0.1/x.dtd\"><{root}/>");
        let e = Xmb.extract(ext.as_bytes(), &opts()).err().unwrap();
        assert!(e.to_string().contains("external DTD"), "{e}");
    }
}

#[test]
fn deep_nesting_is_a_limit_error_not_a_stack_overflow() {
    for depth in [300usize, 5_000, 200_000] {
        let mut x = String::from("<messagebundle><msg id=\"1\">");
        x.push_str(&"<a>".repeat(depth));
        x.push_str(&"</a>".repeat(depth));
        x.push_str("</msg></messagebundle>");
        let e = Xmb.extract(x.as_bytes(), &opts()).err().unwrap();
        assert!(matches!(e, Error::Limit(LimitError { kind: LimitKind::XmlDepth, .. })), "{depth}: {e:?}");
    }
    // within the limit it works: deep unknown elements are one code
    let mut x = String::from("<messagebundle><msg id=\"1\">hi ");
    x.push_str(&"<a>".repeat(200));
    x.push_str(&"</a>".repeat(200));
    x.push_str("</msg></messagebundle>");
    let d = Xmb.extract(x.as_bytes(), &opts()).unwrap();
    assert_eq!(merged(&d), x.as_bytes());
    assert_eq!(d.units[0].source().len(), 2);
}

#[test]
fn oversized_input_and_unit_text_are_limit_errors() {
    let big = vec![b' '; 2000];
    let l = Limits { max_input_bytes: 1000, ..Limits::default() };
    assert!(matches!(Xmb.extract_with(&big, &opts(), &l), Err(Error::Limit(LimitError { kind: LimitKind::InputBytes, .. }))));
    let long = format!("<messagebundle><msg id='1'>{}</msg></messagebundle>", "x".repeat(200));
    let l = Limits { max_unit_chars: 100, ..Limits::default() };
    assert!(matches!(Xmb.extract_with(long.as_bytes(), &opts(), &l), Err(Error::Limit(LimitError { kind: LimitKind::UnitChars, .. }))));
    let l = Limits { max_xml_elements: 5, ..Limits::default() };
    let many = format!("<messagebundle>{}</messagebundle>", "<msg id='a'>x</msg>".repeat(10));
    assert!(matches!(Xmb.extract_with(many.as_bytes(), &opts(), &l), Err(Error::Limit(LimitError { kind: LimitKind::XmlElements, .. }))));
    // a huge attribute value and a huge text run are fine (and quick)
    let huge = format!("<messagebundle><msg id='{}'>{}</msg></messagebundle>", "i".repeat(5_000_000), "t".repeat(90_000));
    let d = Xmb.extract(huge.as_bytes(), &opts()).unwrap();
    assert_eq!(merged(&d), huge.as_bytes());
}

#[test]
fn every_prefix_of_every_fixture_is_handled() {
    for f in [XMB, XMB_LOCALIZE, XTB] {
        for n in 0..f.len() {
            check(&f[..n]);
        }
    }
}

#[test]
fn every_single_byte_deletion_and_flip_of_the_small_fixture_is_handled() {
    let f = XMB_LOCALIZE;
    for i in 0..f.len() {
        let mut del = f.to_vec();
        del.remove(i);
        check(&del);
        for b in [0u8, b'<', b'>', b'&', b'"', 0xFF] {
            let mut m = f.to_vec();
            m[i] = b;
            check(&m);
        }
    }
}

fn bundle_strategy() -> impl Strategy<Value = Vec<u8>> {
    let text = "[A-Za-z0-9 .,!?\u{e9}\u{4e16}\u{645}&<>\"']{0,24}";
    let msg = (proptest::option::of("[a-z0-9.]{1,6}"), text, proptest::bool::ANY, proptest::bool::ANY).prop_map(|(id, t, ph, pair)| {
        let esc = t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let mut inner = format!("  {esc}  ");
        if ph {
            inner.push_str("<ph name=\"PH\"><ex>e</ex>x</ph>");
        }
        if pair {
            inner = format!("<ph name=\"START_B\"/>{inner}<ph name=\"CLOSE_B\"/>");
        }
        match id {
            Some(i) => format!("<msg id=\"{i}\" desc=\"d &amp; e\"><source>f:1</source>{inner}</msg>"),
            None => format!("<msg>{inner}</msg>"),
        }
    });
    proptest::collection::vec(msg, 0..12).prop_map(|v| format!("<?xml version=\"1.0\"?>\n<messagebundle>\n{}\n</messagebundle>\n", v.join("\n")).into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, .. ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        check(&bytes);
    }

    #[test]
    fn random_markup_never_panics(s in "[<>/=\"'&;!?\\-\\[\\] a-zA-Z0-9_:#x]{0,300}") {
        check(s.as_bytes());
        let wrapped = format!("<messagebundle>{s}</messagebundle>");
        check(wrapped.as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(which in 0usize..3, edits in proptest::collection::vec((any::<proptest::sample::Index>(), any::<u8>(), 0u8..3), 1..8)) {
        let mut f = [XMB, XMB_LOCALIZE, XTB][which].to_vec();
        for (idx, b, op) in edits {
            if f.is_empty() { break; }
            let i = idx.index(f.len());
            match op { 0 => f[i] = b, 1 => { f.remove(i); } _ => f.insert(i, b) }
        }
        check(&f);
    }

    #[test]
    fn generated_bundles_round_trip(bytes in bundle_strategy()) {
        let d = Xmb.extract(&bytes, &opts()).unwrap();
        prop_assert_eq!(merged(&d), bytes.clone());
        let mut p = d.clone();
        pseudo(&mut p);
        let out = merged(&p);
        let back = Xtb.extract(&out, &opts()).unwrap();
        prop_assert_eq!(back.units.len(), d.units.len());
        prop_assert_eq!(tag_ids(&back).iter().map(Vec::len).collect::<Vec<_>>(), tag_ids(&d).iter().map(Vec::len).collect::<Vec<_>>());
        // and the XTB splices again without touching anything else
        prop_assert_eq!(merged(&back), out);
    }
}
