//! Hostile and malformed input: typed errors, never a panic. Includes a property test over random bytes and
//! byte-mutated fixtures.

mod common;
use bilingua_idml::Idml;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn fixture() -> Pkg {
    let body = format!(
        "{}\n{}",
        psr(&format!(
            "{}{}",
            csr("<Content>Hello <?ACE 7?>there\t</Content><HyperlinkTextSource Self=\"h\"><Content>link</Content></HyperlinkTextSource>"),
            csr_b("<Content>bold</Content><Br/><Content>Next</Content>", BOLD)
        )),
        psr(&csr("<Content>Para</Content><Footnote><ParagraphStyleRange><CharacterStyleRange><Content>Note</Content></CharacterStyleRange></ParagraphStyleRange></Footnote>"))
    );
    Pkg::one(body)
}

fn try_all(data: &[u8]) {
    if let Ok(mut doc) = Idml.extract(data, &ExtractOptions::new().with_target_lang("ar-SA")) {
        let _ = Idml.merge(&doc);
        for u in &mut doc.units {
            let mut t = u.segments[0].source.clone();
            t.reverse();
            t.push(Inline::text(" x\r\n\u{1}"));
            u.segments[0].target = t;
        }
        let _ = Idml.merge(&doc);
    }
}

fn mutate(bytes: &mut Vec<u8>, seed: &[(usize, u8, u8)]) {
    for (pos, op, val) in seed {
        if bytes.is_empty() {
            return;
        }
        let p = pos % bytes.len();
        match op % 4 {
            0 => bytes[p] = *val,
            1 => {
                bytes.remove(p);
            }
            2 => bytes.insert(p, *val),
            _ => bytes.truncate(p.max(1)),
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = Idml.extract(&bytes, &ExtractOptions::default());
        let mut with_sig = b"PK\x03\x04".to_vec();
        with_sig.extend(&bytes);
        let _ = Idml.extract(&with_sig, &ExtractOptions::default());
    }

    #[test]
    fn mutated_xml_parts_never_panic(
        which in 0usize..3,
        edits in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 1..12),
    ) {
        let mut members = fixture().members();
        let name = ["Stories/Story_a.xml", "designmap.xml", "Spreads/Spread_s1.xml"][which];
        for m in &mut members {
            if m.0 == name {
                mutate(&mut m.1, &edits);
            }
        }
        try_all(&zip_of(&members));
    }

    #[test]
    fn mutated_zip_bytes_never_panic(edits in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 1..8)) {
        let mut data = fixture().build();
        mutate(&mut data, &edits);
        try_all(&data);
    }

    #[test]
    fn random_targets_never_panic(picks in proptest::collection::vec((0usize..8, "[a-z \\n\\t]{0,12}"), 0..12)) {
        let data = fixture().build();
        let mut doc = extract(&data);
        for u in &mut doc.units {
            let n = u.codes.len();
            let mut t = vec![];
            for (k, s) in &picks {
                match k % 4 {
                    0 => t.push(Inline::text(s.clone())),
                    1 => t.push(Inline::tag(((k / 4) % (n + 2)).to_string(), TagKind::Open, "")),
                    2 => t.push(Inline::tag(((k / 4) % (n + 2)).to_string(), TagKind::Close, "")),
                    _ => t.push(Inline::tag(((k / 4) % (n + 2)).to_string(), TagKind::Standalone, "")),
                }
            }
            u.segments[0].target = t;
        }
        let out = Idml.merge(&doc).unwrap();
        // whatever the target said, the result is still a readable package with every element once
        let t = story_tree(&out, "a");
        prop_assert_eq!(named(&t, "HyperlinkTextSource").len(), 1);
        prop_assert_eq!(named(&t, "Footnote").len(), 1);
        prop_assert_eq!(named(&t, "Br").len(), 1);
        prop_assert!(Idml.extract(&out, &ExtractOptions::default()).is_ok());
    }
}

#[test]
fn every_truncation_of_a_package_is_a_typed_error() {
    let data = fixture().build();
    let step = (data.len() / 120).max(1);
    for n in (0..data.len()).step_by(step) {
        assert!(Idml.extract(&data[..n], &ExtractOptions::default()).is_err(), "prefix {n} must not parse");
    }
}

fn with_story(xml: String) -> Vec<u8> {
    let mut m = Pkg::one(String::new()).members();
    for e in &mut m {
        if e.0 == "Stories/Story_a.xml" {
            e.1 = xml.clone().into_bytes();
        }
    }
    zip_of(&m)
}

#[test]
fn deep_nesting_is_refused_and_does_not_overflow_the_stack() {
    for depth in [300usize, 5_000, 200_000] {
        let body = format!("<Content>x</Content>{}<Content>y</Content>{}", "<HyperlinkTextSource>".repeat(depth), "</HyperlinkTextSource>".repeat(depth));
        let xml = String::from_utf8(story_xml("a", &psr(&csr(&body)))).unwrap();
        let r = Idml.extract(&with_story(xml), &ExtractOptions::default());
        assert!(r.is_err(), "depth {depth}");
    }
}

#[test]
fn nesting_just_under_the_limit_works() {
    let depth = 200usize;
    let body = format!("<Content>x</Content>{}<Content>deep</Content>{}", "<HyperlinkTextSource>".repeat(depth), "</HyperlinkTextSource>".repeat(depth));
    let xml = String::from_utf8(story_xml("a", &psr(&csr(&body)))).unwrap();
    let data = with_story(xml);
    let mut doc = extract(&data);
    assert_eq!(doc.units.len(), 1);
    let src = doc.units[0].segments[0].source.clone();
    doc.units[0].segments[0].target = src.into_iter().map(|i| if let Inline::Text(t) = i { Inline::text(t.to_uppercase()) } else { i }).collect();
    let out = merge(&doc);
    assert!(raw_story(&out, "a").contains("DEEP"));
}

#[test]
fn entity_bombs_and_doctypes_are_refused() {
    let bomb = "<?xml version=\"1.0\"?><!DOCTYPE lolz [<!ENTITY lol \"lol\"><!ENTITY lol2 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\"><!ENTITY lol3 \"&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;\">]><idPkg:Story xmlns:idPkg=\"x\"><Story><ParagraphStyleRange><CharacterStyleRange><Content>&lol3;</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>";
    let e = Idml.extract(&with_story(bomb.into()), &ExtractOptions::default()).err().unwrap();
    assert!(matches!(e, Error::Limit(_)), "{e}");
    let plain_doctype = "<?xml version=\"1.0\"?><!DOCTYPE x SYSTEM \"http://example.invalid/x.dtd\"><idPkg:Story xmlns:idPkg=\"x\"><Story/></idPkg:Story>";
    assert!(Idml.extract(&with_story(plain_doctype.into()), &ExtractOptions::default()).is_err());
    // an undefined entity is not expanded to anything
    let undefined = String::from_utf8(story_xml("a", &psr(&csr("<Content>&xxe;</Content>")))).unwrap();
    assert!(Idml.extract(&with_story(undefined), &ExtractOptions::default()).is_err());
    // the same in the design map
    let mut m = fixture().members();
    for e in &mut m {
        if e.0 == "designmap.xml" {
            e.1 = bomb.as_bytes().to_vec();
        }
    }
    assert!(Idml.extract(&zip_of(&m), &ExtractOptions::default()).is_err());
}

#[test]
fn limits_apply() {
    let data = fixture().build();
    let few = ExtractOptions::new().with_limits(Limits::new().with_max_units(1));
    assert!(matches!(Idml.extract(&data, &few), Err(Error::Limit(_))));
    let small = ExtractOptions::new().with_limits(Limits::new().with_max_input_bytes(100));
    assert!(matches!(Idml.extract(&data, &small), Err(Error::Limit(_))));
    let entries = ExtractOptions::new().with_limits(Limits::new().with_max_zip_entries(3));
    assert!(matches!(Idml.extract(&data, &entries), Err(Error::Limit(_))));
    let elements = ExtractOptions::new().with_limits(Limits::new().with_max_xml_elements(5));
    assert!(matches!(Idml.extract(&data, &elements), Err(Error::Limit(_))));
    // a story with a million empty elements
    let body = format!("<Content>x</Content>{}", "<a/>".repeat(1_000_000));
    let xml = String::from_utf8(story_xml("a", &psr(&csr(&body)))).unwrap();
    let opts = ExtractOptions::new().with_limits(Limits::new().with_max_xml_elements(100_000));
    assert!(matches!(Idml.extract(&with_story(xml), &opts), Err(Error::Limit(_))));
    // the markup budget: forty tags per allowed unit
    let many = format!("<Content>x</Content>{}", "<a/>".repeat(500));
    let xml = String::from_utf8(story_xml("a", &psr(&csr(&many)))).unwrap();
    let opts = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    assert!(matches!(Idml.extract(&with_story(xml), &opts), Err(Error::Limit(_))));
}

#[test]
fn zip_level_tricks_are_refused() {
    let good = fixture();
    // a story member name with a parent-folder segment, an absolute path, a drive path
    for bad in ["../Stories/Story_a.xml", "/Stories/Story_a.xml", "C:/Stories/Story_a.xml", "Stories/../../x.xml"] {
        let m: Vec<(String, Vec<u8>)> = good.members().into_iter().map(|(n, d)| (if n == "Stories/Story_a.xml" { bad.to_string() } else { n }, d)).collect();
        assert!(Idml.extract(&zip_of(&m), &ExtractOptions::default()).is_err(), "{bad}");
    }
    // case-only duplicates
    let mut m = good.members();
    m.push(("STORIES/story_a.xml".into(), b"<x/>".to_vec()));
    assert!(Idml.extract(&zip_of(&m), &ExtractOptions::default()).is_err());
    // mimetype that claims another format
    let m: Vec<(String, Vec<u8>)> = good.members().into_iter().map(|(n, d)| if n == "mimetype" { (n, b"application/epub+zip".to_vec()) } else { (n, d) }).collect();
    assert!(Idml.extract(&zip_of(&m), &ExtractOptions::default()).is_err());
}

#[test]
fn a_tampered_skeleton_is_an_error_not_a_panic() {
    let data = fixture().build();
    let doc = extract(&data);
    let mut bad = doc.clone();
    bad.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(Idml.merge(&bad).is_err() || bad.units.iter().all(|u| u.segments[0].target.is_empty()));
    let mut t = doc.clone();
    t.units[0].segments[0].target = vec![Inline::text("x")];
    t.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(Idml.merge(&t).is_err());
    let mut garbage = doc.clone();
    garbage.units[0].segments[0].target = vec![Inline::text("x")];
    if let Skeleton::Zip { entries, .. } = &mut garbage.skeleton {
        entries[0].data = ZipData::Raw(b"PK\x03\x04 not really".to_vec());
    }
    assert!(Idml.merge(&garbage).is_err());
    let mut text_skel = doc.clone();
    text_skel.skeleton = Skeleton::default();
    assert!(Idml.merge(&text_skel).is_err());
    let mut other = doc.clone();
    if let Skeleton::Zip { parser, .. } = &mut other.skeleton {
        *parser = Some(ParserInfo::new("bilingua-docx", "0.1.0", 1));
    }
    assert!(matches!(Idml.merge(&other), Err(Error::ParserVersion { .. })));
    // units that name no paragraph, and tags that are not numbers, are ignored
    let mut odd = doc.clone();
    odd.units[0].id = "nonsense".into();
    odd.units[0].segments[0].target = vec![Inline::text("x")];
    assert_eq!(merge(&odd), data);
    let mut tags = doc.clone();
    tags.units[0].segments[0].target = vec![Inline::tag("99999999999999999999999", TagKind::Open, ""), Inline::tag("abc", TagKind::Standalone, ""), Inline::text("ok")];
    let out = merge(&tags);
    assert!(raw_story(&out, "a").contains("ok"));
}

#[test]
fn control_characters_in_a_target_never_reach_the_file() {
    let data = fixture().build();
    let mut doc = extract(&data);
    doc.units[0].segments[0].target = vec![Inline::text("a\u{0}b\u{1}c\u{b}d\u{feff}e\u{fffe}f<&>")];
    let out = merge(&doc);
    let t = story_tree(&out, "a");
    let v = visible_text(&t);
    assert!(v.contains("abc") && v.contains("<&>"), "{v}");
    assert!(!v.contains('\u{0}') && !v.contains('\u{fffe}'));
}
