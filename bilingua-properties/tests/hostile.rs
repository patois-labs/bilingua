//! Hostile input: typed errors and no panics, over random bytes, random structured text and mutated fixtures.

mod common;

use bilingua_model::*;
use bilingua_properties::{Limits, Properties};
use common::*;
use proptest::prelude::*;

const SEEDS: [&[u8]; 4] = [
    include_bytes!("data/locos_suite_ascii.properties"),
    include_bytes!("data/languages_utf8.properties"),
    include_bytes!("data/languages_latin1.properties"),
    include_bytes!("data/edge.properties"),
];

/// Extract, and when it works: untouched merge is the same bytes; a pseudo-translated merge reads back with the
/// same unit count.
fn check(data: &[u8]) {
    let Ok(d) = Properties.extract(data, &opts()) else { return };
    assert_eq!(Properties.merge(&d).expect("merge"), data, "untouched merge");
    let mut e = d.clone();
    for u in &d.units {
        let id = u.segments[0].id.clone();
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        e.set_target(&id, t).expect("set_target");
    }
    let out = Properties.merge(&e).expect("pseudo merge");
    let back = Properties.extract(&out, &opts()).expect("re-extract");
    assert_eq!(back.units.len(), d.units.len(), "unit count after translation");
    // `#1` is a tag only in the Mozilla dialect, which is a property of the whole file's text (escaping a character
    // can change whether the file qualifies), so it is not counted here.
    let tags = |d: &Document| {
        d.units.iter().map(|u| u.source().iter().filter(|i| matches!(i, Inline::Tag { display, .. } if !display.starts_with('#'))).count()).sum::<usize>()
    };
    assert_eq!(tags(&back), tags(&d), "tag count after translation");
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("\\".to_string()),
        Just("\\\\".to_string()),
        Just("\\u".to_string()),
        Just("\\u00e9".to_string()),
        Just("\\uD83D\\uDE00".to_string()),
        Just("\\uD83D".to_string()),
        Just("=".to_string()),
        Just(":".to_string()),
        Just(" ".to_string()),
        Just("\t".to_string()),
        Just("\n".to_string()),
        Just("\r\n".to_string()),
        Just("\r".to_string()),
        Just("#".to_string()),
        Just("!".to_string()),
        Just("'".to_string()),
        Just("''".to_string()),
        Just("{0}".to_string()),
        Just("{".to_string()),
        Just("{1,number}".to_string()),
        Just("%s".to_string()),
        Just("%1$S".to_string()),
        Just("%".to_string()),
        Just("#1".to_string()),
        Just("key".to_string()),
        Just("Hello".to_string()),
        Just("caf\u{e9}".to_string()),
        Just("\u{65e5}\u{672c}".to_string()),
        Just("\u{645}\u{631}".to_string()),
        Just("\u{feff}".to_string()),
        Just("\u{1f600}".to_string()),
        Just("
# LOCALIZATION NOTE
".to_string()),
    ];
    prop::collection::vec(piece, 0..60).prop_map(|v| v.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..600)) {
        let _ = Properties.extract(&data, &opts());
        let _ = Properties.sniff(&data);
        let _ = Properties.records(&data);
        check(&data);
    }

    #[test]
    fn structured_soup_round_trips(data in soup()) {
        check(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(seed in 0usize..4, flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8), cut in prop::option::of(any::<prop::sample::Index>())) {
        let mut data = SEEDS[seed].to_vec();
        for (i, b) in flips {
            let at = i.index(data.len());
            data[at] = b;
        }
        if let Some(c) = cut {
            data.truncate(c.index(data.len() + 1));
        }
        check(&data);
    }

    #[test]
    fn every_prefix_of_a_fixture_is_handled(seed in 0usize..4, cut in any::<prop::sample::Index>()) {
        let data = &SEEDS[seed][..cut.index(SEEDS[seed].len() + 1)];
        check(data);
    }
}

#[test]
fn truncated_escape_is_a_typed_error() {
    for s in [&b"k = \\u"[..], b"k = \\u1", b"k = \\u12G4", b"k\\u00 = v", b"k = ab\\u12"] {
        assert!(matches!(Properties.extract(s, &opts()), Err(Error::Parse { .. })), "{}", String::from_utf8_lossy(s));
    }
}

#[test]
fn limits_give_typed_errors() {
    let small = { let mut built = Limits::default(); built.max_input_bytes = 100; built };
    let err = Properties.extract_with(&[b'a'; 200], &opts(), &small).unwrap_err();
    assert!(matches!(&err, Error::Limit(e) if e.msg.contains("limit") && e.kind == LimitKind::InputBytes), "{err:?}");
    let l = { let mut built = Limits::default(); built.max_value_chars = 1000; built };
    let long = format!("k = {}\n", "word ".repeat(400));
    assert!(matches!(Properties.extract_with(long.as_bytes(), &opts(), &l), Err(Error::Limit(_))));
    // a flood of blank lines is refused before any per-line work
    let l = { let mut built = Limits::default(); built.max_units = 5; built };
    assert!(matches!(Properties.extract_with("\n".repeat(500).as_bytes(), &opts(), &l), Err(Error::Limit(_))));
}

#[test]
fn big_and_odd_inputs_are_handled() {
    // a very long line, a long run of backslashes, many continuation lines, deep braces
    let long = format!("k = {}\n", "a".repeat(200_000));
    assert!(Properties.extract(long.as_bytes(), &opts()).is_ok());
    let bs = format!("k = {}\n", "\\".repeat(100_001));
    check(bs.as_bytes());
    let cont: String = std::iter::once("k = start \\\n".to_string()).chain((0..20_000).map(|_| "  more \\\n".to_string())).collect();
    check(cont.as_bytes());
    let braces = format!("k = {}{}\n", "{0,".repeat(5000), "}".repeat(5000));
    check(braces.as_bytes());
    check(b"");
    check(b"\xEF\xBB\xBF");
    check(b"\n\n\n");
    check(b"=\n:\n \\\n");
}
