//! Hostile input: random bytes, LaTeX-shaped token soup, mutated fixtures, every truncation, deep and huge files.
//! Nothing panics; a file that is accepted merges back to its exact bytes; a translated merge never panics and
//! never unbalances the braces of a unit.
mod common;
use bilingua_latex::Latex;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = Latex::default().sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(Latex::default().merge(&doc).unwrap(), data, "untouched merge must be exact");
            let n = doc.units.len();
            doc.target_lang = Some("de-DE".into());
            for u in doc.units.iter_mut().filter(|u| u.translate) {
                let mut t = vec![Inline::text("~ & % ")];
                t.extend(u.source());
                u.segments[0].target = t;
            }
            let out = Latex::default().merge(&doc).unwrap();
            match extract(&out) {
                Ok(back) => assert!(back.units.len() <= 6 * n + 5, "unit count exploded"),
                Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_)) => {}
                Err(e) => panic!("unexpected error kind on the merged output: {e}"),
            }
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
    fn random_text_never_panics(s in "[ -~\\t\\r\\n\\u{a0}\\u{e9}\\u{65e5}\\u{2028}]{0,400}") {
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

const WORDS: &[&str] = &[
    "\\begin{document}", "\\end{document}", "\\begin{verbatim}", "\\end{verbatim}", "\\begin{equation}", "\\end{equation}",
    "\\begin{tabular}{ll}", "\\end{tabular}", "\\begin{itemize}", "\\end{itemize}", "\\item", "\\item[", "\\section{", "\\section*",
    "\\emph{", "\\textbf", "\\href{u}{", "\\verb", "\\verb*", "\\lstinline", "\\\\", "\\[", "\\]", "\\(", "\\)", "$$", "$", "{", "}", "[", "]",
    "&", "%", "%ab\n", "~", "\\foo", "\\Q", "\\.", "word", "Text", "caf\u{e9}", "\n\n", "\n", " ", "\\caption{", "\\input{x}", "\\newcommand",
    "\\def", "\\url{a%b}", "\\mintinline", "\\captionof", "\\begin{document }", "\\end {document}",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(600))]

    #[test]
    fn latex_shaped_soup_never_panics(picks in proptest::collection::vec(0usize..WORDS.len(), 0..160), crlf in any::<bool>()) {
        let s: String = picks.iter().map(|&i| WORDS[i]).collect::<Vec<_>>().join(" ");
        let s = if crlf { s.replace('\n', "\r\n") } else { s };
        check(s.as_bytes());
    }
}

#[test]
fn soup_regression_url_percent_before_begin() {
    // the shrunk `latex_shaped_soup_never_panics` case: the `%` in `\url{a%b}` hides the `{` of the next `\begin` from
    // the group maps, which left `{document}` at the start of a unit, and a translation was written between `\begin`
    // and its name (the re-read found 12 units for 1)
    let picks = [0usize, 48, 0, 42, 24, 0, 0, 45, 0, 0, 45, 0, 0, 0, 0, 0];
    let s: String = picks.iter().map(|&i| WORDS[i]).collect::<Vec<_>>().join(" ");
    check(s.as_bytes());
    check(b"\\begin{document}\n\\url{a%b} \\begin{itemize} Text \\end{itemize} more\n\\url{a%b} \\end{x} words\n\\end{document}\n");
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
    let doc = |s: String| format!("\\begin{{document}}\n{s}\n\\end{{document}}\n");
    let cases: Vec<String> = vec![
        doc(format!("\\section{{{}x{}}}", "{".repeat(100_000), "}".repeat(100_000))),
        doc("{".repeat(200_000)),
        doc("\\emph{".repeat(100_000)),
        doc(format!("{}x{}", "\\emph{".repeat(5_000), "}".repeat(5_000))),
        doc("$".repeat(100_001)),
        doc("$a ".repeat(100_000)),
        doc("\\[ ".repeat(100_000)),
        doc("\\( ".repeat(100_000)),
        doc("\\begin{verbatim} ".repeat(100_000)),
        doc("\\begin{equation} ".repeat(100_000)),
        doc("\\begin{itemize} ".repeat(100_000)),
        doc("\\item[ ".repeat(100_000)),
        doc("\\verb".repeat(100_000)),
        doc("\\verb|a ".repeat(100_000)),
        doc("\\\\".repeat(200_000)),
        doc("\\end{x} ".repeat(100_000)),
        doc("%\n".repeat(100_000)),
        doc("\\href{u}{".repeat(50_000)),
        doc("\\begin{tabular}{ll} a & b \\\\ ".repeat(50_000)),
        "\\begin{document}\n".repeat(50_000),
        "\\end{document}\n".repeat(50_000),
        "%\\begin{document}\n".repeat(50_000),
        "[".repeat(300_000),
        "\\".repeat(300_001),
        "\\section[".repeat(100_000),
        doc("\u{e9}\\".repeat(100_000)),
    ];
    for c in &cases {
        check(c.as_bytes());
    }
    assert!(t0.elapsed().as_secs() < 60, "took {:?}", t0.elapsed());
}

#[test]
fn limits_are_typed_errors() {
    let many: String = (0..200).map(|i| format!("Paragraph {i}.\n\n")).collect();
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(100));
    assert!(matches!(Latex::default().extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_input_bytes(50));
    assert!(matches!(Latex::default().extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_lines(10));
    assert!(matches!(Latex::default().extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Lines));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_unit_chars(10));
    assert!(matches!(Latex::default().extract(b"A very long paragraph indeed.\n", &o), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
}

#[test]
fn encoding_errors_are_typed() {
    for data in [&b"\xff\xfeS\x00T"[..], &b"\x81\n"[..], &b"\x01\x02\x03\x04\x05\x06\x07\x08\xe9"[..]] {
        assert!(matches!(extract(data), Err(Error::Encoding(_))), "{data:?}");
    }
}

#[test]
fn a_merge_never_unbalances_a_unit() {
    let data = b"\\begin{document}\nA \\textbf{bold \\emph{very} word} and \\footnote{note} end.\n\\end{document}\n";
    let doc = extract(data).unwrap();
    // every way to drop, repeat or reorder the tags of the unit
    let tag_ids: Vec<Inline> = doc.units[0].source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    for mask in 0u32..(1 << tag_ids.len().min(10)) {
        for rev in [false, true] {
            let mut t = vec![Inline::text("x ")];
            let order: Vec<usize> = if rev { (0..tag_ids.len()).rev().collect() } else { (0..tag_ids.len()).collect() };
            for k in order {
                if mask & (1 << k) != 0 {
                    t.push(tag_ids[k].clone());
                    t.push(Inline::text(" y "));
                }
            }
            let mut d = doc.clone();
            d.units[0].segments[0].target = t;
            let out = String::from_utf8(Latex::default().merge(&d).unwrap()).unwrap();
            assert_eq!(out.matches('{').count(), out.matches('}').count(), "{mask:b}: {out}");
        }
    }
}
