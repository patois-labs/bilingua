//! Hostile input: random bytes, shaped JSON noise, byte-mutated and truncated fixtures never panic. Whenever a file is
//! accepted the untouched merge gives back its exact bytes, a pseudo-translated merge succeeds and reads back with the
//! same unit count. Deep and huge files give typed errors.
mod common;
use bilingua_model::*;
use bilingua_richtext::*;
use common::*;
use proptest::prelude::*;
use serde_json::{json, Value};

fn filters() -> Vec<Box<dyn Filter>> {
    vec![Box::new(Contentful), Box::new(Sanity), Box::new(Adf), Box::new(Notion)]
}

fn check(f: &dyn Filter, data: &[u8]) {
    let _ = f.sniff(data);
    let o = opts();
    for leaf in [false, true] {
        let o = if leaf { o.clone().with_format_option("adf.units", "leaf") } else { o.clone() };
        let Ok(mut doc) = f.extract(data, &o) else { continue };
        assert_eq!(f.merge(&doc).unwrap(), data, "untouched merge must be exact");
        let n = doc.units.len();
        for i in 0..n {
            let mut t = vec![Inline::text("~")];
            t.extend(doc.units[i].source());
            let sid = doc.units[i].segments[0].id.clone();
            doc.set_target(&sid, t).unwrap();
        }
        let out = match f.merge(&doc) {
            Ok(o) => o,
            Err(e) => panic!("pseudo merge failed: {e}\n{}", String::from_utf8_lossy(data)),
        };
        let back = f.extract(&out, &o).unwrap_or_else(|e| panic!("merged output unreadable: {e}\n{}", String::from_utf8_lossy(&out)));
        assert_eq!(back.units.len(), n, "{}", String::from_utf8_lossy(data));
    }
}

fn all(data: &[u8]) {
    for f in filters() {
        check(&*f, data);
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    vec![CONTENTFUL.to_vec(), PORTABLE.to_vec(), JIRA.to_vec(), NOTION.to_vec()]
}

const KEYS: [&str; 22] = [
    "nodeType", "content", "value", "marks", "data", "type", "text", "_type", "_key", "children", "markDefs", "rich_text", "caption", "cells", "results", "object", "id", "attrs", "paragraph", "code", "link", "annotations",
];
const STRS: [&str; 26] = [
    "paragraph", "text", "document", "hyperlink", "embedded-entry-inline", "heading-1", "list-item", "block", "span", "code", "doc", "mention", "table_row", "toggle", "codeBlock", "Hello world", "a\u{27E6}b", "x", "", " ", "em", "strong", "k1", "\u{65e5}\u{672c}", "bold", "unordered-list",
];

fn leaf() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (0u32..5).prop_map(|n| json!(n)),
        proptest::sample::select(STRS.to_vec()).prop_map(|s| json!(s)),
    ]
}

fn tree() -> impl Strategy<Value = Value> {
    leaf().prop_recursive(6, 80, 6, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..5).prop_map(Value::Array),
            proptest::collection::vec((proptest::sample::select(KEYS.to_vec()), inner), 0..6).prop_map(|kv| Value::Object(kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect())),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1500, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..400)) {
        all(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(which in 0usize..4, edits in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..8)) {
        let mut d = fixtures().swap_remove(which);
        for (pos, b) in edits {
            let i = pos % d.len();
            d[i] = b;
        }
        all(&d);
    }

    #[test]
    fn shaped_noise_in_every_wrapper_never_panics(body in tree(), kids in proptest::collection::vec(tree(), 0..4)) {
        let doc = json!({"nodeType": "document", "content": [{"nodeType": "paragraph", "content": kids.clone()}, body.clone()]});
        let pt = json!([{"_type": "block", "children": kids.clone()}, body.clone()]);
        let adf = json!({"type": "doc", "content": [{"type": "paragraph", "content": kids.clone()}, body.clone()]});
        let nb = json!([{"object": "block", "type": "paragraph", "id": "a", "paragraph": {"rich_text": kids, "children": [body.clone()]}}, body]);
        for v in [doc, pt, adf, nb] {
            all(v.to_string().as_bytes());
            all(serde_json::to_string_pretty(&v).unwrap().as_bytes());
        }
    }
}

#[test]
fn every_truncation_of_every_fixture_is_handled() {
    for fx in fixtures() {
        for n in 0..fx.len() {
            all(&fx[..n]);
        }
    }
}

#[test]
fn deep_and_huge_input_gives_typed_errors() {
    for f in filters() {
        for deep in ["[".repeat(100_000), "{\"a\":".repeat(100_000), format!("{}1{}", "[".repeat(300), "]".repeat(300))] {
            assert!(matches!(f.extract(deep.as_bytes(), &opts()), Err(Error::Parse { .. })), "deep");
        }
        let big = format!("[{}]", vec!["\"x\""; 200].join(","));
        let o = ExtractOptions::new().with_limits(Limits::new().with_max_input_bytes(100));
        assert!(matches!(f.extract(big.as_bytes(), &o), Err(Error::Limit(_))));
    }
    // a document with too many units for the cap
    let paras: Vec<Value> = (0..50).map(|i| json!({"nodeType": "paragraph", "data": {}, "content": [{"nodeType": "text", "value": format!("Para {i}"), "marks": [], "data": {}}]})).collect();
    let doc = json!({"nodeType": "document", "data": {}, "content": paras}).to_string();
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(10));
    assert!(matches!(Contentful.extract(doc.as_bytes(), &o), Err(Error::Limit(_))));
    // very long strings and many keys are fine
    let long = json!([{"_type": "block", "children": [{"_type": "span", "text": "w".repeat(90_000), "marks": []}]}]).to_string();
    assert_eq!(Sanity.extract(long.as_bytes(), &opts()).unwrap().units.len(), 1);
    let wide: String = format!("{{{}}}", (0..20_000).map(|i| format!("\"k{i}\":{i}")).collect::<Vec<_>>().join(","));
    assert!(Adf.extract(wide.as_bytes(), &opts()).is_err());
}
