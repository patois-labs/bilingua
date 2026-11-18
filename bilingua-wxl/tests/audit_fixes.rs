//! Regression tests for the 0.9.1 security audit: the search for the `]` of a formatted reference is bounded, and
//! a value far over the unit length limit is refused before it is split.
mod common;
use bilingua_model::*;
use bilingua_wxl::Wxl;
use common::*;
use std::time::{Duration, Instant};

fn file(values: &[String]) -> Vec<u8> {
    let mut s = String::from("<WixLocalization Culture=\"en-us\">");
    for (i, v) in values.iter().enumerate() {
        s.push_str(&format!("<String Id=\"s{i}\" Value=\"{v}\"/>"));
    }
    s.push_str("</WixLocalization>");
    s.into_bytes()
}

fn is_unit_chars(r: &Result<Document, Error>) -> bool {
    matches!(r, Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars)
}

/// Values of many `[` and no `]`, each under the unit length limit: every `[` used to search the rest of the value
/// (2.8 s for ten such values in a debug build). The file is accepted as before, the brackets are plain text.
#[test]
fn many_open_brackets_stay_fast() {
    let values: Vec<String> = (0..40).map(|_| format!("a{}", "[".repeat(99_000))).collect();
    let data = file(&values);
    let t = Instant::now();
    let doc = try_ex(&data).expect("extract");
    let took = t.elapsed();
    assert_eq!(doc.units.len(), 40);
    assert_eq!(texts(&doc)[0], values[0]);
    assert!(doc.units.iter().all(|u| u.source().iter().all(|i| matches!(i, Inline::Text(_)))));
    assert_eq!(merged(&doc), data);
    assert!(took < Duration::from_secs(5), "{took:?}");
}

/// One value over the limit is refused with the unit length error, quickly (4 s for 400 000 `[` before).
#[test]
fn an_over_long_value_is_refused_quickly() {
    let data = file(&[format!("a{}", "[".repeat(400_000))]);
    let t = Instant::now();
    let r = try_ex(&data);
    let took = t.elapsed();
    assert!(is_unit_chars(&r), "{:?}", r.err().map(|e| e.to_string()));
    assert!(took < Duration::from_secs(2), "{took:?}");
}

/// The early refusal only takes values that are over the limit however they split: a reference counts as one
/// character, so a value of few references is accepted although it has more bytes than the limit has characters.
#[test]
fn the_early_length_check_keeps_the_unit_count_rule() {
    let tight = |v: &str| Wxl.extract(&file(&[v.to_string()]), &opts(false).with_limits(Limits::new().with_max_unit_chars(10)));
    let reference = format!("[{}]", "A".repeat(100));
    let doc = tight(&reference.repeat(5)).expect("five references are five characters");
    assert_eq!(doc.units[0].source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count(), 5);
    for long in ["x".repeat(2_000), format!("a{}", "[".repeat(2_000)), reference.repeat(11)] {
        let r = tight(&long);
        assert!(is_unit_chars(&r), "{:?}", r.err().map(|e| e.to_string()));
    }
}

/// References are found as before, up to the longest one allowed.
#[test]
fn references_are_recognised_as_before() {
    let longest = format!("[{}]", "P".repeat(128));
    let too_long = format!("[{}]", "P".repeat(129));
    let doc = ex(&file(&[format!("Install [ProductName] to [#file.txt] [~] {longest} {too_long} [not a ref] [a[b]")]));
    let mut got: Vec<String> = doc.units[0]
        .codes()
        .values()
        .filter(|c| c.label() == Some("placeholder"))
        .filter_map(|c| c.standalone_markup().map(str::to_string))
        .collect();
    let mut want = vec!["[ProductName]".to_string(), "[#file.txt]".into(), "[~]".into(), longest, "[b]".into()];
    got.sort();
    want.sort();
    assert_eq!(got, want);
}
