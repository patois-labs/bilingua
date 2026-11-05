//! Hostile input: random bytes, wiki-shaped token soup, mutated fixtures, every truncation, deep and huge files.
//! Nothing panics; a file that is accepted merges back to its exact bytes, and a pseudo-translated merge reads back.
mod common;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn check(f: &dyn Filter, data: &[u8]) {
    let _ = f.sniff(data);
    match extract(f, data) {
        Ok(mut doc) => {
            assert_eq!(f.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let n = doc.units.len();
            doc.target_lang = Some("de-DE".into());
            for u in doc.units.iter_mut().filter(|u| u.translate) {
                let mut t = vec![Inline::text("~")];
                t.extend(u.source());
                t.push(Inline::text("~"));
                u.segments[0].target = merge_text(t);
            }
            let out = f.merge(&doc).unwrap();
            let back = extract(f, &out).unwrap_or_else(|e| panic!("merged output unreadable: {e}"));
            // The unit count is not asserted here: in a page that is already broken (an unclosed `<nowiki>`), the
            // inert `<nowiki>` a translation needs can close it and move the unit boundaries, as in LocOS. The
            // corpus round trip (tests/roundtrip.rs) asserts the count for well-formed pages.
            let _ = (n, back);
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn both(data: &[u8]) {
    check(&mw(), data);
    check(&dw(), data);
}

fn fixtures() -> Vec<(bool, Vec<u8>)> {
    let mut v: Vec<(bool, Vec<u8>)> = corpus().into_iter().map(|(n, d)| (n.ends_with(".dokuwiki"), d)).collect();
    v.push((false, MW.as_bytes().to_vec()));
    v.push((true, DW.as_bytes().to_vec()));
    v
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..600)) {
        both(&data);
    }

    #[test]
    fn random_text_never_panics(s in "[ -~\t\r\n\u{a0}\u{e9}\u{65e5}\u{2028}]{0,400}") {
        both(s.as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..80, edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0u8..4), 1..8)) {
        let fx = fixtures();
        let (is_dw, src) = &fx[idx % fx.len()];
        let mut d = src.clone();
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
        if *is_dw { check(&dw(), &d); check(&mw(), &d); } else { check(&mw(), &d); check(&dw(), &d); }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(250))]

    #[test]
    fn wiki_shaped_soup_never_panics(words in proptest::collection::vec(
        r"(\[\[|\]\]|\{\{|\}\}|\{\||\|\}|\|\||\||!!|!|'''|''|=|==|<!--|-->|<nowiki>|</nowiki>|<pre>|</pre>|<code>|</code>|<ref>|</ref>|<br/>|&amp;|&#12;|__NOTOC__|~~~~|\*\*|//|__|%%|\(\(|\)\)|\^|>|\\\\|~~NOTOC~~|File:|Category:|#REDIRECT|https?://x\.y|\[https://x\.y |[a-z]{1,6}|\u{e9}|\u{65e5}|;|:|\*|#|-{4}|  |\t|\n|\r\n| )", 0..140),
        glue in prop::sample::select(vec!["", " "])) {
        both(words.join(glue).as_bytes());
    }
}

#[test]
fn every_truncation_of_the_fixtures_is_safe() {
    for (is_dw, d) in fixtures() {
        let step = if d.len() > 1500 { d.len() / 400 } else { 1 };
        let mut i = 0;
        while i <= d.len() {
            if is_dw { check(&dw(), &d[..i]) } else { check(&mw(), &d[..i]) }
            i += step.max(1);
        }
    }
}

#[test]
fn deep_nesting_and_long_runs_are_linear() {
    let t0 = std::time::Instant::now();
    // the debug build (cargo test) runs a fifth of the size; release runs the full sizes
    let n = |x: usize| if cfg!(debug_assertions) { x / 5 } else { x };
    let r = |s: &str, k: usize| s.repeat(k);
    let cases = [
        format!("{}x{}", r("{{a|", n(200_000)), r("}}", n(200_000))),
        format!("{}x{}", r("[[a|", n(200_000)), r("]]", n(200_000))),
        format!("{}x{}", r("'''a ''b ", n(100_000)), r("''' ''", n(100_000))),
        r("<!--", n(100_000)),
        r("<nowiki>", n(100_000)),
        r("<pre>", n(100_000)),
        format!("{}{}", r("<ref>", n(50_000)), r("</ref>", n(50_000))),
        r("[http://x.y z", n(50_000)),
        r("{|\n|-\n| a || b\n", n(50_000)),
        r("[[File:a.png|thumb|", n(50_000)),
        r("= a =\n", n(100_000)),
        r("; a : b\n", n(100_000)),
        r("\n", n(200_000)),
        r("&#1;", n(100_000)),
        format!("[[{}]]", "a|".repeat(n(300_000))),
        format!("[[a|{}]]", "[[b|".repeat(n(100_000))),
        r("**a** //b// __c__ ''d'' ", n(20_000)),
        r("%%", n(100_000)),
        r("<code>", n(100_000)),
        r("~~NOTOC", n(100_000)),
        r("((", n(100_000)),
        format!("a{}b", r("\\\\", n(200_000))),
        r("| a | b\n^ c ^ d\n", n(50_000)),
        format!("[[{}", "[".repeat(n(300_000))),
    ];
    for (ci, c) in cases.iter().enumerate() {
        let t1 = std::time::Instant::now();
        let o = ExtractOptions { limits: Some(Limits { max_units: 10_000_000, max_lines: 100_000_000, ..Limits::default() }), ..Default::default() };
        for f in [&mw() as &dyn Filter, &dw() as &dyn Filter] {
            match f.extract(c.as_bytes(), &o) {
                Ok(doc) => assert_eq!(f.merge(&doc).unwrap(), c.as_bytes()),
                Err(Error::Limit(_) | Error::Parse { .. }) => {}
                Err(e) => panic!("{e}"),
            }
        }
        if std::env::var("WIKI_HOSTILE_TIMES").is_ok() {
            eprintln!("case {ci}: {:?}", t1.elapsed());
        }
    }
    assert!(t0.elapsed().as_secs() < 120, "took {:?}", t0.elapsed());
}

#[test]
fn limits_are_typed_errors() {
    let many: String = (0..200).map(|i| format!("* Item {i}\n")).collect();
    let o = ExtractOptions { limits: Some(Limits { max_units: 100, ..Limits::default() }), ..Default::default() };
    assert!(matches!(mw().extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let o = ExtractOptions { limits: Some(Limits { max_input_bytes: 50, ..Limits::default() }), ..Default::default() };
    assert!(matches!(mw().extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    let o = ExtractOptions { limits: Some(Limits { max_lines: 10, ..Limits::default() }), ..Default::default() };
    assert!(matches!(dw().extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Lines));
    let o = ExtractOptions { limits: Some(Limits { max_unit_chars: 10, ..Limits::default() }), ..Default::default() };
    assert!(matches!(mw().extract(b"A very long paragraph indeed.\n", &o), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
}

#[test]
fn encoding_errors_are_typed() {
    for data in [&b"\xff\xfeS\x00T"[..], &b"Text\x81\x8d\x90\n"[..]] {
        assert!(matches!(extract(&mw(), data), Err(Error::Encoding(_))), "{data:?}");
    }
}
