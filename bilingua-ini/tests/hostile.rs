//! Hostile input: random bytes, mutated fixtures, deep and huge files. Nothing panics; a file that is accepted
//! merges back to its exact bytes.
mod common;
use bilingua_ini::{records, IniFilter, Limits};
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = IniFilter.sniff(data);
    let _ = records(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(IniFilter.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let ids: Vec<(String, String)> =
                doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), plain_text(&u.source()))).collect();
            let n = doc.units.len();
            for (id, t) in ids {
                doc.set_target(&id, vec![Inline::text(format!("~{t}~"))]).unwrap();
            }
            let out = IniFilter.merge(&doc).unwrap();
            assert_eq!(units(&out).len(), n, "unit count after translation {:?} -> {:?}", String::from_utf8_lossy(data), String::from_utf8_lossy(&out));
        }
        Err(Error::Parse { .. } | Error::Unsupported(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(corpus_dir()).unwrap().map(|e| std::fs::read(e.unwrap().path()).unwrap()).collect();
    v.sort();
    v
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..600)) {
        check(&data);
    }

    #[test]
    fn random_text_never_panics(s in "[ -~\\t\\r\\n\\u{a0}\\u{e9}\\u{65e5}\\u{2028}]{0,400}") {
        check(s.as_bytes());
    }

    #[test]
    fn ini_shaped_lines_never_panic(lines in proptest::collection::vec("(\\[[a-z =]{0,6}\\]?|[;#][ a-z]{0,6}|[A-Za-z_]{0,5} ?= ?\"?[A-Za-z %d\\\\\"_QÀ]{0,12}\"?|[ =\"]{0,4})", 0..40), nl in prop::sample::select(vec!["\n", "\r\n", "\r"])) {
        check(lines.join(nl).as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..30, edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0u8..4), 1..8)) {
        let fx = fixtures();
        let mut d = fx[idx % fx.len()].clone();
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
    // nothing to nest in INI, but a very long line, a huge number of sections and brackets must stay linear
    let long_line = format!("k={}\n", "[".repeat(2_000_000));
    check(long_line.as_bytes());
    let brackets = format!("{}{}\n", "[".repeat(500_000), "]".repeat(500_000));
    check(brackets.as_bytes());
    // one value over the cap: refused, not truncated
    let big = format!("k={}\n", "a".repeat(100_001));
    assert!(matches!(extract(big.as_bytes()), Err(Error::Unsupported(_))));
    // more entries than the unit cap: refused
    let mut many = String::new();
    for i in 0..50_001 {
        many.push_str(&format!("k{i}=v{i}\n"));
    }
    assert!(matches!(extract(many.as_bytes()), Err(Error::Unsupported(_))));
    assert!(IniFilter.extract_with(many.as_bytes(), &ExtractOptions::default(), &Limits { max_units: 60_000, ..Limits::default() }).is_ok());
    // more lines than the line cap: refused before any scan
    let lines = "\n".repeat(1_000_001);
    assert!(matches!(extract(lines.as_bytes()), Err(Error::Unsupported(_))));
    // truncated multi-byte and UTF-16 files
    let mut s = "k=日本語\n".as_bytes().to_vec();
    s.pop();
    s.pop();
    check(&s);
    check(b"\xff\xfek\x00=\x00\x3d");
    // a lone surrogate half in a UTF-16 file
    assert!(extract(b"\xff\xfe\x00\xd8").is_err());
}
