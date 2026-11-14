//! Hostile input: nothing panics, errors are typed, and whatever opens also merges.

use bilingua_arb::Arb;
use bilingua_model::*;
use proptest::prelude::*;

const FIXTURES: [&str; 4] = [
    include_str!("data/locos_main.arb"),
    include_str!("data/locos_suffix.arb"),
    include_str!("data/languages.arb"),
    include_str!("data/locos_insert.arb"),
];

/// Opens `bytes`; if it opens, an untouched merge must give the same bytes, and a pseudo-translated merge into
/// a target language must give bytes that open again.
fn exercise(bytes: &[u8], target: Option<&str>) {
    let Ok(mut doc) = Arb.extract(bytes, &ExtractOptions::default()) else { return };
    assert_eq!(Arb.merge(&doc).unwrap(), bytes, "untouched merge changed the file");
    doc.target_lang = target.map(str::to_string);
    for u in doc.units.iter_mut() {
        for s in u.segments.iter_mut() {
            let mut t = vec![Inline::text("~")];
            t.extend(s.source.clone());
            s.target = merge_text(t);
        }
    }
    let out = Arb.merge(&doc).expect("a document that opened must merge");
    let again = Arb.extract(&out, &ExtractOptions::default()).expect("merged output must open again");
    assert!(again.units.len() >= doc.units.len() || target.is_some());
}

fn mutate(src: &[u8], edits: &[(usize, u8, u8)]) -> Vec<u8> {
    let mut v = src.to_vec();
    for &(at, kind, byte) in edits {
        if v.is_empty() {
            break;
        }
        let i = at % v.len();
        match kind % 4 {
            0 => v[i] = byte,
            1 => {
                v.remove(i);
            }
            2 => v.insert(i, byte),
            _ => v.truncate(i),
        }
    }
    v
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        exercise(&bytes, Some("ru"));
    }

    #[test]
    fn random_json_like_text_never_panics(s in r#"[{}\[\]":,a-z0-9 #'|\\@=.\n-]{0,300}"#) {
        exercise(s.as_bytes(), Some("ar-EG"));
        exercise(s.as_bytes(), None);
    }

    #[test]
    fn mutated_fixtures_never_panic(
        which in 0..FIXTURES.len(),
        edits in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 1..8),
        target in prop_oneof![Just(None), Just(Some("ja")), Just(Some("pl-PL")), Just(Some("zz"))],
    ) {
        let bytes = mutate(FIXTURES[which].as_bytes(), &edits);
        exercise(&bytes, target);
    }

    #[test]
    fn random_icu_in_a_message_never_panics(msg in r#"[{}#',a-z =0-9|]{0,120}"#) {
        let doc = format!("{{\"m\": {}}}", serde_json::to_string(&msg).unwrap());
        exercise(doc.as_bytes(), Some("ru"));
        exercise(doc.as_bytes(), None);
    }
}

#[test]
fn every_truncation_of_every_fixture_is_handled() {
    for f in FIXTURES {
        for cut in 0..f.len() {
            if f.is_char_boundary(cut) {
                exercise(&f.as_bytes()[..cut], Some("pl"));
            }
        }
    }
}

#[test]
fn deep_and_huge_inputs_give_typed_errors() {
    // the crate's own limits (1,000,000 units): LocOS's MAX_DOCUMENT_SEGMENTS budget would refuse the floods below
    let ex = |b: &[u8]| Arb.extract_with(b, &ExtractOptions::default(), &bilingua_arb::Limits::default());
    // 200k open brackets and braces: refused at the nesting limit, no stack overflow
    for open in ["[", "{\"a\":"] {
        let deep = format!("{{\"k\": {}", open.repeat(200_000));
        assert!(matches!(ex(deep.as_bytes()), Err(Error::Limit(e)) if e.kind == LimitKind::Depth));
    }
    // a message nested 100k deep in ICU is one opaque unit
    let icu = format!("{}x{}", "{a, plural, other{".repeat(100_000), "}".repeat(100_000));
    let doc = format!("{{\"m\": \"{icu}\"}}");
    assert_eq!(ex(doc.as_bytes()).unwrap().units.len(), 1);
    // a 6 MB message and 100k tiny ones are fine and quick
    let big = format!("{{\"m\": \"{}\"}}", "word ".repeat(1_200_000));
    let d = ex(big.as_bytes()).unwrap();
    assert_eq!(Arb.merge(&d).unwrap(), big.as_bytes());
    let many = format!("{{{}}}", (0..100_000).map(|i| format!("\"k{i}\":\"v{i}\"")).collect::<Vec<_>>().join(","));
    assert_eq!(ex(many.as_bytes()).unwrap().units.len(), 100_000);
    // a million braces in one message
    let braces = format!("{{\"m\": \"{}\"}}", "{".repeat(1_000_000));
    let d = ex(braces.as_bytes()).unwrap();
    assert!(d.units.is_empty());
    // lots of quotes and a lone surrogate escape
    assert!(ex(br#"{"a": "\ud800 lone", "b": "fine text"}"#).unwrap().units.len() == 1);
}
