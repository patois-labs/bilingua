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

#[test]
fn deep_nesting_and_long_runs_are_linear() {
    let t0 = std::time::Instant::now();
    check("*_`#".repeat(30_000).as_bytes());
    check(format!("{}x{}\n", "*_".repeat(50_000), "_*".repeat(50_000)).as_bytes());
    check(format!("{}\n", "[.r]#".repeat(50_000)).as_bytes());
    check(format!("{}\n", "link:x[".repeat(50_000)).as_bytes());
    check(format!("{}\n", "<<a,".repeat(50_000)).as_bytes());
    check("|===\n|a\n".repeat(30_000).as_bytes());
    check("!===\n|===\n!===\n".repeat(30_000).as_bytes());
    check("----\n".repeat(60_000).as_bytes());
    check("a::\n".repeat(60_000).as_bytes());
    check(format!("{}\n", " +\n".repeat(60_000)).as_bytes());
    check(format!("{}\n", "a ".repeat(60_000)).as_bytes());
    check(format!("text {}\n", "+x ".repeat(30_000)).as_bytes());
    check(format!("text {}\n", "`x ".repeat(30_000)).as_bytes());
    check(format!("text {}\n", "*x ".repeat(30_000)).as_bytes());
    check(format!(":a: \\\n{}", "b \\\n".repeat(60_000)).as_bytes());
    check(format!("= T\n:description: {}\n", "a \\\n  ".repeat(30_000)).as_bytes());
    check(format!("{}\n", "{a}".repeat(30_000)).as_bytes());
    check(format!("{}\n", "https://x.org/ ".repeat(30_000)).as_bytes());
    // one unit near the 100 000-character cap: scans that never find their closer stop at the pattern's bound
    for p in ["link:", "{set:", "pass:[", "a link:a[", "x <<y", "[[z"] {
        check(format!("Text {}\n", p.repeat(90_000 / p.len())).as_bytes());
    }
    assert!(t0.elapsed().as_secs() < 60, "took {:?}", t0.elapsed());
}

#[test]
fn limits_are_typed_errors() {
    let many: String = (0..200).map(|i| format!("* item {i}\n")).collect();
    let o = ExtractOptions { limits: Some(Limits { max_units: 100, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Adoc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let o = ExtractOptions { limits: Some(Limits { max_input_bytes: 50, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Adoc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    let o = ExtractOptions { limits: Some(Limits { max_lines: 10, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Adoc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Lines));
    let o = ExtractOptions { limits: Some(Limits { max_unit_chars: 10, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Adoc.extract(b"a very long paragraph indeed\n", &o), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
}

#[test]
fn encoding_errors_are_typed() {
    for data in [&b"\x89PNG\r\n\x1a\n\x00\x00"[..], b"abc\x81def"] {
        assert!(matches!(extract(data), Err(Error::Encoding(_))), "{data:?}");
    }
}
