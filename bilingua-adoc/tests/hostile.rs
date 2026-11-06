//! Hostile input: random bytes, AsciiDoc-shaped token soup, mutated fixtures, every truncation, deep and huge files.
//! Nothing panics; a file that is accepted merges back to its exact bytes (UTF-8 and UTF-16 and the code pages).
mod common;
use bilingua_adoc::Adoc;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = Adoc.sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(Adoc.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let ids: Vec<(String, String)> =
                doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), plain_text(&u.source()))).collect();
            doc.target_lang = Some("de-DE".into());
            for (id, t) in ids {
                doc.set_target(&id, vec![Inline::text(format!("~{t}~"))]).unwrap();
            }
            let out = Adoc.merge(&doc).unwrap();
            // the output can be read again (a typed error is fine, a panic is not)
            let _ = extract(&out);
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
    fn random_text_never_panics(s in "[ -~\\t\\r\\n\\u{a0}\\u{e9}\\u{65e5}\\u{2028}\\u{1c}]{0,400}") {
        check(s.as_bytes());
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
                1 => { d.remove(p); }
                2 => d.insert(p, byte),
                _ => { let end = (p + 1 + byte as usize % 40).min(d.len()); let chunk = d[p..end].to_vec(); d.splice(p..p, chunk); }
            }
        }
        check(&d);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn adoc_shaped_soup_never_panics(words in proptest::collection::vec(
        "(\\*|_|#|`|``|\\+|\\+\\+|\\+\\+\\+|\\^|~|\\[|\\]|\\[\\[|\\]\\]|<<|>>|\\{|\\}|\\\\|link:|xref:|https://x\\.org|kbd:\\[|footnote:\\[|pass:\\[|image::|include::|ifdef::|endif::|----|\\.\\.\\.\\.|\\|===|!===|,===|====|\\*\\*\\*\\*|--|//|////|::|;;|:attr:|= |== |\\. |\\* |-|\\.Title|NOTE: |\\[source\\]|\\[cols=\"1\"\\]|[A-Za-z]{1,6}| \\+\n|\n\n|\n| |\t|\\|)", 0..160),
        nl in prop::sample::select(vec!["\n", "\r\n", "\r"])) {
        check(words.join("").replace('\n', nl).as_bytes());
    }
}

#[test]
fn every_truncation_of_the_fixtures_is_safe() {
    for d in fixtures() {
        let step = if d.len() > 3000 { d.len() / 600 } else { 1 };
        let mut i = 0;
        while i <= d.len() {
            check(&d[..i]);
            i += step.max(1);
        }
    }
}

/// Scanning steps ([`bilingua_model::work`]) one file costs: extract, untouched merge, targets set, merge, re-read.
fn work_of(data: &[u8]) -> u64 {
    bilingua_model::work::count(|| check(data)).1
}

/// Linear work doubles when the input doubles; quadratic work quadruples. Counting steps instead of time makes
/// the answer the same on a slow machine, a busy one and the M5.
fn assert_linear(name: &str, make: impl Fn(usize) -> Vec<u8>, n: usize) {
    let (small, big) = (work_of(&make(n)), work_of(&make(2 * n)));
    assert!(small > 0, "{name}: no work counted");
    assert!(big * 10 <= small * 25 + 2_000, "{name}: {small} steps for n={n}, {big} for n={}: more than linear", 2 * n);
}

#[test]
fn deep_nesting_and_long_runs_are_linear() {
    let r = |s: &'static str| move |k: usize| s.repeat(k).into_bytes();
    assert_linear("nesting", r("*_`#"), 15_000);
    assert_linear("marks", |k| format!("{}x{}\n", "*_".repeat(k), "_*".repeat(k)).into_bytes(), 25_000);
    assert_linear("roles", |k| format!("{}\n", "[.r]#".repeat(k)).into_bytes(), 25_000);
    assert_linear("links", |k| format!("{}\n", "link:x[".repeat(k)).into_bytes(), 25_000);
    assert_linear("xrefs", |k| format!("{}\n", "<<a,".repeat(k)).into_bytes(), 25_000);
    assert_linear("tables", r("|===\n|a\n"), 15_000);
    assert_linear("nested tables", r("!===\n|===\n!===\n"), 15_000);
    assert_linear("listing fences", r("----\n"), 30_000);
    assert_linear("description lists", r("a::\n"), 30_000);
    assert_linear("line breaks", |k| format!("{}\n", " +\n".repeat(k)).into_bytes(), 30_000);
    assert_linear("words", |k| format!("{}\n", "a ".repeat(k)).into_bytes(), 30_000);
    assert_linear("passthrough", |k| format!("text {}\n", "+x ".repeat(k)).into_bytes(), 15_000);
    assert_linear("monospace", |k| format!("text {}\n", "`x ".repeat(k)).into_bytes(), 15_000);
    assert_linear("bold", |k| format!("text {}\n", "*x ".repeat(k)).into_bytes(), 15_000);
    assert_linear("attribute continuation", |k| format!(":a: \\\n{}", "b \\\n".repeat(k)).into_bytes(), 30_000);
    assert_linear("header continuation", |k| format!("= T\n:description: {}\n", "a \\\n  ".repeat(k)).into_bytes(), 15_000);
    assert_linear("attribute refs", |k| format!("{}\n", "{a}".repeat(k)).into_bytes(), 15_000);
    assert_linear("urls", |k| format!("{}\n", "https://x.org/ ".repeat(k)).into_bytes(), 15_000);
    // one unit near the 100 000-character cap: scans that never find their closer stop at the pattern's bound
    for p in ["link:", "{set:", "pass:[", "a link:a[", "x <<y", "[[z"] {
        assert_linear(p, |k| format!("Text {}\n", p.repeat(k / p.len())).into_bytes(), 45_000);
    }
}

#[test]
fn limits_are_typed_errors() {
    let many: String = (0..200).map(|i| format!("* item {i}\n")).collect();
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(100));
    assert!(matches!(Adoc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_input_bytes(50));
    assert!(matches!(Adoc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_lines(10));
    assert!(matches!(Adoc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Lines));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_unit_chars(10));
    assert!(matches!(Adoc.extract(b"a very long paragraph indeed\n", &o), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
}

#[test]
fn encoding_errors_are_typed() {
    for data in [&b"\x89PNG\r\n\x1a\n\x00\x00"[..], b"abc\x81def"] {
        assert!(matches!(extract(data), Err(Error::Encoding(_))), "{data:?}");
    }
}
