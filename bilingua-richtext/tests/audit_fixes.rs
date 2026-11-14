//! Regression tests for the findings of the 0.9.1 source audit.

use bilingua_model::*;
use bilingua_richtext::*;
use std::time::{Duration, Instant};

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("de-DE")
}

fn with_limits(l: Limits) -> ExtractOptions {
    opts().with_limits(l)
}

/// An ADF document with `paragraphs` one-word paragraphs under `depth` nested containers.
fn deep_adf(depth: usize, paragraphs: usize) -> Vec<u8> {
    let mut s = String::from("{\"type\":\"doc\",\"content\":[");
    for _ in 0..depth {
        s.push_str("{\"content\":[");
    }
    for i in 0..paragraphs {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"content\":[{\"type\":\"text\",\"text\":\"a\"}]}");
    }
    for _ in 0..depth {
        s.push_str("]}");
    }
    s.push_str("]}");
    s.into_bytes()
}

fn is_limit(e: &Error, kind: LimitKind) -> bool {
    matches!(e, Error::Limit(l) if l.kind == kind)
}

/// `bilingua-richtext/src/common.rs:extract_doc:unit-cap-after-plan-build`: an 8 MB file of 200 000 paragraphs
/// 120 containers deep had every plan built, each with its own copy of a 240 step path (gigabytes), before the
/// count was refused. The walk now stops one plan past the limit.
#[test]
fn a_deep_file_over_the_unit_limit_is_refused_without_planning_all_of_it() {
    let bytes = deep_adf(120, 200_000);
    let t = Instant::now();
    let e = Adf.extract(&bytes, &opts()).expect_err("200 000 units");
    assert!(is_limit(&e, LimitKind::Units), "{e:?}");
    let leaf = opts().with_format_option("adf.units", "leaf");
    let e = Adf.extract(&bytes, &leaf).expect_err("200 000 units");
    assert!(is_limit(&e, LimitKind::Units), "{e:?}");
    assert!(t.elapsed() < Duration::from_secs(10), "took {:?}", t.elapsed());
}

/// The same under a small limit, for all four filters: one unit past the limit is the unit-limit error, the
/// limit itself is accepted.
#[test]
fn every_filter_stops_at_the_unit_limit() {
    let l = Limits::default().with_max_units(50);
    let adf = |n: usize| deep_adf(3, n);
    let contentful = |n: usize| {
        let p = "{\"nodeType\":\"paragraph\",\"data\":{},\"content\":[{\"nodeType\":\"text\",\"value\":\"a\",\"marks\":[],\"data\":{}}]}";
        format!("{{\"nodeType\":\"document\",\"data\":{{}},\"content\":[{}]}}", vec![p; n].join(",")).into_bytes()
    };
    let sanity = |n: usize| {
        let b = "{\"_type\":\"block\",\"style\":\"normal\",\"markDefs\":[],\"children\":[{\"_type\":\"span\",\"text\":\"a\",\"marks\":[]}]}";
        format!("[{}]", vec![b; n].join(",")).into_bytes()
    };
    let notion = |n: usize| {
        let b = "{\"object\":\"block\",\"type\":\"paragraph\",\"paragraph\":{\"rich_text\":[{\"type\":\"text\",\"text\":{\"content\":\"a\"}}]}}";
        format!("[{}]", vec![b; n].join(",")).into_bytes()
    };
    let cases: Vec<(&dyn Filter, Box<dyn Fn(usize) -> Vec<u8>>)> =
        vec![(&Adf, Box::new(adf)), (&Contentful, Box::new(contentful)), (&Sanity, Box::new(sanity)), (&Notion, Box::new(notion))];
    for (f, make) in &cases {
        let doc = f.extract(&make(50), &with_limits(l)).unwrap_or_else(|e| panic!("{}: {e:?}", f.id()));
        assert_eq!(doc.units.len(), 50, "{}", f.id());
        for n in [51, 52, 400] {
            let e = f.extract(&make(n), &with_limits(l)).expect_err("over the unit limit");
            assert!(is_limit(&e, LimitKind::Units), "{}: {e:?}", f.id());
        }
    }
}

/// Far more strings or values than the unit limit can use are refused before the value tree is built.
#[test]
fn the_pre_parse_budget_refuses_a_file_of_markup_without_units() {
    let l = Limits::default().with_max_units(10);
    // 5 000 empty arrays: no unit at all, but 5 000 values against a budget of 10 units
    let values = format!("{{\"type\":\"doc\",\"content\":[{}]}}", vec!["[]"; 5_000].join(","));
    let quotes = format!("{{\"type\":\"doc\",\"content\":[],\"x\":\"{}\"}}", "\\\"".repeat(5_000));
    for (f, text) in [(&Adf as &dyn Filter, &values), (&Adf, &quotes), (&Notion, &values), (&Sanity, &values), (&Contentful, &values)] {
        let e = f.extract(text.as_bytes(), &with_limits(l)).expect_err("over the budget");
        assert!(is_limit(&e, LimitKind::Units), "{}: {e:?}", f.id());
    }
    // the default limits leave an ordinary document alone
    assert_eq!(Adf.extract(&deep_adf(2, 30), &opts()).expect("an ordinary file").units.len(), 30);
}

/// The path of a deep unit is kept once in the Document, as its pointer: those copies count against
/// `max_context_bytes` (the plans themselves share their paths).
#[test]
fn deep_pointers_count_against_the_context_limit() {
    let bytes = deep_adf(100, 1_000);
    let doc = Adf.extract(&bytes, &opts()).expect("within the default limits");
    assert_eq!(doc.units.len(), 1_000);
    let key = doc.units[999].context.key.clone().expect("a pointer");
    assert_eq!(key, format!("{}/content/999", "/content/0".repeat(100)));
    // about 1 000 pointers of 1 000 bytes: refused under a 100 000 byte cap
    let small = Limits::default().with_max_context_bytes(100_000);
    let e = Adf.extract(&bytes, &with_limits(small)).expect_err("over the context cap");
    assert!(is_limit(&e, LimitKind::Context), "{e:?}");
}

/// Shared paths still lead to the right node: a translation of one deep paragraph is written there and nowhere
/// else, and an untouched merge is the file.
#[test]
fn a_deep_unit_is_written_back_at_its_own_place() {
    let bytes = deep_adf(40, 300);
    for o in [opts(), opts().with_format_option("adf.units", "leaf")] {
        let mut doc = Adf.extract(&bytes, &o).expect("extract");
        assert_eq!(doc.units.len(), 300);
        assert_eq!(Adf.merge(&doc).expect("merge"), bytes);
        let sid = doc.units[123].segments[0].id.clone();
        doc.set_target(&sid, vec![Inline::text("ZZ")]).expect("set");
        let out = String::from_utf8(Adf.merge(&doc).expect("merge")).expect("utf-8");
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        let key = doc.units[123].context.key.clone().expect("a pointer");
        let node = v.pointer(&key).expect("the pointer resolves");
        let at = if key.ends_with("/content/0") && node.get("text").is_some() { node.clone() } else { node["content"][0].clone() };
        assert_eq!(at["text"], "ZZ");
        assert_eq!(out.matches("ZZ").count(), 1);
        assert_eq!(out.matches("\"text\":\"a\"").count(), 299);
    }
}

/// The four fixtures read and write as before: every unit's pointer resolves in the file, an untouched merge is
/// the file, and a translated merge reads back with every translation at its unit.
#[test]
fn the_fixtures_keep_their_pointers_and_round_trip() {
    let cases: [(&dyn Filter, &[u8]); 4] = [
        (&Contentful, include_bytes!("data/contentful.json")),
        (&Sanity, include_bytes!("data/portable.json")),
        (&Adf, include_bytes!("data/jira.json")),
        (&Notion, include_bytes!("data/notion.json")),
    ];
    for (f, bytes) in cases {
        let modes: &[bool] = if f.id() == "adf" { &[false, true] } else { &[false] };
        for leaf in modes {
            let o = if *leaf { opts().with_format_option("adf.units", "leaf") } else { opts() };
            let mut doc = f.extract(bytes, &o).unwrap_or_else(|e| panic!("{}: {e:?}", f.id()));
            assert!(!doc.units.is_empty(), "{}", f.id());
            let v: serde_json::Value = serde_json::from_slice(bytes).expect("json");
            let mut seen = std::collections::HashSet::new();
            for u in &doc.units {
                let key = u.context.key.as_deref().expect("a pointer");
                assert!(v.pointer(key).is_some(), "{}: {key} does not resolve", f.id());
                assert!(seen.insert(key.to_string()), "{}: {key} twice", f.id());
            }
            assert_eq!(f.merge(&doc).expect("merge"), bytes, "{}", f.id());
            for i in 0..doc.units.len() {
                let mut t = vec![Inline::text(format!("~{i}~"))];
                t.extend(doc.units[i].source());
                let sid = doc.units[i].segments[0].id.clone();
                doc.set_target(&sid, t).expect("set");
            }
            let out = f.merge(&doc).unwrap_or_else(|e| panic!("{}: {e:?}", f.id()));
            let back = f.extract(&out, &o).unwrap_or_else(|e| panic!("{}: {e:?}", f.id()));
            assert_eq!(back.units.len(), doc.units.len(), "{}", f.id());
            for (i, u) in back.units.iter().enumerate() {
                let text: String = u.source().iter().filter_map(|x| if let Inline::Text(t) = x { Some(t.as_str()) } else { None }).collect();
                assert!(text.starts_with(&format!("~{i}~")), "{}: unit {i} is {text:?}", f.id());
                assert_eq!(u.context.key, doc.units[i].context.key, "{}", f.id());
            }
        }
    }
}

/// `merge_doc` has the same bounds: a Document whose stored text holds far more units than the Document is
/// refused with a typed error, without planning the whole text.
#[test]
fn merge_is_bounded_like_extract() {
    let mut doc = Adf.extract(&deep_adf(2, 3), &opts()).expect("extract");
    let sid = doc.units[0].segments[0].id.clone();
    doc.set_target(&sid, vec![Inline::text("b")]).expect("set");
    let swap = |doc: &mut Document, text: Vec<u8>| {
        let Skeleton::Text { parts, .. } = &mut doc.skeleton else { panic!("a text skeleton") };
        parts[1] = SkelPart::Raw(String::from_utf8(text).expect("utf-8"));
    };
    // a stored text with 200 000 deep units against a Document of 3
    let mut deep = doc.clone();
    swap(&mut deep, deep_adf(120, 200_000));
    let t = Instant::now();
    let e = Adf.merge(&deep).expect_err("more units than the document");
    assert!(matches!(&e, Error::Write(m) if m.contains("3 units")), "{e:?}");
    assert!(t.elapsed() < Duration::from_secs(10), "took {:?}", t.elapsed());
    // a stored text with far more values than any file of the default limits
    let mut wide = doc.clone();
    swap(&mut wide, format!("{{\"type\":\"doc\",\"content\":[{}]}}", vec!["[]"; 5_100_000].join(",")).into_bytes());
    let e = Adf.merge(&wide).expect_err("over the budget");
    assert!(is_limit(&e, LimitKind::Units), "{e:?}");
    // the document's own text still merges
    assert!(String::from_utf8(Adf.merge(&doc).expect("merge")).expect("utf-8").contains("\"text\":\"b\""));
}
