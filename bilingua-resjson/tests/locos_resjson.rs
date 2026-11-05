//! The Localization OS suite `tests/verify_resjson_filter.py`, ported (all 4 test functions), run against
//! the same fixture bytes (`tests/data/locos_suite.resjson`, byte for byte what the suite builds).
//!
//! Not portable: `assert_placeholder_dialect` calls the LocOS quality service (`placeholder_violation`), which is
//! a different layer than the filter. Its filter-side half is kept: `{0}` stays plain text in the unit.

mod common;
use bilingua_model::*;
use bilingua_resjson::Resjson;
use common::*;

fn replace(hay: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let pos = hay.windows(from.len()).position(|w| w == from).expect("pattern present");
    let mut out = hay[..pos].to_vec();
    out.extend_from_slice(to);
    out.extend_from_slice(&hay[pos + from.len()..]);
    out
}

#[test]
fn units_and_identity() {
    let d = ex(LOCOS_SUITE);
    assert_eq!(texts(&d), vec!["Hello", "Goodbye", "Say \"cheese\" \u{e9}", "{0} new messages", "Open file"]);
    // An untouched merge, and a merge where every unit is echoed back, are byte-identical.
    assert_eq!(merged(&d), LOCOS_SUITE);
    let mut echo = d.clone();
    translate_all(&mut echo, |s| s.to_string());
    assert_eq!(merged(&echo), LOCOS_SUITE);
}

#[test]
fn round_trip() {
    let mut d = ex(LOCOS_SUITE);
    d.set_target("u1", vec![Inline::text("Hallo")]).unwrap();
    d.set_target("u3", vec![Inline::text("Sag \"K\u{e4}se\"\n")]).unwrap();
    d.set_target("u5", vec![Inline::text("Datei \u{f6}ffnen")]).unwrap();
    let out = merged(&d);
    let expected = replace(LOCOS_SUITE, b"\"Hello\"", b"\"Hallo\"");
    let expected = replace(&expected, b"\"Say \\\"cheese\\\" \\u00e9\"", "\"Sag \\\"K\u{e4}se\\\"\\n\"".as_bytes());
    let expected = replace(&expected, b"\"Open file\"", "\"Datei \u{f6}ffnen\"".as_bytes());
    assert_eq!(String::from_utf8_lossy(&out), String::from_utf8_lossy(&expected));
    assert_eq!(out, expected);
    // The result reads back with the same number of units and the translations (white space trimmed).
    let back = ex(&out);
    let t = texts(&back);
    assert_eq!(t.len(), 5);
    assert_eq!(t[0], "Hallo");
    assert_eq!(t[2], "Sag \"K\u{e4}se\"");
    assert_eq!(t[4], "Datei \u{f6}ffnen");
    // Untranslated strings keep their original spelling.
    assert_eq!(t[1], "Goodbye");
}

#[test]
fn hostile_inputs() {
    let refuse = |data: &[u8], needle: &str| {
        let msg = parse_msg(try_ex(data)).to_lowercase();
        assert!(msg.contains(&needle.to_lowercase()), "{msg:?} should mention {needle:?} for {:?}", String::from_utf8_lossy(&data[..data.len().min(60)]));
    };
    refuse(b"{\"a\": \"never closed}", "never closed");
    refuse(b"{\"a\": \"b\" /* open", "never closed");
    refuse(b"{\"a\": \"b\"} {\"c\": \"d\"}", "after the top-level");
    refuse(b"{\"a\": nope}", "expected a value");
    refuse(b"{\"a\" \"b\"}", "expected ':'");
    refuse(b"[\"a\"]", "one JSON object");
    refuse(b"{\"a\": \"bad \\x escape\"}", "escape");
    let mut deep = b"{\"a\":".to_vec();
    deep.extend(std::iter::repeat_n(b'[', 200));
    deep.extend(std::iter::repeat_n(b']', 200));
    deep.push(b'}');
    let err = try_ex(&deep).unwrap_err();
    assert!(matches!(err, Error::Unsupported(_)), "{err:?}");
    assert!(err.to_string().contains("nested"));
    refuse(b"{\"a\": \"caf\xe9\"}", "UTF-8");

    // The segment cap: more units than the limit is refused (LocOS lowers MAX_DOCUMENT_SEGMENTS to 10, feeds 30).
    let mut data = Vec::new();
    for i in 0..30 {
        if i == 0 {
            data.push(b'{');
        }
        data.extend_from_slice(format!("\"k{i}\": \"Value number {i}\"").as_bytes());
        data.push(if i == 29 { b'}' } else { b',' });
    }
    let err = Resjson.extract_with(&data, &opts(false), &small_limits(10)).unwrap_err();
    assert!(matches!(err, Error::Unsupported(_)), "{err:?}");
    // Exactly at the cap is fine.
    assert_eq!(Resjson.extract_with(&data, &opts(false), &small_limits(30)).unwrap().units.len(), 30);
}

#[test]
fn records_and_placeholders() {
    // LocOS `records()`: every entry, translatable or not, with key, value, comment, context and unit index.
    let d = ex_locked(LOCOS_SUITE);
    let by = |ctx: &str, key: &str| {
        d.units
            .iter()
            .enumerate()
            .find(|(_, u)| u.context.extra.get("path").map(String::as_str) == Some(ctx) && u.context.extra.get("name").map(String::as_str) == Some(key))
            .unwrap_or_else(|| panic!("no entry {ctx}/{key}"))
    };
    // Comments are joined from `_key.comment`.
    assert_eq!(by("", "greeting").1.context.notes, vec!["A welcome greeting."]);
    assert_eq!(by("", "farewell").1.context.notes, vec!["A goodbye."]);
    // The comment and the placeholder-only value are present but not translatable (no unit index in LocOS).
    assert!(!by("", "_greeting.comment").1.translate);
    assert!(!by("", "id").1.translate);
    // A nested value has its path as context, and its position among the translatable units (index 4).
    let (_, open) = by("menu", "open");
    assert!(open.translate);
    assert_eq!(plain_text(&open.source()), "Open file");
    assert_eq!(open.context.key.as_deref(), Some("/menu/open"));
    let default = ex(LOCOS_SUITE);
    assert_eq!(default.units[4].context.key.as_deref(), Some("/menu/open"));
    // Non-string members (`true`, `40`) are not entries at all.
    assert!(!d.units.iter().any(|u| u.context.extra.get("name").map(String::as_str) == Some("flag")));
    // `_count.maxLength: 40` belongs to `count`.
    assert_eq!(by("", "count").1.context.max_len, Some(40));
    // A placeholder stays plain text inside the unit (the LocOS filter does not tokenise it).
    let count = &by("", "count").1;
    assert_eq!(count.segments[0].source, vec![Inline::text("{0} new messages")]);
}
