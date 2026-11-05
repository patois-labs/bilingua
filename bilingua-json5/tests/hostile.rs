//! Hostile input: random bytes, byte-mutated and truncated fixtures never panic, and whenever a file is accepted
//! the untouched merge gives back its exact bytes. Also deep, huge and malformed files give typed errors.

use bilingua_json5::{Json5Filter, Limits};
use bilingua_model::*;
use proptest::prelude::*;

const APP: &[u8] = include_bytes!("data/app.json5");
const SETTINGS: &[u8] = include_bytes!("data/settings.jsonc");

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("es").with_granularity(Granularity::Paragraph).with_include_locked(true)
}

/// Extract; when accepted, the untouched merge is the same bytes and a pseudo merge re-opens with the same count.
fn check(data: &[u8]) {
    let f = Json5Filter::default();
    let Ok(mut doc) = f.extract(data, &opts()) else { return };
    assert_eq!(f.merge(&doc).expect("merge of an accepted file"), data);
    let ids: Vec<(String, Vec<Inline>, bool)> = doc.units.iter().map(|u| (u.segments[0].id.clone(), u.segments[0].source.clone(), u.translate)).collect();
    for (id, src, tr) in ids {
        if tr {
            let mut t = vec![Inline::text("\"'\\n")];
            t.extend(src);
            let _ = doc.set_target(&id, t);
        }
    }
    let out = f.merge(&doc).expect("merge of a translated file");
    let back = f.extract(&out, &opts()).expect("a merged file re-opens");
    assert_eq!(back.units.len(), doc.units.len());
}

const TOKENS: [&str; 34] = [
    "{", "}", "[", "]", ":", ",", "'", "\"", "//", "/*", "*/", "\n", "\r\n", "\u{5c}", "\u{5c}u", "\u{5c}x", "\u{5c}ud83d",
    "a", "b", "key", "1", "0x", "+", ".", " ", "true", "null", "$", "Infinity", "NaN", "e", "-", "\u{2028}", "\u{feff}",
];

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..400)) {
        check(&data);
    }

    #[test]
    fn json5_shaped_noise_never_panics(parts in proptest::collection::vec(proptest::sample::select(TOKENS.to_vec()), 0..80)) {
        check(parts.concat().as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(which in 0usize..2, edits in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..8)) {
        let mut d = if which == 0 { APP.to_vec() } else { SETTINGS.to_vec() };
        for (pos, b) in edits {
            let i = pos % d.len();
            d[i] = b;
        }
        check(&d);
    }

    #[test]
    fn truncated_fixtures_never_panic(which in 0usize..2, cut in any::<usize>()) {
        let d = if which == 0 { APP } else { SETTINGS };
        check(&d[..cut % (d.len() + 1)]);
    }

    #[test]
    fn deleted_and_inserted_bytes_never_panic(which in 0usize..2, pos in any::<usize>(), ins in proptest::collection::vec(any::<u8>(), 0..6), del in 0usize..6) {
        let src = if which == 0 { APP } else { SETTINGS };
        let p = pos % src.len();
        let mut d = src[..p].to_vec();
        d.extend(&ins);
        d.extend(&src[(p + del).min(src.len())..]);
        check(&d);
    }
}

#[test]
fn the_fixtures_themselves_pass_the_check() {
    check(APP);
    check(SETTINGS);
}

#[test]
fn deep_nesting_gives_a_typed_error_not_a_stack_overflow() {
    let f = Json5Filter::default();
    for depth in [257usize, 10_000, 1_000_000] {
        let mut d = vec![b'{'; 0];
        for _ in 0..depth {
            d.extend(b"a:[");
        }
        let src = [b"{".to_vec(), d, b"}".to_vec()].concat();
        let e = f.extract(&src, &opts()).unwrap_err();
        assert!(matches!(e, Error::Parse { .. }), "{e}");
    }
}

#[test]
fn truncated_everywhere_is_an_error_or_a_clean_file() {
    let f = Json5Filter::default();
    for cut in 0..APP.len() {
        let _ = f.extract(&APP[..cut], &opts());
    }
}

#[test]
fn a_huge_file_over_the_byte_limit_is_refused() {
    let f = Json5Filter::with_limits(Limits { max_bytes: 1000, ..Limits::default() });
    let big = format!("[{}]", "'word',".repeat(1000));
    let e = f.extract(big.as_bytes(), &opts()).unwrap_err();
    assert!(matches!(e, Error::Parse { .. }));
}

#[test]
fn errors_name_the_line_and_column() {
    let e = Json5Filter::default().extract(b"{\n  a: 'x',\n  b: oops,\n}", &opts()).unwrap_err().to_string();
    assert!(e.contains("line 3 column 6"), "{e}");
}

#[test]
fn a_stale_or_foreign_skeleton_is_refused_by_merge() {
    let f = Json5Filter::default();
    let mut d = f.extract(b"{a: 'Hi'}", &opts()).unwrap();
    if let Skeleton::Text { parser: Some(p), .. } = &mut d.skeleton {
        p.name = "bilingua-other".into();
    }
    assert!(matches!(f.merge(&d), Err(Error::ParserVersion { .. })));
    let mut d = f.extract(b"{a: 'Hi'}", &opts()).unwrap();
    d.units.clear();
    assert!(f.merge(&d).is_err());
}
