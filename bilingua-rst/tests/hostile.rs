//! Hostile input: random bytes, reStructuredText-shaped token soup, mutated fixtures, every truncation, deep and
//! huge files. Nothing panics; a file that is accepted merges back to its exact bytes, and a translated merge
//! reads back (a table that cannot fit is a typed refusal).
mod common;
use bilingua_model::*;
use bilingua_rst::Rst;
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = Rst.sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(Rst.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let n = doc.units.len();
            for u in &mut doc.units {
                let mut t = vec![Inline::text("Zq ")];
                t.extend(u.source());
                u.segments[0].target = merge_text(t);
            }
            match Rst.merge(&doc) {
                Ok(out) => {
                    let _ = extract(&out).unwrap_or_else(|e| panic!("merged output unreadable: {e}"));
                    let _ = n;
                }
                Err(Error::Write(m)) => assert!(m.contains("does not fit"), "{m}"),
                Err(e) => panic!("unexpected merge error: {e}"),
            }
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            p.extension().is_some_and(|x| x == "rst").then(|| std::fs::read(p).unwrap())
        })
        .collect();
    v.sort();
    v
}

const SNIPPETS: &[&str] = &[
    "=====", "-----", "~~~~~", "+----+", "+====+", "| a |", "|", "..", ".. note::", ".. code-block:: x", ".. _t:", ".. [1]", ".. |s| replace:: x", "::", ":", ":a:",
    "`", "``", "*", "**", "_", "__", "|x|", "[1]_", ":ref:`x`", "`a <b>`_", "http://x.y/z", "\\", "\\ ", "- ", "* ", "1. ", "#. ", "(a) ", "A. ", ">>> ",
    "   ", "\t", "  ", "\n", "\n", "\n\n", "\r\n", "\r", " ", "word", "Caf\u{e9}", "\u{65e5}\u{672c}", "e\u{301}", "\u{3000}", "\u{a0}", "=  =", "==  ==", "-- --",
    "<", ">", "<<", ">>", "[", "]", "{", "}", "(", ")", "$", "%", "&", "#", "@", "!", "?", "^", "'", "\"", ",", ".", ";",
];

fn soup() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(prop::sample::select(SNIPPETS), 0..120).prop_map(|v| v.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..400)) {
        check(&data);
    }

    #[test]
    fn rst_shaped_soup_never_panics(data in soup()) {
        check(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..64, edits in prop::collection::vec((any::<u16>(), any::<u8>(), 0u8..3), 1..6)) {
        let fx = fixtures();
        let mut d = fx[idx % fx.len()].clone();
        for (pos, byte, kind) in edits {
            if d.is_empty() { break; }
            let p = pos as usize % d.len();
            match kind {
                0 => { d[p] = byte; }
                1 => { d.remove(p); }
                _ => { d.insert(p, byte); }
            }
        }
        check(&d);
    }
}

#[test]
fn every_truncation_of_every_fixture() {
    for d in fixtures() {
        for n in 0..=d.len() {
            check(&d[..n]);
        }
    }
}

#[test]
fn every_line_deletion_and_duplication_of_the_fixtures() {
    for d in fixtures() {
        let text = String::from_utf8(d).unwrap();
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        for i in 0..lines.len() {
            let mut a = lines.clone();
            a.remove(i);
            check(a.concat().as_bytes());
            let mut b = lines.clone();
            b.insert(i, lines[i]);
            check(b.concat().as_bytes());
        }
    }
}

#[test]
fn deep_and_huge_inputs() {
    // nesting far beyond the walk's depth limit
    let deep: String = (0..3000).map(|k| format!("{}- item {k}\n\n", " ".repeat(k * 2 % 2000))).collect();
    check(deep.as_bytes());
    let quotes: String = (0..500).map(|k| format!("{}text\n", " ".repeat(k + 1))).collect();
    check(quotes.as_bytes());
    // one enormous line, no break
    check("word ".repeat(300_000).as_bytes());
    check(&b"*".repeat(1_000_000));
    check(&b"`".repeat(1_000_000));
    check(&b"|".repeat(100_000));
    check(&b"a_".repeat(100_000));
    check(&b"[".repeat(300_000));
    check(&b":a:`".repeat(100_000));
    check(&b"<".repeat(100_000));
    check(&b"`a <".repeat(50_000));
    check(&b"http:".repeat(100_000));
    // many lines
    check(&b"x\n".repeat(200_000));
    check(&b"\n".repeat(500_000));
    check(&b"- \n".repeat(100_000));
    check(&b"..\n".repeat(100_000));
    check(&b".. a::\n   b\n".repeat(50_000));
    check(&b"::\n  x\n".repeat(50_000));
    check(&b"+-+\n| |\n+-+\n\n".repeat(20_000));
    check(&b"=  =\nx  y\n=  =\n\n".repeat(20_000));
    // a grid close to the probe budget and one over the area limit
    let mut g = String::from("+");
    g.push_str(&"-+".repeat(700));
    g.push('\n');
    for _ in 0..200 {
        g.push('|');
        g.push_str(&"a|".repeat(700));
        g.push('\n');
        g.push('+');
        g.push_str(&"-+".repeat(700));
        g.push('\n');
    }
    check(g.as_bytes());
    // a title that is a million columns wide
    let wide = format!("{}\n{}\n", "x".repeat(1_000_000), "=".repeat(1_000_000));
    check(wide.as_bytes());
}

#[test]
fn limits_are_typed_errors() {
    let many: Vec<u8> = (0..100).flat_map(|i| format!("Paragraph number {i}.\n\n").into_bytes()).collect();
    let o = |l: Limits| ExtractOptions { limits: Some(l), ..Default::default() };
    assert!(matches!(Rst.extract(&many, &o(Limits { max_units: 50, ..Limits::default() })), Err(Error::Limit(_))));
    assert!(matches!(Rst.extract(&many, &o(Limits { max_input_bytes: 100, ..Limits::default() })), Err(Error::Limit(_))));
    assert!(matches!(Rst.extract(&many, &o(Limits { max_lines: 10, ..Limits::default() })), Err(Error::Limit(_))));
    assert!(matches!(Rst.extract(&many, &o(Limits { max_unit_chars: 5, ..Limits::default() })), Err(Error::Limit(_))));
}

#[test]
fn a_foreign_skeleton_is_refused() {
    let mut doc = extract(b"Hello world.\n").unwrap();
    doc.skeleton = Skeleton::Text { parts: vec![SkelPart::Unit(7)], encoding: "utf-8".into(), parser: Some(ParserInfo::new("bilingua-other", "0.1.0", 1)) };
    assert!(matches!(Rst.merge(&doc), Err(Error::ParserVersion { .. })));
    doc.skeleton = Skeleton::Text { parts: vec![SkelPart::Unit(7)], encoding: "utf-8".into(), parser: None };
    assert!(matches!(Rst.merge(&doc), Err(Error::Write(_))));
    doc.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(matches!(Rst.merge(&doc), Err(Error::Write(_))));
}
