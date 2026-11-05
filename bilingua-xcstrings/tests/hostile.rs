//! Hostile input: malformed, truncated, deep and huge files give typed errors and never panic; property tests over
//! random bytes, JSON-like text, byte-mutated fixtures and generated catalogs.
mod common;
#[path = "xcloc_common/mod.rs"]
mod xcloc_common;
use bilingua_model::*;
use bilingua_xcstrings::{Xcloc, Xcstrings};
use proptest::prelude::*;

fn typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_))
}

/// Extract; when it works, translate everything and merge: neither may panic, an error must be typed.
fn exercise(f: &dyn Filter, bytes: &[u8], target: Option<&str>) -> bool {
    let o = ExtractOptions { target_lang: target.map(str::to_string), ..Default::default() };
    match f.extract(bytes, &o) {
        Err(e) => {
            assert!(typed(&e), "untyped error: {e:?}");
            false
        }
        Ok(mut d) => {
            let same = f.merge(&d);
            if let Ok(s) = &same {
                if f.id() == "xcstrings" {
                    assert_eq!(s.as_slice(), bytes, "an untouched merge must return the input");
                }
            }
            for u in d.units.iter_mut().filter(|u| u.translate) {
                u.segments[0].target = vec![Inline::text(format!("\u{ab}{}\u{bb}", plain_text(&u.segments[0].source)))];
            }
            let _ = f.merge(&d);
            true
        }
    }
}

#[test]
fn malformed_catalogs_are_parse_errors() {
    let cases: &[&[u8]] = &[
        b"",
        b"   ",
        b"{",
        b"}",
        b"{\"sourceLanguage\"",
        b"{\"sourceLanguage\":",
        b"{\"sourceLanguage\":\"en\"}",
        b"{\"sourceLanguage\":\"en\",\"strings\":[]}",
        b"{\"sourceLanguage\":1,\"strings\":{}}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{}} x",
        b"{\"sourceLanguage\":\"en\",\"strings\":{},}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{\"a\":}}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{\"a\" \"b\"}}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{\"a\":\"\\u12\"}}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{\"a\":\"\\x\"}}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{\"a\":\"line\nbreak\"}}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{\"a\":01}}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{\"a\":1.}}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{\"a\":tru}}",
        b"{\"sourceLanguage\":\"en\",\"strings\":{\"a\":\"unterminated",
        b"[]",
        b"123",
        b"\"just a string\"",
        b"null",
        b"\xff\xfe\x00\x00",
        b"\xef\xbb\xbf",
    ];
    for c in cases {
        match Xcstrings.extract(c, &ExtractOptions::default()) {
            Err(e) => assert!(typed(&e), "{c:?}: {e:?}"),
            Ok(_) => panic!("{c:?} must be refused"),
        }
    }
}

#[test]
fn every_prefix_of_a_catalog_is_refused_or_read() {
    for data in [common::LOCOS_CATALOG, include_bytes!("data/tabs_en_us.xcstrings")] {
        for n in 0..data.len() {
            exercise(&Xcstrings, &data[..n], Some("de"));
        }
    }
}

#[test]
fn very_deep_json_is_refused_without_overflowing_the_stack() {
    for open in ["[", "{\"a\":", "{\"sourceLanguage\":\"en\",\"strings\":{\"k\":{\"localizations\":{\"en\":{\"variations\":{\"plural\":{\"one\":"] {
        let big = open.repeat(200_000);
        let e = Xcstrings.extract(big.as_bytes(), &ExtractOptions::default()).unwrap_err();
        assert!(typed(&e), "{e:?}");
    }
    // 100 000 levels that never close
    let e = Xcstrings.extract("[".repeat(100_000).as_bytes(), &ExtractOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::XmlDepth) || matches!(e, Error::Limit(_)), "{e:?}");
}

#[test]
fn deep_but_legal_nesting_below_a_localization_is_refused() {
    let nested = format!(
        "{{\"sourceLanguage\":\"en\",\"strings\":{{\"k\":{{\"localizations\":{{\"en\":{}\"x\"{}}}}}}}}}",
        "{\"substitutions\":{\"a\":".repeat(20),
        "}}".repeat(20)
    );
    let e = Xcstrings.extract(nested.as_bytes(), &ExtractOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::XmlDepth), "{e:?}");
}

#[test]
fn huge_inputs_hit_the_limits() {
    let o = ExtractOptions { limits: Some(Limits { max_input_bytes: 100, ..Limits::default() }), ..Default::default() };
    let e = Xcstrings.extract(common::LOCOS_CATALOG, &o).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::InputBytes));
    // 2 million empty entries: refused before a tree is built
    let big = format!("{{\"sourceLanguage\":\"en\",\"strings\":{{{}}}}}", (0..300_000).map(|i| format!("\"k{i}\":{{}}")).collect::<Vec<_>>().join(","));
    let e = Xcstrings.extract(big.as_bytes(), &ExtractOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::Units), "{e:?}");
    // one unit with a 150 000 character value
    let long = format!("{{\"sourceLanguage\":\"en\",\"strings\":{{\"k\":{{\"localizations\":{{\"en\":{{\"stringUnit\":{{\"value\":\"{}\"}}}}}}}}}}}}", "a".repeat(150_000));
    let e = Xcstrings.extract(long.as_bytes(), &ExtractOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::UnitChars), "{e:?}");
    // more than 50 000 translatable units
    let many = format!("{{\"sourceLanguage\":\"en\",\"strings\":{{{}}}}}", (0..50_001).map(|i| format!("\"Key number {i}\":{{}}")).collect::<Vec<_>>().join(","));
    let e = Xcstrings.extract(many.as_bytes(), &ExtractOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::Units), "{e:?}");
}

#[test]
fn a_hostile_skeleton_cannot_break_a_merge() {
    let mut d = Xcstrings.extract(common::LOCOS_CATALOG, &ExtractOptions::default()).unwrap();
    if let Skeleton::Text { parts, .. } = &mut d.skeleton {
        *parts = vec![SkelPart::Raw("{\"sourceLanguage\":\"en\",\"strings\":".into())];
    }
    assert!(Xcstrings.merge(&d).is_err());
    let mut d = Xcstrings.extract(common::LOCOS_CATALOG, &ExtractOptions::default()).unwrap();
    d.skeleton = Skeleton::Text { parts: vec![SkelPart::Raw("{}".into())], encoding: "utf-16le".into(), parser: None };
    assert!(Xcstrings.merge(&d).is_err());
    // units that do not exist, and ids that are not ours
    let mut d = Xcstrings.extract(common::LOCOS_CATALOG, &ExtractOptions::default()).unwrap();
    d.units[0].id = "u999999".into();
    d.units[1].id = "zzz".into();
    d.units[0].segments[0].target = vec![Inline::text("x")];
    d.units[1].segments[0].target = vec![Inline::text("y")];
    assert_eq!(Xcstrings.merge(&d).unwrap(), common::LOCOS_CATALOG);
}

#[test]
fn hostile_bundles() {
    let src = xcloc_common::Bundle::default().build();
    // every prefix of the archive
    for n in (0..src.len()).step_by(7) {
        exercise(&Xcloc, &src[..n], Some("de"));
    }
    // a skeleton whose archive was swapped for something else
    let mut d = Xcloc.extract(&src, &ExtractOptions::default()).unwrap();
    if let Skeleton::Zip { entries, .. } = &mut d.skeleton {
        entries[0].data = ZipData::Raw(b"PK\x03\x04 broken".to_vec());
    }
    assert!(Xcloc.merge(&d).is_err());
    let mut d = Xcloc.extract(&src, &ExtractOptions::default()).unwrap();
    d.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(Xcloc.merge(&d).is_err());
    // zip entry names that climb out of the folder
    let bad = bilingua_zip::write_zip(&[(
        bilingua_zip::EntryMeta { name: "../evil/contents.json".into(), stored: false, dir: false, modified: None },
        b"{}".to_vec(),
    )]);
    if let Ok(b) = bad {
        assert!(Xcloc.extract(&b, &ExtractOptions::default()).is_err());
    }
}

fn mutate(data: &[u8], edits: &[(usize, u8)], cut: Option<usize>) -> Vec<u8> {
    let mut v = data.to_vec();
    for (i, b) in edits {
        if !v.is_empty() {
            let at = i % v.len();
            v[at] = *b;
        }
    }
    if let Some(c) = cut {
        v.truncate(c % (v.len() + 1));
    }
    v
}

// Characters that matter to the reader, so random text gets somewhere.
fn jsonish() -> impl Strategy<Value = String> {
    let atoms = prop::sample::select(vec![
        "{", "}", "[", "]", ",", ":", "\"", "\\", "\\u00e9", "\\ud83d", "\\n", "sourceLanguage", "strings", "localizations", "stringUnit", "variations", "plural", "device", "substitutions",
        "value", "state", "en", "ru", "one", "other", "few", "comment", "shouldTranslate", "false", "extractionState", "stale", "%@", "%1$@", "%lld", "%#@x@", "%arg", " ", "\n", "\t", "1", "-", "null", "é", "日本",
    ]);
    prop::collection::vec(atoms, 0..80).prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..600)) {
        exercise(&Xcstrings, &bytes, None);
        exercise(&Xcloc, &bytes, None);
    }

    #[test]
    fn random_pk_prefixed_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..600)) {
        let mut v = b"PK\x03\x04".to_vec();
        v.extend(bytes);
        exercise(&Xcloc, &v, Some("ja"));
    }

    #[test]
    fn json_like_text_never_panics(s in jsonish(), wrap in any::<bool>()) {
        let text = if wrap { format!("{{\"sourceLanguage\":\"en\",\"strings\":{{\"k\":{s}}}}}") } else { s };
        exercise(&Xcstrings, text.as_bytes(), Some("ru"));
        exercise(&Xcstrings, text.as_bytes(), None);
    }

    #[test]
    fn mutated_catalogs_never_panic(edits in prop::collection::vec((any::<usize>(), any::<u8>()), 0..8), cut in prop::option::of(any::<usize>()), which in 0..2usize, lang in prop::sample::select(vec!["de", "ar", "ja", "ru"])) {
        let base: &[u8] = if which == 0 { common::LOCOS_CATALOG } else { include_bytes!("data/tabs_en_us.xcstrings") };
        exercise(&Xcstrings, &mutate(base, &edits, cut), Some(lang));
    }

    #[test]
    fn mutated_bundles_never_panic(edits in prop::collection::vec((any::<usize>(), any::<u8>()), 0..6), cut in prop::option::of(any::<usize>())) {
        let base = xcloc_common::Bundle::default().build();
        exercise(&Xcloc, &mutate(&base, &edits, cut), Some("de"));
    }

    /// Generated valid catalogs: the units read back unchanged after a translated merge into any language.
    #[test]
    fn generated_catalogs_keep_their_source(
        entries in prop::collection::vec(
            (
                "[A-Za-z %@0-9.]{1,12}",
                prop::option::of("[A-Za-z %@.\u{e9}\u{65e5}]{1,20}"),
                prop::bool::ANY,
                prop::bool::ANY,
                prop::option::of("[a-z ]{0,10}"),
            ),
            0..12,
        ),
        lang in prop::sample::select(vec!["de", "ar", "ja", "zh-Hans", "ko", "ru", "pl", "pt_BR", "fr", "he", "tr"]),
        tabs in any::<bool>(),
    ) {
        let mut strings = serde_json::Map::new();
        for (key, value, plural, skip, comment) in &entries {
            let mut e = serde_json::Map::new();
            if let Some(c) = comment { e.insert("comment".into(), c.clone().into()); }
            if *skip { e.insert("shouldTranslate".into(), false.into()); }
            if let Some(v) = value {
                let su = serde_json::json!({"stringUnit": {"state": "translated", "value": v}});
                let loc = if *plural {
                    serde_json::json!({"variations": {"plural": {"one": su.clone(), "other": su}}})
                } else { su };
                e.insert("localizations".into(), serde_json::json!({"en": loc}));
            }
            strings.insert(key.clone(), e.into());
        }
        let doc = serde_json::json!({"sourceLanguage": "en", "strings": strings, "version": "1.0"});
        let bytes = if tabs { serde_json::to_vec_pretty(&doc).unwrap() } else { serde_json::to_vec(&doc).unwrap() };
        let d0 = Xcstrings.extract(&bytes, &ExtractOptions { target_lang: Some(lang.into()), ..Default::default() }).unwrap();
        prop_assert_eq!(Xcstrings.merge(&d0).unwrap(), bytes.clone());
        let mut d = d0.clone();
        for u in d.units.iter_mut() {
            let (t, map) = locos::to_locos_text(&u.source());
            u.segments[0].target = locos::from_locos_text(&format!("\u{ab}{t}\u{bb}"), &map);
        }
        let out = Xcstrings.merge(&d).unwrap();
        let back = Xcstrings.extract(&out, &ExtractOptions::default()).unwrap();
        let src = |d: &Document| d.units.iter().map(|u| locos::to_locos_text(&u.source()).0).collect::<Vec<_>>();
        prop_assert_eq!(src(&back), src(&d0));
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        prop_assert!(v["strings"].is_object());
    }
}
