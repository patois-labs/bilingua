//! Hostile input: malformed, truncated, deep and oversized files give typed errors and never panic; property
//! tests over random bytes, JSON-like text, byte-mutated fixtures, and generated documents.

mod common;
use bilingua_model::*;
use bilingua_resjson::{Limits, Resjson};
use common::*;
use proptest::prelude::*;

fn is_typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Unsupported(_))
}

#[test]
fn malformed_files_are_parse_errors() {
    let cases: &[&[u8]] = &[
        b"",
        b"   ",
        b"// only a comment",
        b"/* only a comment */",
        b"{",
        b"}",
        b"{\"a\"",
        b"{\"a\":",
        b"{\"a\":}",
        b"{\"a\": \"b\"",
        b"{\"a\": \"b\",",
        b"{,}",
        b"{\"a\":\"b\",,}",
        b"{\"a\":\"b\"}}",
        b"{\"a\":\"b\"} x",
        b"{1:2}",
        b"{\"a\" \"b\"}",
        b"{\"a\":01}",
        b"{\"a\":-}",
        b"{\"a\":1.}",
        b"{\"a\":1e}",
        b"{\"a\":tru}",
        b"{\"a\":nope}",
        b"{\"a\":/x}",
        b"/",
        b"[",
        b"[]",
        b"\"just a string\"",
        b"123",
        b"{\"a\":\"\\u12\"}",
        b"{\"a\":\"\\u12zz\"}",
        b"{\"a\":\"\\x\"}",
        b"{\"a\":\"line\nbreak\"}",
        b"{\"a\":\"tab\there\"}",
        b"{\"a\":\"ends with backslash\\",
        b"{\"a\":\"b\" /* never closed",
        b"{\"a\":\"b\"} /* never closed",
        b"{\"a\": [1, 2",
        b"{\"a\": [,]}",
        b"{\"a\": [1 2]}",
        b"{\"a\" : \"b\" \"c\" : \"d\"}",
    ];
    for c in cases {
        match try_ex(c) {
            Err(e) => assert!(is_typed(&e), "{e:?} for {:?}", String::from_utf8_lossy(c)),
            Ok(_) => panic!("must be refused: {:?}", String::from_utf8_lossy(c)),
        }
    }
}

#[test]
fn encoding_errors_are_typed() {
    for (data, needle) in [
        (&b"{\"a\": \"caf\xe9\"}"[..], "UTF-8"),
        (&b"\xff\xfe\x00\x00{\x00\x00\x00"[..], "UTF-32"),
        (&b"\x00\x00\xfe\xff"[..], "UTF-32"),
        (&b"\xff\xfe{\x00}"[..], "UTF-16"),
        (&b"\xff\xfe{\x00}\x00\x00"[..], "UTF-16"),
        (&b"\xfe\xff\x00{\xd8\x00\x00}"[..], "UTF-16"),
        (&b"\xef\xbb\xbf{\"a\": \"\xc3\"}"[..], "UTF-8"),
    ] {
        let e = try_ex(data).unwrap_err();
        assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
        assert!(e.to_string().contains(needle), "{e}");
    }
}

#[test]
fn deep_nesting_is_refused_without_a_stack_overflow() {
    let boundary = |n: usize| {
        let mut v = b"{\"a\":".to_vec();
        v.extend(std::iter::repeat_n(b'[', n));
        v.extend(std::iter::repeat_n(b']', n));
        v.push(b'}');
        v
    };
    // The top-level object is level 1: 63 arrays inside it is the deepest allowed.
    assert!(try_ex(&boundary(63)).is_ok());
    assert!(matches!(try_ex(&boundary(64)), Err(Error::Unsupported(_))));
    // A million brackets, and a million unclosed ones.
    let mut huge = b"{\"a\":".to_vec();
    huge.extend(std::iter::repeat_n(b'[', 1_000_000));
    assert!(try_ex(&huge).is_err());
    let mut objs = Vec::new();
    for _ in 0..200_000 {
        objs.extend_from_slice(b"{\"a\":");
    }
    assert!(try_ex(&objs).is_err());
    // Custom depth.
    let lim = { let mut built = Limits::default(); built.max_depth = 3; built };
    assert!(Resjson.extract_with(b"{\"a\":{\"b\":\"Text\"}}", &opts(false), &lim).is_ok());
    assert!(Resjson.extract_with(b"{\"a\":{\"b\":{\"c\":{\"d\":\"Text\"}}}}", &opts(false), &lim).is_err());
}

#[test]
fn oversized_files_are_refused() {
    // Too many lines for the default limits (20 per allowed unit).
    let mut lines = b"{\"a\": \"b\"".to_vec();
    lines.extend(std::iter::repeat_n(b'\n', 1_000_001));
    lines.push(b'}');
    let e = try_ex(&lines).unwrap_err();
    assert!(matches!(e, Error::Unsupported(_)) && e.to_string().contains("lines"), "{e}");
    // Too many quotes (40 per allowed unit): 2 000 001 in a few MB.
    let mut quotes = b"{\"a\": \"b\", \"c\": ".to_vec();
    quotes.extend(std::iter::repeat_n(b'"', 2_000_001));
    assert!(matches!(try_ex(&quotes), Err(Error::Unsupported(_))));
    // Too many values: commas, brackets and braces.
    let mut arr = b"{\"a\": [".to_vec();
    arr.extend(std::iter::repeat_n(b'1', 1));
    for _ in 0..1_000_001 {
        arr.extend_from_slice(b",1");
    }
    arr.extend_from_slice(b"]}");
    assert!(matches!(try_ex(&arr), Err(Error::Unsupported(_))));
    // More bytes than the byte limit.
    let lim = { let mut built = Limits::default(); built.max_bytes = 10; built };
    assert!(matches!(Resjson.extract_with(b"{\"a\": \"Hello there\"}", &opts(false), &lim), Err(Error::Unsupported(_))));
    // A value longer than the character limit.
    let lim = { let mut built = Limits::default(); built.max_unit_chars = 20; built };
    let long = format!("{{\"a\": \"{}\"}}", "x".repeat(21));
    assert!(matches!(Resjson.extract_with(long.as_bytes(), &opts(false), &lim), Err(Error::Unsupported(_))));
    assert!(Resjson.extract_with(long.replace("x".repeat(21).as_str(), &"x".repeat(20)).as_bytes(), &opts(false), &lim).is_ok());
    // Limits count translatable units; locked ones do not use them up.
    let src = br#"{"a": "{0}", "b": "{1}", "c": "Text"}"#;
    assert!(Resjson.extract_with(src, &opts(true), &small_limits(1)).is_ok());
}

#[test]
fn merge_of_a_tampered_document_is_a_typed_error() {
    let mut d = ex(LOCOS_SUITE);
    if let Skeleton::Text { parts, .. } = &mut d.skeleton {
        parts.push(SkelPart::Unit(999));
    }
    assert!(matches!(Resjson.merge(&d), Err(Error::Write(_))));
    let mut zip = ex(LOCOS_SUITE);
    zip.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(matches!(Resjson.merge(&zip), Err(Error::Write(_))));
}

/// The invariants of any file that extracts: the untouched merge returns the bytes, and a pseudo-translated
/// merge reads back with the same units.
fn check_file(bytes: &[u8]) {
    for locked in [false, true] {
        let Ok(mut d) = Resjson.extract(bytes, &opts(locked)) else { continue };
        let out = Resjson.merge(&d).expect("merge of an extracted document");
        assert_eq!(out, bytes, "untouched merge changed the file");
        pseudo(&mut d);
        let out = Resjson.merge(&d).expect("merge of a translated document");
        let back = Resjson.extract(&out, &opts(locked)).expect("the translated file must extract");
        assert_eq!(back.units.len(), d.units.len());
    }
    let _ = Resjson.sniff(bytes);
}

fn mutate(base: &[u8], edits: &[(usize, u8, u8)]) -> Vec<u8> {
    let mut v = base.to_vec();
    for &(pos, byte, kind) in edits {
        if v.is_empty() {
            break;
        }
        let at = pos % v.len();
        match kind % 4 {
            0 => v[at] = byte,
            1 => v.insert(at, byte),
            2 => {
                v.remove(at);
            }
            _ => v.truncate(at),
        }
    }
    v
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn random_bytes_never_panic(b in proptest::collection::vec(any::<u8>(), 0..1500)) {
        check_file(&b);
    }

    #[test]
    fn json_like_text_never_panics(s in "[{}\\[\\]\",:/*a-z0-9\\\\ \\n\\t_.\\-]{0,400}") {
        check_file(s.as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(
        which in 0usize..6,
        edits in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 0..10),
    ) {
        let all = fixtures();
        let (_, base) = all[which % all.len()];
        check_file(&mutate(base, &edits));
    }

    #[test]
    fn deep_random_brackets_never_panic(s in "[{}\\[\\]\":,a]{0,3000}") {
        check_file(s.as_bytes());
    }
}

fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    /// Any object of string values, written by a real JSON serialiser, extracts, and a translation merged back
    /// parses (with a real JSON parser) to exactly the new values, white space kept, other values untouched.
    #[test]
    fn generated_documents_translate_exactly(
        members in proptest::collection::btree_map("[a-z]{1,6}", any::<String>(), 0..12),
        suffix in any::<String>(),
    ) {
        let obj: serde_json::Map<String, serde_json::Value> = members.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect();
        let text = serde_json::to_string_pretty(&serde_json::Value::Object(obj)).unwrap();
        let mut d = Resjson.extract(text.as_bytes(), &opts(false)).expect("valid JSON object");
        prop_assert_eq!(Resjson.merge(&d).unwrap(), text.as_bytes().to_vec());
        let mut expected = members.clone();
        let jobs: Vec<(String, String)> = d.units.iter().map(|u| (u.segments[0].id.clone(), u.context.key.clone().unwrap())).collect();
        for (seg, ptr) in &jobs {
            let key = ptr.trim_start_matches('/').to_string();
            let old = &members[&key];
            let new = format!("T{suffix}");
            d.set_target(seg, vec![Inline::text(new.clone())]).unwrap();
            let lead_len = old.len() - old.trim_start_matches(is_py_space).len();
            let trail_start = old.trim_end_matches(is_py_space).len();
            expected.insert(key, format!("{}{}{}", &old[..lead_len], new, &old[trail_start..]));
        }
        let out = Resjson.merge(&d).unwrap();
        let parsed: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&out).expect("output is valid JSON");
        let got: std::collections::BTreeMap<String, String> = parsed.into_iter().map(|(k, v)| (k, v.as_str().unwrap().to_string())).collect();
        prop_assert_eq!(got, expected);
    }
}
