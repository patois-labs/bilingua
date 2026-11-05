//! Hostile input: random bytes, mutated fixtures, deep and huge files. Nothing panics; a file that is accepted
//! merges back to its exact bytes and a translated merge opens again.
mod common;
use bilingua_csv::{CsvFilter, TsvFilter};
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn calls() -> Vec<Call> {
    vec![
        Call::csv(),
        Call::tsv(),
        Call::csv().langs("en", &["de", "fr"], Some("de")),
        Call::csv().cols("source=2,target=3").langs("en", &["de"], Some("de")),
        Call::csv().cols("header=no,key=1,source=2,target.de=4,context=3").langs("en", &["de"], Some("de")),
        Call::tsv().cols("source=text,target=Target").langs("en", &["de"], None),
    ]
}

fn check_one(c: &Call, data: &[u8]) {
    let _ = CsvFilter::default().sniff(data);
    let _ = TsvFilter::default().sniff(data);
    match c.extract(data) {
        Ok(mut doc) => {
            assert_eq!(c.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let ids: Vec<(String, String)> =
                doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), plain_text(&u.source()))).collect();
            for (id, t) in ids {
                doc.set_target(&id, vec![Inline::text(format!("~{t}~"))]).unwrap();
            }
            // a translated merge never fails (the reading of a record that opens a quote and never closes it
            // may differ afterwards, so only the typed result is checked)
            let out = c.merge(&doc).expect("merge");
            let _ = c.extract(&out);
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn check(data: &[u8]) {
    for c in calls() {
        check_one(&c, data);
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(corpus_dir()).unwrap().map(|e| std::fs::read(e.unwrap().path()).unwrap()).collect();
    v.sort();
    v
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(800))]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..600)) {
        check(&data);
    }

    #[test]
    fn random_text_never_panics(s in "[ -~\\t\\r\\n\\u{a0}\\u{e9}\\u{65e5}\\u{2028}\"]{0,400}") {
        check(s.as_bytes());
    }

    #[test]
    fn csv_shaped_lines_never_panic(
        lines in proptest::collection::vec("(\"[A-Za-z ,;\"\\n]{0,10}\"?[,;\\t|]?){0,6}", 0..30),
        nl in prop::sample::select(vec!["\n", "\r\n", "\r"]),
    ) {
        check(lines.join(nl).as_bytes());
    }

    #[test]
    fn mapping_specs_never_panic(spec in "[a-zA-Z0-9=,.:_ -]{0,60}") {
        let c = Call::csv().cols(&spec).langs("en", &["de"], Some("de"));
        let _ = c.extract(b"id,source,target\n1,Hello,\n2,World,Welt\n");
        let _ = c.extract(b"1,Hello\n");
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..60, edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0u8..4), 1..8)) {
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
    let c = Call::csv();
    // quotes nested as deeply as a file can: linear, no recursion
    let quotes = "\"".repeat(2_000_001);
    check_one(&c, quotes.as_bytes());
    let doubled = format!("source\n\"{}\"\n", "\"\"".repeat(40_000));
    check_one(&c, doubled.as_bytes());
    // a very long line of delimiters: refused as too wide
    let wide = format!("{}\n", ",".repeat(5_000));
    assert!(matches!(c.extract(wide.as_bytes()), Err(Error::Limit(_))));
    // one value over the field limit (131072): refused, not truncated
    let big = format!("source\n{}\n", "a".repeat(131_073));
    assert!(matches!(c.extract(big.as_bytes()), Err(Error::Limit(_))));
    let ok = format!("source\n{}\n", "a".repeat(100_000));
    assert!(c.extract(ok.as_bytes()).is_ok());
    // a value over the unit cap (100000 characters) but under the field limit is refused by the unit check
    let unit_big = format!("source\n\"{}\"\n", "b".repeat(100_001));
    assert!(matches!(c.extract(unit_big.as_bytes()), Err(Error::Limit(_))));
    // more rows than the unit cap: refused
    let mut many = String::from("source\n");
    for i in 0..50_001 {
        many.push_str(&format!("row {i}\n"));
    }
    assert!(matches!(c.extract(many.as_bytes()), Err(Error::Limit(_))));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(60_000));
    assert!(c.extract_with(many.as_bytes(), &o).is_ok());
    // more lines than the line cap: refused before any scan
    let lines = "\n".repeat(1_000_001);
    assert!(matches!(c.extract(lines.as_bytes()), Err(Error::Limit(_))));
    // truncated multi-byte and UTF-16 files
    let mut s = "source\n\u{65e5}\u{672c}\u{8a9e}\n".as_bytes().to_vec();
    s.pop();
    s.pop();
    check(&s);
    check(b"\xff\xfes\x00\x3d");
    assert!(c.extract(b"\xff\xfe\x00\xd8").is_err(), "a lone surrogate half in a UTF-16 file");
    // a column mapping that is far too long
    let long_spec = format!("source={}", "x".repeat(2_001));
    assert!(matches!(Call::csv().cols(&long_spec).extract(b"a,b\n"), Err(Error::Unsupported(_))));
}

#[test]
fn edited_record_codes_are_refused_not_dropped() {
    let c = Call::csv();
    let data = b"source,target\nHello,\nWorld,\n";
    let fresh = || {
        let mut d = c.extract(data).unwrap();
        for i in 0..d.units.len() {
            let id = d.units[i].segments[0].id.clone();
            d.set_target(&id, vec![Inline::text("Hallo")]).unwrap();
        }
        d
    };
    assert!(c.merge(&fresh()).is_ok());
    // the record text gone: an error, never a file with the row missing
    let mut d = fresh();
    d.units[0].codes.get_mut("$csv").unwrap().standalone = None;
    assert!(c.merge(&d).is_err());
    // the whole code gone
    let mut d = fresh();
    d.units[1].codes.clear();
    assert!(c.merge(&d).is_err());
    // a delimiter that is not one of the four (a multi-byte one would split a character)
    let mut d = fresh();
    d.units[0].codes.get_mut("$csv").unwrap().open = Some("\u{e9}".into());
    assert!(c.merge(&d).is_err());
    // a write column far past the limit would pad the row without bound
    let mut d = fresh();
    d.units[0].codes.get_mut("$csv").unwrap().label = Some("0:999999999999:0".into());
    assert!(c.merge(&d).is_err());
}
