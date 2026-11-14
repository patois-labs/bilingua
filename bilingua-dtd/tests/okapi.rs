//! Comparison with Okapi okf_dtd 1.48.0 on Okapi's own DTD test resources (`Test01.dtd`, `Test02.dtd`, copied into
//! `tests/data`, Apache 2.0) and the corpus. `data/okapi_golden.json` records Okapi's units (made by
//! `tools/gen_okapi.py` through the okapi-oracle); the text has one U+00A4 per Okapi inline code.
//!
//! Where LocOS and Okapi disagree, LocOS wins; every difference is listed in `KNOWN` with its reason.
mod common;
use bilingua_model::*;
use common::*;

fn norm(s: &str) -> String {
    s.replace('\u{a0}', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Our unit text with one marker per tag (the oracle's norm for Bilingua).
fn marked(v: &[Inline]) -> String {
    norm(
        &v.iter()
            .map(|i| match i {
                Inline::Text(t) => t.clone(),
                Inline::Tag { .. } => "\u{a4}".into(),
            })
            .collect::<String>(),
    )
}

/// (file, entity, why): the units where we differ from Okapi, all by design.
const KNOWN: &[(&str, &str, &str)] = &[
    ("*", "test1", "a literal &amp;name; resolves to the text &name;, which LocOS protects like a reference: one tag here, plain text in Okapi"),
    ("*", "bytes.before", "Okapi extracts an empty value as a unit; LocOS and we skip a value without a letter or digit"),
    ("*", "ref.only", "a value that is only a reference is not a unit in LocOS (Okapi extracts it)"),
    ("*", "escaped.markup", "Okapi merges `&lt;now&gt;` into one code; we keep `&lt;` and `&gt;` as two references (only `&lt;tag ...>` with a literal `>` is one markup tag)"),
];

#[test]
fn matches_okapi_on_its_test_resources() {
    let text = std::fs::read_to_string(data_dir().join("okapi_golden.json")).unwrap();
    let golden: serde_json::Value = serde_json::from_str(&text).unwrap();
    let (mut compared, mut same, mut known) = (0, 0, 0);
    let mut unexpected = Vec::new();
    for (file, g) in golden.as_object().unwrap() {
        let Some(units_g) = g.get("units").and_then(|u| u.as_array()) else { continue };
        let path = if file.starts_with("Test0") { data_dir().join(file) } else { corpus_dir().join(file) };
        let data = std::fs::read(&path).unwrap();
        let doc = extract(&data).unwrap();
        let mine: std::collections::BTreeMap<String, String> =
            doc.units.iter().map(|u| (u.context.key.clone().unwrap(), marked(&u.source()))).collect();
        let theirs: Vec<(String, String)> =
            units_g.iter().map(|u| (u["name"].as_str().unwrap().to_string(), norm(u["text"].as_str().unwrap()))).collect();
        // every unit of ours exists in Okapi's list, in the same order
        let order_theirs: Vec<&String> = theirs.iter().map(|(n, _)| n).filter(|n| mine.contains_key(*n)).collect();
        let order_mine: Vec<String> = doc.units.iter().map(|u| u.context.key.clone().unwrap()).collect();
        assert_eq!(order_theirs.iter().map(|s| s.as_str()).collect::<Vec<_>>(), order_mine.iter().map(|s| s.as_str()).collect::<Vec<_>>(), "{file}: order");
        for (name, okapi_text) in &theirs {
            compared += 1;
            let ours = mine.get(name);
            let is_known = KNOWN.iter().any(|(f, n, _)| (*f == "*" || f == file) && n == name);
            match ours {
                Some(t) if t == okapi_text && !is_known => same += 1,
                _ if is_known => known += 1,
                other => unexpected.push(format!("{file} {name}: okapi {okapi_text:?} ours {other:?}")),
            }
        }
    }
    println!("okapi: {compared} units compared, {same} identical, {known} known differences");
    assert!(unexpected.is_empty(), "unexpected differences:\n{}", unexpected.join("\n"));
    assert!(same >= 90, "{same}");
}

#[test]
fn okapi_test_files_round_trip() {
    for f in ["Test01.dtd", "Test02.dtd"] {
        let data = std::fs::read(data_dir().join(f)).unwrap();
        assert_identity(&data);
    }
    // Test02 is the Qt TS DTD: declarations only, no general entity with a value
    assert!(units(&std::fs::read(data_dir().join("Test02.dtd")).unwrap()).is_empty());
    // Test01: 44 Okapi units, 43 here (the empty `bytes.before` is skipped)
    assert_eq!(units(&std::fs::read(data_dir().join("Test01.dtd")).unwrap()).len(), 43);
}
