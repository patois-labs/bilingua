//! Hostile input: random bytes, mutated fixtures, deep and huge files. Nothing panics; a file that is accepted
//! merges back to its exact bytes, and a translated merge reads back with the same units.
mod common;
use bilingua_ftl::{structure, FtlFilter, Limits};
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = FtlFilter.sniff(data);
    let _ = structure(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(FtlFilter.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let n = doc.units.len();
            let ids: Vec<(String, Vec<Inline>)> = doc.units.iter().map(|u| (u.segments[0].id.clone(), u.segments[0].source.clone())).collect();
            for (id, src) in ids {
                let mut t = vec![Inline::text("~")];
                t.extend(src);
                t.push(Inline::text("~"));
                doc.set_target(&id, merge_text(t)).unwrap();
            }
            let out = FtlFilter.merge(&doc).unwrap();

            let filled = FtlFilter.merge_localized(&doc, Some("pl")).unwrap();
            assert_eq!(structure(&filled).unwrap().junk, structure(&out).unwrap().junk, "plural fill must not add Junk");
            assert_eq!(units(&out).len(), n, "unit count after translation {:?} -> {:?}", String::from_utf8_lossy(data), String::from_utf8_lossy(&out));
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_)) => {}
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
    fn random_text_never_panics(s in "[ -~\t\r\n\u{a0}\u{e9}\u{65e5}\u{2028}]{0,400}") {
        check(s.as_bytes());
    }

    #[test]
    fn ftl_shaped_lines_never_panic(lines in proptest::collection::vec(r#"([#]{1,3} ?[a-z ]{0,6}|-?[a-z][a-z-]{0,5} ?= ?[A-Za-z { }$>\[\]*."-]{0,16}|    ?[.\[*]?[a-z\[\]* =.{}$-]{0,14}|[ {}]{0,3}|\}|\{ \$[a-z]+ ->)"#, 0..40), nl in prop::sample::select(vec!["\n", "\r\n", "\r"])) {
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
    // nesting: refused before the parser
    for n in [41usize, 200, 100_000] {
        let deep = format!("x = {}$a{}\n", "{ ".repeat(n), " }".repeat(n));
        assert!(matches!(extract(deep.as_bytes()), Err(Error::Unsupported(_))), "{n}");
    }
    // nesting at the limit still parses
    let ok = format!("x = {}$a{}\n", "{ ".repeat(40), " }".repeat(40));
    assert!(extract(ok.as_bytes()).is_ok());
    // nested selects stay under the limit
    let mut sel = String::from("x = ");
    for i in 0..20 {
        sel.push_str(&format!("{{ $a{i} ->\n    [one] "));
    }
    sel.push_str("deep");
    for _ in 0..20 {
        sel.push_str("\n   *[other] x\n}");
    }
    sel.push('\n');
    check(sel.as_bytes());
    // a very long line, many brackets, many stray braces: linear, no panic
    check(format!("k = {}\n", "[".repeat(2_000_000)).as_bytes());
    check(format!("k = {}\n", "}".repeat(500_000)).as_bytes());
    check(format!("k = {}\n", "{".repeat(30)).as_bytes());
    check("#".repeat(1_000_000).as_bytes());
    check("#\n".repeat(300_000).as_bytes());
    check("a =\n".repeat(100_000).as_bytes());
    // one value over the cap: refused, not truncated
    let big = format!("k = {}\n", "a".repeat(100_001));
    assert!(matches!(extract(big.as_bytes()), Err(Error::Limit(LimitError { kind: LimitKind::UnitChars, .. }))));
    // more entries than the unit cap: refused
    let mut many = String::new();
    for i in 0..50_001 {
        many.push_str(&format!("k{i} = v{i}\n"));
    }
    assert!(matches!(extract(many.as_bytes()), Err(Error::Limit(LimitError { kind: LimitKind::Units, .. }))));
    assert!(FtlFilter.extract_with(many.as_bytes(), &ExtractOptions::default(), &Limits { max_units: 60_000, ..Limits::default() }).is_ok());
    // more lines than the line cap: refused before any scan
    let lines = "\n".repeat(1_000_001);
    assert!(matches!(extract(lines.as_bytes()), Err(Error::Limit(LimitError { kind: LimitKind::Lines, .. }))));
    // the caller's limits in ExtractOptions are the ones in force
    let few = ExtractOptions::new().with_limits(bilingua_model::Limits::new().with_max_units(2));
    assert!(matches!(FtlFilter.extract(b"a = A\nb = B\nc = C\n", &few), Err(Error::Limit(LimitError { kind: LimitKind::Units, .. }))));
    // over the input size cap
    let small = Limits { max_input_bytes: 10, ..Limits::default() };
    assert!(FtlFilter.extract_with(b"hello = Hello world\n", &ExtractOptions::default(), &small).is_err());
    // truncated multi-byte, a lone BOM, invalid UTF-8 and NUL
    let mut s = "k = \u{65e5}\u{672c}\u{8a9e}\n".as_bytes().to_vec();
    s.pop();
    s.pop();
    assert!(extract(&s).is_err());
    check(b"\xef\xbb\xbf");
    check(b"\xef\xbb\xbf\xef\xbb\xbfk = v\n");
    assert!(extract(b"k = a\x00b\n").is_err());
}
