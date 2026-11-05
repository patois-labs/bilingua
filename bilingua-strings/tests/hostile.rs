//! Malformed, truncated, deep and huge input: typed errors, never a panic. Whatever parses must merge back
//! to its own bytes.
mod common;
use bilingua_model::*;
use bilingua_strings::{records, Limits, Strings};
use common::*;
use proptest::prelude::*;

fn fixtures() -> Vec<&'static [u8]> {
    vec![
        include_bytes!("data/locos_src_utf8.strings"),
        include_bytes!("data/locos_src_utf16le_bom.strings"),
        include_bytes!("data/locos_src_utf16le_nobom.strings"),
        include_bytes!("data/locos_src_utf16be_bom.strings"),
        include_bytes!("data/locos_braces.strings"),
        include_bytes!("../../oracle/corpus/strings/edge.strings"),
        include_bytes!("../../oracle/corpus/strings/app_ja.strings"),
        include_bytes!("../../oracle/corpus/strings/app_de.strings"),
        include_bytes!("../../oracle/corpus/strings/edge_braces_crlf.strings"),
    ]
}

/// Extract must not panic; an Ok document merges to the same bytes; sniff, records and pseudo-translation
/// must not panic either.
fn check(b: &[u8]) {
    let _ = Strings.sniff(b);
    let _ = records(b);
    if let Ok(d) = Strings.extract(b, &opts()) {
        assert_eq!(merged(&d), b, "parsed file must merge back unchanged");
        let mut p = d.clone();
        for u in d.units.iter().filter(|u| u.translate) {
            let mut t = vec![Inline::text("\"\\\u{7f}\n")];
            t.extend(u.source());
            let _ = p.set_target(&u.segments[0].id, t);
        }
        let out = match Strings.merge(&p) {
            Ok(o) => o,
            // A NUL character in a text that is not BOM-marked UTF-16 is the only reason a merge may refuse.
            Err(Error::Write(m)) => {
                assert!(m.contains("NUL"), "{m}");
                return;
            }
            Err(e) => panic!("{e}"),
        };
        let back = Strings.extract(&out, &opts()).expect("a merged file must parse again");
        assert_eq!(back.units.len(), d.units.len());
    }
}

#[test]
fn every_truncation_of_every_fixture() {
    for f in fixtures() {
        for n in 0..=f.len() {
            check(&f[..n]);
        }
    }
}

#[test]
fn deep_and_huge() {
    // Deeply nested braces (the format has no real nesting, they are skipped) and very long runs.
    let mut deep = "{".repeat(200_000);
    deep.push_str("\"a\" = \"b\";");
    deep.push_str(&"}".repeat(200_000));
    let d = Strings.extract(deep.as_bytes(), &opts()).unwrap();
    assert_eq!(d.units.len(), 1);
    assert_eq!(merged(&d), deep.as_bytes());
    // A comment, a string, a key and a value that are enormous but allowed under the byte cap.
    let big = format!("/* {} */\n\"k\" = \"{}\";\n", "c".repeat(3_000_000), "v".repeat(90_000));
    assert!(Strings.extract(big.as_bytes(), &opts()).is_ok());
    // A value over the character limit is refused.
    let over = format!("\"k\" = \"{}\";\n", "v".repeat(100_001));
    assert!(matches!(Strings.extract(over.as_bytes(), &opts()), Err(Error::Unsupported(_))));
    // Over the byte cap.
    let small = Limits { max_bytes: 100, ..Limits::default() };
    assert!(matches!(Strings.extract_with(&[b' '; 101], &opts(), &small), Err(Error::Unsupported(_))));
    // A newline bomb is refused before any work.
    let nl = vec![b'\n'; 20 * 50_000 + 1];
    assert!(matches!(Strings.extract(&nl, &opts()), Err(Error::Unsupported(_))));
    // Quote bomb.
    let q = vec![b'"'; 8 * 50_000 + 1];
    assert!(matches!(Strings.extract(&q, &opts()), Err(Error::Unsupported(_))));
    // Many entries: refused at the unit limit.
    let many: String = (0..50_001).map(|i| format!("k{i}=v{i};\n")).collect();
    assert!(matches!(Strings.extract(many.as_bytes(), &opts()), Err(Error::Unsupported(_))));
}

#[test]
fn bad_encodings_are_typed_errors() {
    for b in [
        &b"\xff\xfe\x00"[..],                    // odd UTF-16
        &b"\xff\xfe\x00\xd8x\x00"[..],           // lone high surrogate
        &b"\xff\xfe\x00\x00"[..],                // UTF-32 LE BOM
        &b"\x00\x00\xfe\xff"[..],                // UTF-32 BE BOM
        &b"\xef\xbb\xbf\xff\xfe"[..],            // invalid UTF-8 after the BOM
        &b"a\x00b"[..],                          // NUL without a UTF-16 pattern
        &b"\xe9=\xe9;"[..],                      // Latin-1
        &b"bplist00"[..],
    ] {
        assert!(matches!(Strings.extract(b, &opts()), Err(Error::Unsupported(_))), "{b:?}");
    }
    // Malformed text.
    for b in [&b"="[..], b";", b"\"a\"", b"\"a\" =", b"\"a\" = ;", b"a b = c;", b"\"a\" = \"b\" \"c\";", b"/*", b"\"", b"@", b"\"a\" = \"b\"; }}} = ;"] {
        assert!(matches!(Strings.extract(b, &opts()), Err(Error::Parse { .. })), "{:?}", String::from_utf8_lossy(b));
    }
}

#[test]
fn merge_rejects_a_broken_document() {
    let mut d = ex(b"\"a\" = \"b\";\n");
    if let Skeleton::Text { parts, .. } = &mut d.skeleton {
        parts.push(SkelPart::Unit(99));
    }
    assert!(matches!(Strings.merge(&d), Err(Error::Write(_))));
    let mut d = ex(b"\"a\" = \"b\";\n");
    if let Skeleton::Text { encoding, .. } = &mut d.skeleton {
        *encoding = "latin-1".into();
    }
    assert!(matches!(Strings.merge(&d), Err(Error::Write(_))));
    let mut d = ex(b"\"a\" = \"b\";\n");
    d.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(matches!(Strings.merge(&d), Err(Error::Write(_))));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn random_bytes_never_panic(b in proptest::collection::vec(any::<u8>(), 0..400)) {
        check(&b);
    }

    #[test]
    fn random_text_never_panics(s in "[ -~\u{e9}\u{65e5}\n\r\t]{0,300}") {
        check(s.as_bytes());
        let mut u16 = vec![0xFF, 0xFE];
        for u in s.encode_utf16() { u16.extend_from_slice(&u.to_le_bytes()); }
        check(&u16);
    }

    #[test]
    fn structured_noise_never_panics(parts in proptest::collection::vec(prop_oneof![
        Just("\"".to_string()), Just("\\".to_string()), Just("/*".to_string()), Just("*/".to_string()), Just("//".to_string()),
        Just("=".to_string()), Just(";".to_string()), Just("{".to_string()), Just("}".to_string()), Just("%@".to_string()),
        Just("\\U".to_string()), Just("\\U00".to_string()), Just("\\777".to_string()), Just("\n".to_string()), Just("key".to_string()),
        Just("\u{e9}".to_string()), Just("\u{1F600}".to_string()), Just("%".to_string()), Just("%1$".to_string()), Just(" ".to_string()),
    ], 0..60)) {
        check(parts.concat().as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..9, edits in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0u8..3), 1..6)) {
        let mut b = fixtures()[idx].to_vec();
        for (pos, val, how) in edits {
            if b.is_empty() { break; }
            let p = pos.index(b.len());
            match how {
                0 => b[p] = val,
                1 => { b.remove(p); }
                _ => b.insert(p, val),
            }
        }
        check(&b);
    }
}

#[test]
fn nul_in_a_translation_is_refused_unless_utf16_bom() {
    let mut d = ex(b"\"a\" = \"b\";\n");
    set_nth(&mut d, 0, "x\0y");
    assert!(matches!(Strings.merge(&d), Err(Error::Write(_))));
    let mut b = vec![0xFF, 0xFE];
    for u in "\"a\" = \"b\";\n".encode_utf16() {
        b.extend_from_slice(&u.to_le_bytes());
    }
    let mut d = ex(&b);
    set_nth(&mut d, 0, "x\0y");
    let out = Strings.merge(&d).unwrap();
    assert_eq!(texts(&ex(&out)), ["x\0y"]);
}
