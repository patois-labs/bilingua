//! Hostile input: random bytes, mutated fixtures, deep and huge files. Nothing panics; a file that is accepted merges
//! back to its exact bytes, and a pseudo-translated merge reads back with the same unit count.
mod common;
use bilingua_model::*;
use bilingua_stf::{records, StfFilter};
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = StfFilter.sniff(data);
    let _ = records(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(StfFilter.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let ids: Vec<(String, String)> = doc.units.iter().map(|u| (u.segments[0].id.clone(), plain_text(&u.source()))).collect();
            let n = doc.units.len();
            doc.target_lang = Some("de-DE".into());
            for (id, t) in ids {
                doc.set_target(&id, vec![Inline::text(format!("~{t}~"))]).unwrap();
            }
            for opts in [MergeOptions::default(), MergeOptions::locos()] {
                let out = StfFilter.merge_with(&doc, &opts).unwrap();
                // a translation can stay a unit only if it has a letter; "~x~" always does
                assert_eq!(units(&out).len(), n, "unit count after translation");
            }
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Encoding(_) | Error::Limit(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..600)) {
        check(&data);
    }

    #[test]
    fn random_text_never_panics(s in "[ -~\\t\\r\\n\\u{a0}\\u{e9}\\u{65e5}\\u{2028}\\u{85}]{0,400}") {
        check(s.as_bytes());
    }

    #[test]
    fn stf_shaped_lines_never_panic(
        lines in proptest::collection::vec("(Language code ?: ?[a-z_]{0,5}|Type:[ A-Za-z]{0,10}|-{0,4}[A-Z ]{0,8}-{0,4}|# ?KEY\\tLABEL(\\tTRANSLATION)?(\\tOUT OF DATE)?|#[ a-z]{0,6}|[A-Za-z.]{0,6}\\t[ A-Za-z{}!0-9,\\\\]{0,12}(\\t[A-Za-z*-]{0,6}){0,2}|[ \\t]{0,3})", 0..40),
        nl in prop::sample::select(vec!["\n", "\r\n", "\r"]),
    ) {
        check(lines.join(nl).as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..40, edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0u8..4), 1..8)) {
        let fx = corpus();
        let mut d = fx[idx % fx.len()].1.clone();
        for (pos, byte, op) in edits {
            if d.is_empty() { break; }
            let p = pos as usize % d.len();
            match op {
                0 => d[p] = byte,
                1 => d.insert(p, byte),
                2 => { d.remove(p); }
                _ => d.truncate(p),
            }
        }
        check(&d);
    }
}

#[test]
fn deep_and_huge_inputs_give_typed_errors() {
    // nothing nests in STF, but long runs of braces, backslashes and dashes must stay linear
    check(format!("CustomLabel.A\t{}\n", "{".repeat(2_000_000)).as_bytes());
    check(format!("CustomLabel.A\t{}\n", "{!".repeat(1_000_000)).as_bytes());
    check(format!("CustomLabel.A\t{}\n", "\\".repeat(2_000_000)).as_bytes());
    check(format!("{}\t{}\n", "-".repeat(1_000_000), "x").as_bytes());
    check(format!("{}A{}\n", "-".repeat(1_000_000), "-".repeat(1_000_000)).as_bytes());
    check(format!("# KEY{}\n", "\tLABEL".repeat(200_000)).as_bytes());
    check(format!("K{}\n", "\tx".repeat(500_000)).as_bytes());
    // one label over the cap: refused, not truncated
    let big = format!("CustomLabel.A\t{}\n", "a".repeat(100_001));
    assert!(matches!(extract(big.as_bytes()), Err(Error::Limit(_))));
    // more rows than the unit cap: refused
    let mut many = String::new();
    for i in 0..50_001 {
        many.push_str(&format!("K{i}\tv{i}\n"));
    }
    assert!(matches!(extract(many.as_bytes()), Err(Error::Limit(_))));
    assert!(StfFilter.extract_with(many.as_bytes(), &ExtractOptions::default(), &Limits { max_units: 60_000, ..Limits::default() }).is_ok());
    // more lines than the line cap: refused before any scan
    let lines = "\n".repeat(1_000_001);
    assert!(matches!(extract(lines.as_bytes()), Err(Error::Limit(_))));
    // input over the byte cap
    let l = Limits { max_input_bytes: 10, ..Limits::default() };
    assert!(matches!(StfFilter.extract_with(&[b'a'; 11], &opts(), &l), Err(Error::Limit(_))));
    // truncated multi-byte and UTF-16 files, lone surrogates, NUL
    let mut s = "K\t\u{65e5}\u{672c}\u{8a9e}\n".as_bytes().to_vec();
    s.pop();
    s.pop();
    check(&s);
    check(b"\xff\xfeK\x00\t\x00");
    assert!(extract(b"\xff\xfe\x00\xd8").is_err());
    assert!(extract(b"K\tabc\x00\n").is_err());
}
