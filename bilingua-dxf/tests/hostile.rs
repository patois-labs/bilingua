//! Hostile input: random bytes, DXF-shaped token soup, mutated fixtures, every truncation, deep and huge files.
//! Nothing panics; a file that is accepted merges back to its exact bytes and takes a translation.
mod common;
use bilingua_dxf::Dxf;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = Dxf.sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(Dxf.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let ids: Vec<(String, String)> =
                doc.units.iter().map(|u| (u.segments[0].id.clone(), plain_text(&u.source()))).collect();
            doc.target_lang = Some("de-DE".into());
            for (id, t) in ids {
                doc.set_target(&id, vec![Inline::text(format!("~{t}~"))]).unwrap();
            }
            let out = Dxf.merge(&doc).unwrap_or_else(|e| panic!("translated merge failed: {e}"));
            // the output is itself a drawing this crate can read
            extract(&out).unwrap_or_else(|e| panic!("merged output unreadable: {e}"));
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = corpus().into_iter().map(|c| c.1).collect();
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
    fn mutated_fixtures_never_panic(idx in 0usize..64, edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0u8..4), 1..8)) {
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
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn dxf_shaped_soup_never_panics(words in proptest::collection::vec(
        "(0|1|3|9|2|8|101|304|999|1001|-1|SECTION|ENDSEC|EOF|HEADER|ENTITIES|BLOCKS|TABLES|TEXT|MTEXT|ATTRIB|ATTDEF|DIMENSION|MULTILEADER|MLEADER|\\$ACADVER|\\$DWGCODEPAGE|AC10[0-9]{2}|ANSI_[0-9]{3,4}|DOS[0-9]{3}|UTF8|[A-Za-z %\\\\{}\\^<>\\[\\]~;|0-9]{0,14}|\\\\U\\+[0-9A-Fa-f]{0,5}|\\\\M\\+[0-9A-Fa-f]{0,6}|%%[a-z0-9]{0,3}| *|\\x{e9}|\\x{65e5})", 0..160),
        nl in prop::sample::select(vec!["\n", "\r\n", "\r"])) {
        check(words.join(nl).as_bytes());
    }
}

#[test]
fn every_truncation_of_the_fixtures_is_safe() {
    for d in fixtures() {
        // every cut point of the small ones, a stride through the rest
        let step = if d.len() > 3000 { d.len() / 400 } else { 1 };
        let mut i = 0;
        while i <= d.len() {
            check(&d[..i]);
            i += step.max(1);
        }
    }
}

fn frame(body: &str) -> Vec<u8> {
    format!("  0\nSECTION\n  2\nENTITIES\n{body}  0\nENDSEC\n  0\nEOF\n").into_bytes()
}

#[test]
fn deep_nesting_and_long_runs_are_linear() {
    let t0 = std::time::Instant::now();
    check(&frame(&format!("  0\nMTEXT\n  1\n{}x{}\n", "{".repeat(200_000), "}".repeat(200_000))));
    check(&frame(&format!("  0\nMTEXT\n  1\n{}\n", "{\\f".repeat(100_000))));
    check(&frame(&format!("  0\nMTEXT\n  1\nx{}\n", "\\S".repeat(100_000))));
    check(&frame(&format!("  0\nMTEXT\n  1\nx{}\n", "\\U+D83D".repeat(50_000))));
    check(&frame(&format!("  0\nTEXT\n  1\nx{}\n", "%%".repeat(200_000))));
    check(&frame(&format!("  0\nTEXT\n  1\nx{}\n", "^".repeat(200_000))));
    check(&frame(&format!("  0\nTEXT\n  1\nx{}\n", "\\".repeat(200_000))));
    check(&frame(&format!("  0\nDIMENSION\n  1\nx{}\n", "<>[]".repeat(50_000))));
    check(&frame(&"  0\nMTEXT\n  3\nabc\n".repeat(20_000)));
    check(&"  0\n".repeat(100_000).into_bytes());
    check(&"0\n\n".repeat(100_000).into_bytes());
    check(&frame(&format!("  0\nMTEXT\n  1\n{}\n", "a".repeat(5_000_000))));
    assert!(t0.elapsed().as_secs() < 60, "took {:?}", t0.elapsed());
}

#[test]
fn limits_are_typed_errors() {
    let mut many = String::new();
    for i in 0..200 {
        many.push_str(&format!("  0\nTEXT\n  1\nText {i}\n"));
    }
    let data = frame(&many);
    let o = ExtractOptions { limits: Some(Limits { max_units: 100, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Dxf.extract(&data, &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let o = ExtractOptions { limits: Some(Limits { max_input_bytes: 50, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Dxf.extract(&data, &o), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    let o = ExtractOptions { limits: Some(Limits { max_unit_chars: 10, ..Limits::default() }), ..Default::default() };
    assert!(matches!(
        Dxf.extract(&frame("  0\nTEXT\n  1\na very long string indeed\n"), &o),
        Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars
    ));
    let o = ExtractOptions { limits: Some(Limits { max_units: 2, ..Limits::default() }), ..Default::default() };
    let flood = format!("  0\n{}", "999\nx\n".repeat(1000));
    assert!(matches!(Dxf.extract(flood.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Lines));
}

#[test]
fn binary_dxf_and_encoding_errors_are_typed_and_say_why() {
    match Dxf.extract(b"AutoCAD Binary DXF\r\n\x1a\x00", &opts()) {
        Err(Error::Unsupported(m)) => assert!(m.contains("SAVEAS"), "{m}"),
        other => panic!("{other:?}"),
    }
    // a byte the declared code page rejects, and one that is not UTF-8, name the byte and the place
    let mut h = header("AC1015", "ANSI_1253");
    h.extend(p(&[(0, "SECTION"), (2, "ENTITIES"), (0, "TEXT"), (1, "ab"), (0, "ENDSEC"), (0, "EOF")]));
    let mut d = dxf(&h, "\n", utf8);
    let at = d.windows(2).position(|w| w == b"ab").unwrap();
    d[at] = 0xAA; // undefined in Windows-1253
    match extract(&d) {
        Err(Error::Encoding(m)) => assert!(m.contains("0xaa") && m.contains("cp1253"), "{m}"),
        other => panic!("{other:?}"),
    }
}
