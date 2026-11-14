//! Hostile input: typed errors and no panics, over random bytes, random structured XML and mutated fixtures.

mod common;

use bilingua_model::*;
use bilingua_sdlproj::{flip_package_type, project_info, Sdlproj};
use common::*;
use proptest::prelude::*;

include!("data_list.inc");

/// Extract, and when it works: untouched merge is the same bytes; a pseudo-translated merge (nothing to
/// translate) reads back with the same unit count.
fn check(data: &[u8]) {
    let _ = Sdlproj.sniff(data);
    let _ = project_info(data);
    let _ = flip_package_type(data);
    let Ok(d) = Sdlproj.extract(data, &opts()) else { return };
    assert!(Sdlproj.merge(&d).expect("merge") == data, "untouched merge");
    let back = Sdlproj.extract(&Sdlproj.merge(&d).expect("merge"), &opts()).expect("re-extract");
    assert_eq!(back.units.len(), d.units.len());
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<PackageProject Guid=\"g\" PackageType=\"ProjectPackage\" Name=\"n\">".to_string()),
        Just("</PackageProject>".to_string()),
        Just("<Project xmlns:p=\"urn:x\">".to_string()),
        Just("</Project>".to_string()),
        Just("<LanguageDirections>".to_string()),
        Just("</LanguageDirections>".to_string()),
        Just("<LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"de-DE\"/>".to_string()),
        Just("<LanguageDirection SourceLanguageCode='fr' TargetLanguageCode=\"\"/>".to_string()),
        Just("<p:X p:a=\"1\"/>".to_string()),
        Just("<!--".to_string()),
        Just("-->".to_string()),
        Just("<![CDATA[".to_string()),
        Just("]]>".to_string()),
        Just("<?pi x?>".to_string()),
        Just("<?xml version=\"1.0\"?>".to_string()),
        Just("<?xml version=\"1.0\" encoding=\"shift_jis\"?>".to_string()),
        Just("<!DOCTYPE a [<!ENTITY e \"x\">]>".to_string()),
        Just("<!DOCTYPE a>".to_string()),
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
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..600)) {
        check(&data);
    }

    #[test]
    fn random_markup_never_panics(data in soup()) {
        check(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(seed in 0usize..FILES.len(), edits in prop::collection::vec((any::<usize>(), any::<u8>(), 0u8..3), 0..6)) {
        let mut data = FILES[seed].1.to_vec();
        for (at, byte, op) in edits {
            if data.is_empty() { break; }
            let i = at % data.len();
            match op {
                0 => data[i] = byte,
                1 => { data.remove(i); }
                _ => data.insert(i, byte),
            }
        }
        check(&data);
    }

    #[test]
    fn truncated_fixtures_never_panic(seed in 0usize..FILES.len(), cut in any::<usize>()) {
        let data = FILES[seed].1;
        check(&data[..cut % (data.len() + 1)]);
    }
}

#[test]
fn every_truncation_of_the_demo_is_a_typed_error_or_a_read() {
    let data = fixture(&FILES, "locos_demo");
    let mut ok = 0;
    for n in 0..data.len() - 1 {
        match Sdlproj.extract(&data[..n], &opts()) {
            Ok(_) => ok += 1,
            Err(e) => assert!(!e.to_string().is_empty()),
        }
    }
    assert_eq!(ok, 0, "a truncated project file never reads");
}

#[test]
fn deep_and_huge_files_are_limit_errors() {
    let deep = format!("{}{}", "<a>".repeat(300), "</a>".repeat(300));
    assert!(matches!(Sdlproj.extract(deep.as_bytes(), &opts()), Err(Error::Limit(_))));
    let l = Limits::new().with_max_input_bytes(100);
    let data = fixture(&FILES, "locos_demo");
    assert!(matches!(Sdlproj.extract_with(data, &opts(), &l), Err(Error::Limit(_))));
    let l = Limits::new().with_max_xml_elements(3);
    assert!(matches!(Sdlproj.extract_with(data, &opts(), &l), Err(Error::Limit(_))));
    // a million nested open tags is refused before any tree exists
    let wide = "<a>".repeat(1_000_000);
    assert!(matches!(Sdlproj.extract(wide.as_bytes(), &opts()), Err(Error::Limit(_))));
}

#[test]
fn malformed_xml_is_a_parse_error() {
    for bad in [
        &b""[..],
        b"not xml",
        b"<a><b></a></b>",
        b"<Project><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/>",
        b"<Project a=\"1\" a=\"2\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"<Project><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project><Project/>",
        b"<Project>&nope;<LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"<x:Project><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></x:Project>",
        b"<Project n=\"\x01\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"junk<Project><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"<Project n=\"a<b\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"<Project>]]><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b" <?xml version=\"1.0\"?><Project><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"<Project><!-- a -- b --><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"<Project n=\"a & b\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"<Project n=\"&#0;\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"<Project x:a=\"1\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
        b"<Project n=\"Caf\xe9\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>",
    ] {
        match Sdlproj.extract(bad, &opts()) {
            Err(Error::Parse { .. }) | Err(Error::Encoding(_)) => {}
            other => panic!("{:?} gave {:?}", String::from_utf8_lossy(bad), other.map(|d| d.units.len())),
        }
    }
}

#[test]
fn a_skeleton_from_another_parser_or_format_is_refused_on_merge() {
    let mut d = extract(fixture(&FILES, "locos_demo"));
    d.skeleton = Skeleton::zip(Vec::new(), ParserInfo::new("bilingua-sdlproj", "0.1.0", 99));
    assert!(Sdlproj.merge(&d).is_err());
}
