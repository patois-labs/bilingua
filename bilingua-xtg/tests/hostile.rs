//! Hostile input: random bytes, XPress-Tags-shaped token soup, mutated fixtures, every truncation, deep and huge
//! files. Nothing panics; a file that is accepted merges back to its exact bytes (but for a multi-byte code page
//! whose bytes have several spellings, which the document warns about) and a translated merge reads back.
mod common;
use bilingua_model::*;
use bilingua_xtg::Xtg;
use common::*;
use proptest::prelude::*;

fn exact_expected(doc: &Document) -> bool {
    !doc.warnings.iter().any(|w| w.message.contains("more than one spelling"))
}

fn check(data: &[u8]) {
    let _ = Xtg.sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            if exact_expected(&doc) {
                assert_eq!(Xtg.merge(&doc).unwrap(), data, "untouched merge must be exact");
            }
            let n = doc.units.len();
            for u in doc.units.iter_mut() {
                let mut t = vec![Inline::text("~")];
                t.extend(u.source());
                u.segments[0].target = merge_text(t);
            }
            let out = Xtg.merge(&doc).unwrap_or_else(|e| panic!("translated merge failed: {e}"));
            // a BOM-less UTF-16 file is found by its first bytes; a translation cannot move them
            let back = extract(&out).unwrap_or_else(|e| panic!("merged output unreadable: {e}"));
            assert!(back.units.len() <= n, "{} vs {n}", back.units.len());
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/xtg");
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(dir).unwrap().map(|e| std::fs::read(e.unwrap().path()).unwrap()).collect();
    v.push(b"<v19.00><e9>\r\n@Body:Hello <B>bold<P> <A(3,\"x\",)[0]>link<\\n>end\r\n".to_vec());
    v.sort();
    v
}

const BITS: &[&str] = &[
    "<v19.00><e9>", "<e0>", "<e8>", "<B>", "<B$>", "<I>", "<P>", "<$>", "<f\"Arial\">", "<f$>", "<z12>", "<@Emph>", "<@$>", "<\\n>", "<\\@>",
    "<\\#13>", "<\\", "<A(", ")", "\"", ">", "<", "@Body:", "@:", "@N=x", "~M=<y>", "\r\n", "\r", "\n", "Hello ", "caf\u{e9}", "\u{27E6}", "\\",
    "<*L>", "<XI,\"a\">", "<BI>", "<//>", "<s50>",
];

fn soup(picks: &[usize]) -> String {
    picks.iter().map(|&i| BITS[i % BITS.len()]).collect()
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
    fn token_soup_never_panics(picks in proptest::collection::vec(0usize..200, 0..80)) {
        check(soup(&picks).as_bytes());
        let mut with_head = b"<v19.00><e9>\r\n".to_vec();
        with_head.extend_from_slice(soup(&picks).as_bytes());
        check(&with_head);
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
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn arbitrary_translations_keep_the_file_readable(picks in proptest::collection::vec(0usize..200, 0..20), t in "\\PC{0,60}") {
        let src = b"<v19.00><e9>\r\n@Body:Hello <B>bold<P> world<\\n>second <I>and <B>x<B$><I$> line\r\n@Body:Two <A(3,\"x\",)[0]>link\r\n";
        let mut doc = extract(src).unwrap();
        let n = doc.units.len();
        for (k, u) in doc.units.iter_mut().enumerate() {
            let mut tags: Vec<Inline> = u.source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
            // shuffle, drop and repeat the codes by the random picks
            let mut out = vec![Inline::text(t.clone())];
            for &p in &picks {
                if tags.is_empty() { break; }
                let i = p % tags.len();
                out.push(tags[i].clone());
                out.push(Inline::text(format!(" w{k}{p} ")));
                if p % 3 == 0 { tags.remove(i); }
            }
            u.segments[0].target = merge_text(out);
        }
        let out = Xtg.merge(&doc).unwrap();
        let back = extract(&out).unwrap();
        prop_assert!(back.units.len() <= n);
    }
}

#[test]
fn every_truncation_of_every_fixture_is_clean() {
    for f in fixtures() {
        let step = (f.len() / 300).max(1);
        for cut in (0..=f.len()).step_by(step) {
            check(&f[..cut]);
        }
    }
}

#[test]
fn deep_nesting_is_refused_not_recursed() {
    for depth in [65usize, 1_000, 200_000] {
        let mut d = b"<v1><e9>\n@N:x<A".to_vec();
        d.extend(std::iter::repeat_n(b'(', depth));
        d.extend_from_slice(b">\n");
        assert!(matches!(extract(&d), Err(Error::Limit(_))), "depth {depth}");
    }
    // deeply nested but legal (64) and a very long balanced tag
    let mut d = b"<v1><e9>\n@N:x<A".to_vec();
    d.extend(std::iter::repeat_n(b'(', 64));
    d.extend(std::iter::repeat_n(b')', 64));
    d.extend_from_slice(b">y\n");
    assert!(extract(&d).is_ok());
    let mut long = b"<v1><e9>\n@N:x<A(".to_vec();
    long.extend(std::iter::repeat_n(b'a', 70_000));
    long.extend_from_slice(b")>y\n");
    assert!(matches!(extract(&long), Err(Error::Parse { .. })));
}

#[test]
fn huge_and_degenerate_files() {
    // 12 MB of open angle brackets: refused fast
    let d = vec![b'<'; 12_000_000];
    assert!(extract(&d).is_err());
    // over the tag budget (200 per allowed unit)
    let mut d = b"<v1><e9>\n".to_vec();
    d.extend(b"<B>".repeat(10_100_000));
    d.extend_from_slice(b"x\n");
    assert!(matches!(extract(&d), Err(Error::Limit(_))));
    // a million empty lines and a million one-letter paragraphs
    let d = b"\n".repeat(2_000_000);
    assert!(matches!(extract(&d), Err(Error::Limit(_))));
    let mut d = b"<v1><e9>\n".to_vec();
    d.extend(b"a\n".repeat(60_000));
    assert!(matches!(extract(&d), Err(Error::Limit(_))));
    // a 20 MB single line of text under the unit length limit is refused, not truncated
    let mut d = b"<v1><e9>\n@N:".to_vec();
    d.extend(b"a".repeat(200_000));
    assert!(matches!(extract(&d), Err(Error::Limit(_))));
    // oversized input
    let l = Limits { max_input_bytes: 100, ..Limits::default() };
    let o = ExtractOptions { limits: Some(l), ..Default::default() };
    assert!(matches!(Xtg.extract(&[b'a'; 101], &o), Err(Error::Limit(_))));
}

#[test]
fn a_foreign_skeleton_is_refused_not_panicked_on() {
    let mut doc = extract(b"<v1><e9>\n@N:Hi\n").unwrap();
    doc.skeleton = Skeleton::Text { parts: vec![SkelPart::Unit(7)], encoding: "utf-8".into(), parser: doc.skeleton.parser().cloned() };
    assert!(Xtg.merge(&doc).is_err());
    doc.skeleton = Skeleton::Text { parts: vec![], encoding: "klingon".into(), parser: doc.skeleton.parser().cloned() };
    assert!(Xtg.merge(&doc).is_err());
}
