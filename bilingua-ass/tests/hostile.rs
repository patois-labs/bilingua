//! Hostile input: random bytes, mutated fixtures, deep and huge files. Nothing panics; a file that is accepted
//! merges back to its exact bytes and a pseudo-translation of it reads back with the same unit count.
mod common;
use bilingua_ass::AssFilter;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = AssFilter.sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(AssFilter.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let items: Vec<(String, Vec<Inline>)> = doc
                .units
                .iter()
                .filter(|u| u.translate)
                .map(|u| {
                    let t = u
                        .source()
                        .into_iter()
                        .map(|i| match i {
                            Inline::Text(t) => Inline::Text(format!("~{t}~")),
                            x => x,
                        })
                        .collect();
                    (u.segments[0].id.clone(), t)
                })
                .collect();
            let n = doc.units.len();
            doc.set_targets(items).unwrap();
            doc.target_lang = Some("de".into());
            let out = AssFilter.merge_with(&doc, &MergeOptions::locos()).unwrap();
            // a translation can grow a line past the cap (each `{\r}` closing many pairs writes them all): refused, typed
            let back = match extract(&out) {
                Ok(b) => b,
                Err(Error::Limit(_)) => return,
                Err(e) => panic!("{e} for {:?} -> {:?}", String::from_utf8_lossy(data), String::from_utf8_lossy(&out)),
            };
            assert_eq!(back.units.len(), n, "unit count after translation {:?} -> {:?}", String::from_utf8_lossy(data), String::from_utf8_lossy(&out));
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_)) => {}
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
    fn random_text_never_panics(s in "[ -~\\t\\r\\n\\u{a0}\\u{e9}\\u{65e5}\\u{2028}\\u{130}]{0,400}") {
        check(s.as_bytes());
    }

    #[test]
    fn ass_shaped_lines_never_panic(lines in proptest::collection::vec("(\\[(Events|Script Info|V4\\+ Styles)\\]|Format ?: ?[A-Za-z, ]{0,40}|Title ?: ?[A-Za-z <>]{0,8}|Language ?:[a-z ]{0,4}|(Dialogue|Comment) ?: ?[0-9],[0-9:.]{0,5},[a-z]{0,3},[A-Za-z ,{}\\\\NnhipbrB0-9&]{0,30}|[ -~]{0,10})", 0..40), nl in prop::sample::select(vec!["\n", "\r\n", "\r"])) {
        check(lines.join(nl).as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..40, edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0u8..4), 1..8)) {
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

const HEAD: &str = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,";

#[test]
fn deep_and_huge_inputs_give_typed_errors_or_stay_linear() {
    // nesting: ten thousand style switches opened and closed in one line
    let nest = format!("{HEAD}{}x{}\n", "{\\b1}".repeat(10_000), "{\\b0}".repeat(10_000));
    check(nest.as_bytes());
    let nest2 = format!("{HEAD}{}x{}\n", "{\\i1}".repeat(10_000), "{\\b0}".repeat(10_000));
    check(nest2.as_bytes());
    let resets = format!("{HEAD}{}x{}\n", "{\\b1}a".repeat(8_000), "{\\r}".repeat(8_000));
    check(resets.as_bytes());
    // braces: many open braces (no close), and many close braces
    check(format!("{HEAD}{}\n", "{".repeat(90_000)).as_bytes());
    check(format!("{HEAD}x{}\n", "}".repeat(90_000)).as_bytes());
    check(format!("{HEAD}{}x\n", "{a".repeat(45_000)).as_bytes());
    check(format!("{HEAD}{}\n", "\\N".repeat(45_000)).as_bytes());
    // a Format line with fifty thousand fields; a Dialogue line with ninety thousand commas
    check(format!("[Events]\nFormat: {}Text\nDialogue: x\n", "a,".repeat(50_000)).as_bytes());
    check(format!("{HEAD}{}\n", ",".repeat(90_000)).as_bytes());
    // one Text field over the cap: refused, not truncated
    let big = format!("{HEAD}{}\n", "a".repeat(100_001));
    assert!(matches!(extract(big.as_bytes()), Err(Error::Limit(_))));
    // more events than the unit cap: refused
    let many = format!("[Events]\n{}", "Dialogue: 0,0,0,Default,,0,0,0,,Hi\n".repeat(50_001));
    assert!(matches!(extract(many.as_bytes()), Err(Error::Limit(_))));
    let wide = ExtractOptions { limits: Some(Limits { max_units: 60_000, ..Limits::default() }), ..Default::default() };
    assert!(extract_with(many.as_bytes(), &wide).is_ok());
    // more lines than the line budget: refused before any scan
    assert!(matches!(extract("\n".repeat(1_000_001).as_bytes()), Err(Error::Limit(_))));
    // truncated multi-byte and UTF-16 files
    let mut s = format!("{HEAD}\u{65e5}\u{672c}\u{8a9e}\n").into_bytes();
    s.pop();
    s.pop();
    check(&s);
    check(b"\xff\xfe[\x00E\x00");
    assert!(extract(b"\xff\xfe\x00\xd8").is_err()); // a lone surrogate half
    // an input over the byte cap
    let tiny = ExtractOptions { limits: Some(Limits { max_input_bytes: 10, ..Limits::default() }), ..Default::default() };
    assert!(matches!(extract_with(b"[Events]\nDialogue: 0,0,0,0,0,0,0,0,0,hello", &tiny), Err(Error::Limit(_))));
}

#[test]
fn deep_nesting_is_fast() {
    let t = std::time::Instant::now();
    let nest = format!("{HEAD}{}x{}\n", "{\\b1}".repeat(9_000), "{\\b0}".repeat(9_000));
    let doc = extract(nest.as_bytes()).unwrap();
    assert_eq!(doc.units.len(), 1);
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
}
