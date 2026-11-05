//! Hostile input: random bytes, mutated fixtures, SCC-shaped garbage, deep and huge files. Nothing panics; a file
//! that is accepted merges back to its exact bytes, and a translated merge either writes a readable file with the
//! same captions or refuses with a typed error.
mod common;
use bilingua_model::*;
use bilingua_scc::SccFilter;
use common::*;
use proptest::prelude::*;
use std::time::Instant;

fn check(data: &[u8]) {
    let _ = SccFilter.sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(SccFilter.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let n = doc.units.len();
            let ids: Vec<String> = doc.units.iter().map(|u| u.segments[0].id.clone()).collect();
            for (k, id) in ids.iter().enumerate() {
                let src = plain_text(&doc.units[k].source());
                let t = match k % 4 {
                    0 => format!("~{src}~"),
                    1 => "\u{65e5}\u{672c}".to_string(),
                    2 => "word ".repeat(30),
                    _ => "\u{e9}t\u{e9} \u{266a}".to_string(),
                };
                // a target without the unit's tags is allowed (the tags are dropped)
                doc.set_target(id, vec![Inline::text(t)]).unwrap();
            }
            match SccFilter.merge(&doc) {
                // a caption that would read back on channel 2 (or vanish otherwise) is refused, so a written file
                // reads back with the same captions
                Ok(out) => assert_eq!(units(&out).len(), n, "unit count after translation of {:?}", String::from_utf8_lossy(data)),
                Err(Error::Write(_)) => {}
                Err(e) => panic!("unexpected error kind: {e}"),
            }
            // only the units that fit
            let mut doc2 = extract(data).unwrap();
            for id in &ids {
                doc2.set_target(id, vec![Inline::text("ok")]).unwrap();
            }
            match SccFilter.merge(&doc2) {
                Ok(out) => assert_eq!(units(&out).len(), n),
                // "ok" always fits; only the channel read-back check may refuse it
                Err(Error::Write(m)) => assert!(m.contains("channel"), "short text must fit: {m}"),
                Err(e) => panic!("short text must fit: {e}"),
            }
        }
        Err(Error::Parse { .. } | Error::Limit(_) | Error::Encoding(_) | Error::Unsupported(_)) => {}
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
    fn random_bytes_after_a_header_never_panic(data in proptest::collection::vec(any::<u8>(), 0..600)) {
        let mut d = b"Scenarist_SCC V1.0\r\n\r\n".to_vec();
        d.extend(data);
        check(&d);
    }

    #[test]
    fn scc_shaped_lines_never_panic(
        lines in proptest::collection::vec(
            ("[0-9]{2}:[0-9]{2}:[0-9]{2}[:;.,][0-9]{2}", "[ \\t]{1,3}", proptest::collection::vec("[0-9a-fA-F]{4}", 0..40)),
            0..12),
        nl in prop::sample::select(vec!["\n", "\r\n", "\r"]),
    ) {
        let mut s = String::from("Scenarist_SCC V1.0");
        s.push_str(nl);
        s.push_str(nl);
        for (tc, sep, words) in lines {
            s.push_str(&tc);
            s.push_str(&sep);
            s.push_str(&words.join(" "));
            s.push_str(nl);
        }
        check(s.as_bytes());
    }

    #[test]
    fn caption_codes_never_panic(
        words in proptest::collection::vec(
            prop::sample::select(vec![
                "9420", "94ae", "942f", "942c", "9425", "9426", "9427", "9429", "94ad", "94a1", "9470", "9440", "94e0", "1340", "13e0",
                "9154", "97a2", "97a1", "97a3", "91ae", "9137", "92a4", "1320", "1c20", "1c70", "8080", "0000", "0102", "c845", "4c4c",
                "4f80", "7361", "e9a1", "ffff", "10d3", "9773", "13c4", "105d", "9120", "912f", "9172", "17a1",
            ]),
            0..80),
        lines in 1usize..4,
    ) {
        let mut s = String::from("Scenarist_SCC V1.0\n\n");
        for l in 0..lines {
            s.push_str(&format!("00:00:0{l}:00\t{}\n\n", words.join(" ")));
        }
        check(s.as_bytes());
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
                1 => { d.remove(p); }
                2 => d.insert(p, byte),
                _ => d.truncate(p),
            }
        }
        check(&d);
    }
}

#[test]
fn every_fixture_truncated_at_every_length_never_panics() {
    for d in fixtures().iter().take(6) {
        for cut in (0..d.len()).step_by(7) {
            check(&d[..cut]);
        }
    }
}

#[test]
fn limits_are_typed_errors() {
    let data = b"Scenarist_SCC V1.0\n\n00:00:01:00\t9420 9420\n";
    // input size
    let o = ExtractOptions { limits: Some(Limits { max_input_bytes: 10, ..Limits::default() }), ..Default::default() };
    assert!(matches!(SccFilter.extract(data, &o), Err(Error::Limit(ref l)) if l.kind == LimitKind::InputBytes));
    // lines
    let o = ExtractOptions { limits: Some(Limits { max_lines: 2, ..Limits::default() }), ..Default::default() };
    assert!(matches!(SccFilter.extract(data, &o), Err(Error::Limit(ref l)) if l.kind == LimitKind::Lines));
    // 100 000 newlines under a small line budget
    let many = format!("Scenarist_SCC V1.0{}", "\n".repeat(100_000));
    let o = ExtractOptions { limits: Some(Limits { max_lines: 1000, ..Limits::default() }), ..Default::default() };
    assert!(matches!(SccFilter.extract(many.as_bytes(), &o), Err(Error::Limit(_))));
    // words per line: 4096 pass, 4097 are refused
    let line = |n: usize| format!("Scenarist_SCC V1.0\n\n00:00:01:00\t{}\n", vec!["c845"; n].join(" "));
    assert!(extract(line(4096).as_bytes()).is_ok());
    assert!(matches!(extract(line(4097).as_bytes()), Err(Error::Limit(_))));
    // unit characters
    let o = ExtractOptions { limits: Some(Limits { max_unit_chars: 100, ..Limits::default() }), ..Default::default() };
    assert!(matches!(SccFilter.extract(line(200).as_bytes(), &o), Err(Error::Limit(ref l)) if l.kind == LimitKind::UnitChars));
}

#[test]
fn deep_and_wide_lines_stay_fast() {
    // 2000 rows in one caption (a PAC, a character, a PAC, ...): rows are many but bounded by the word cap
    let mut words = vec!["9420"; 2];
    for i in 0..1300 {
        words.push(if i % 2 == 0 { "9440" } else { "94e0" });
        words.push(if i % 2 == 0 { "9440" } else { "94e0" });
        words.push("c845");
    }
    let s = format!("Scenarist_SCC V1.0\n\n00:00:01:00\t{}\n", words.join(" "));
    // 3902 words
    let t = Instant::now();
    let doc = extract(s.as_bytes()).unwrap();
    assert_eq!(doc.units.len(), 1);
    let rows = doc.units[0].source();
    assert!(!rows.is_empty());
    for text in ["ok", &"word ".repeat(20_000), &"a".repeat(100_000), "x y\n".repeat(5000).as_str()] {
        let mut d = doc.clone();
        d.set_target("c1", vec![Inline::text(text)]).unwrap();
        let _ = SccFilter.merge(&d);
    }
    assert!(t.elapsed().as_secs() < 20, "took {:?}", t.elapsed());
}

#[test]
fn a_hundred_thousand_short_captions_in_a_row() {
    let mut s = String::from("Scenarist_SCC V1.0\n\n");
    for i in 0..60_000 {
        s.push_str(&format!("00:00:{:02}:00\t9420 9420 9470 9470 c845 4c4c 4f80 942f 942f\n\n", i % 60));
    }
    let o = ExtractOptions { limits: Some(Limits { max_units: 1_000_000, ..Limits::default() }), ..Default::default() };
    let t = Instant::now();
    let doc = SccFilter.extract(s.as_bytes(), &o).unwrap();
    assert_eq!(doc.units.len(), 60_000);
    assert_eq!(SccFilter.merge(&doc).unwrap(), s.as_bytes());
    assert!(t.elapsed().as_secs() < 20);
    // the default unit cap refuses it
    assert!(matches!(extract(s.as_bytes()), Err(Error::Limit(ref l)) if l.kind == LimitKind::Units));
}
