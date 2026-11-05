//! Hostile input: random bytes, mutated fixtures, entity nesting and expansion attempts, deep and huge files.
//! Nothing panics; a file that is accepted merges back to its exact bytes.
mod common;
use bilingua_dtd::{DtdFilter, Limits};
use bilingua_model::*;
use common::*;
use proptest::prelude::*;
use std::time::{Duration, Instant};

fn check(data: &[u8]) {
    let _ = DtdFilter.sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(DtdFilter.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let ids: Vec<(String, Vec<Inline>)> = doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), u.segments[0].source.clone())).collect();
            let n = doc.units.len();
            for (id, src) in ids {
                let mut t = vec![Inline::text("~")];
                t.extend(src);
                t.push(Inline::text("~ & \" ' % <x>"));
                doc.set_target(&id, merge_text(t)).unwrap();
            }
            let out = DtdFilter.merge(&doc).unwrap();
            let back = extract(&out).unwrap_or_else(|e| panic!("translated output must read back: {e}\n{:?}\n{:?}", String::from_utf8_lossy(data), String::from_utf8_lossy(&out)));
            assert_eq!(back.units.len(), n, "unit count after translation {:?} -> {:?}", String::from_utf8_lossy(data), String::from_utf8_lossy(&out));
        }
        Err(Error::Parse { .. } | Error::Limit(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(corpus_dir()).unwrap().map(|e| std::fs::read(e.unwrap().path()).unwrap()).collect();
    v.push(std::fs::read(data_dir().join("Test01.dtd")).unwrap());
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
    fn dtd_shaped_text_never_panics(parts in proptest::collection::vec("(<!ENTITY|<!--|-->|<!|<\\?|\\?>|%[a-z]{0,3};?|&[a-z#x0-9]{0,5};?|\"|'|>|<|[ \\n\\r\\t]|[A-Za-z]{1,6}|SYSTEM|PUBLIC|\\[|\\]|\u{e9}|\u{65e5})", 0..80)) {
        check(parts.concat().as_bytes());
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

/// Runs `check` and requires it to be fast: a quadratic scanner would take minutes on these inputs.
fn timed(data: &[u8]) {
    let t = Instant::now();
    check(data);
    assert!(t.elapsed() < Duration::from_secs(60), "took {:?} for {} bytes", t.elapsed(), data.len());
}

#[test]
fn entity_nesting_and_expansion_attempts_are_never_expanded() {
    // billion laughs: ten levels of ten references, never expanded; the values are text
    let mut s = String::from("<!ENTITY lol0 \"w0 lol\">\n");
    for i in 1..=9 {
        s.push_str(&format!("<!ENTITY lol{i} \"w{i} {}\">\n", format!("&lol{};", i - 1).repeat(10)));
    }
    let doc = extract(s.as_bytes()).unwrap();
    assert_eq!(doc.units.len(), 10);
    assert_eq!(flat(&doc.units[9].segments[0].source), format!("w9 {}", "&lol8;".repeat(10)));
    assert_eq!(DtdFilter.merge(&doc).unwrap(), s.as_bytes());
    // a self-referencing entity, a quadratic blow-up and a very long reference chain
    check(b"<!ENTITY a \"&a;\">");
    let chain: String = (0..100_000).map(|i| format!("<!ENTITY e{i} \"&e{};\">\n", i + 1)).collect();
    let lim = Limits { max_units: 200_000, ..Limits::default() };
    assert!(DtdFilter.extract_with(chain.as_bytes(), &opts(), &lim).is_ok());
    // parameter entities that would declare entities, external entities, and remote references stay raw
    let pe = b"<!ENTITY % x \"<!ENTITY y 'z'>\">\n%x;\n<!ENTITY % e SYSTEM \"http://example.invalid/e.dtd\">\n%e;\n<!ENTITY ext SYSTEM \"file:///etc/passwd\">\n<!ENTITY a \"Real\">\n";
    assert_eq!(units(pe), vec!["Real"]);
    assert_eq!(DtdFilter.merge(&extract(pe).unwrap()).unwrap(), pe);
    // conditional sections and DOCTYPE-like nesting, very deep
    let deep = format!("{}{}", "<![INCLUDE[".repeat(100_000), "]]>".repeat(100_000));
    timed(deep.as_bytes());
    let deep = format!("<!ENTITY a \"{}x{}\">", "<b>".repeat(200_000 / 3), "</b>".repeat(200_000 / 4));
    timed(deep.as_bytes());
    let deep = format!("<!ENTITY a \"{}\">", "<a><b>".repeat(8000) + &"</c></d>".repeat(8000) + "x");
    timed(deep.as_bytes());
    // character references: huge numbers and invalid code points are kept as written
    let huge = format!("<!ENTITY a \"x &#{}; &#x{}; &#0; &#xD800; &#1114112;\">", "9".repeat(5000), "F".repeat(5000));
    let u = units(huge.as_bytes());
    assert_eq!(u.len(), 1);
    assert!(u[0].contains("&#0;") && u[0].contains("&#xD800;") && u[0].contains("&#1114112;"));
    assert_eq!(DtdFilter.merge(&extract(huge.as_bytes()).unwrap()).unwrap(), huge.as_bytes());
}

#[test]
fn deep_and_huge_inputs_give_typed_errors_or_stay_linear() {
    timed(format!("<!ENTITY a \"{}\">", "&".repeat(1_000_000)).as_bytes());
    timed(format!("<!ENTITY a \"{}\">", "&amp;".repeat(1_000_000)).as_bytes());
    timed(format!("<!ENTITY a \"{}\">", "<".repeat(1_000_000)).as_bytes());
    timed("<!--".repeat(250_000).as_bytes());
    timed("<!ENTITY ".repeat(100_000).as_bytes());
    timed("<!ELEMENT \"".repeat(100_000).as_bytes());
    timed("%a;".repeat(250_000).as_bytes());
    timed(format!("<!ENTITY a \"x\"{}>", " ".repeat(3_000_000)).as_bytes());
    // one value over the cap: refused, not truncated
    let big = format!("<!ENTITY a \"{}\">\n", "a".repeat(100_001));
    assert!(matches!(extract(big.as_bytes()), Err(Error::Limit(LimitError { kind: LimitKind::UnitChars, .. }))));
    // more entities than the unit cap: refused
    let many: String = (0..50_001).map(|i| format!("<!ENTITY k{i} \"v{i}\">\n")).collect();
    assert!(matches!(extract(many.as_bytes()), Err(Error::Limit(LimitError { kind: LimitKind::Units, .. }))));
    assert!(DtdFilter.extract_with(many.as_bytes(), &opts(), &Limits { max_units: 60_000, ..Limits::default() }).is_ok());
    // more declarations than 4 x the unit cap: refused before any scan
    let decls = "<!---->".repeat(200_001);
    assert!(matches!(extract(decls.as_bytes()), Err(Error::Limit(LimitError { kind: LimitKind::Units, .. }))));
    // more lines than the line cap
    assert!(matches!(extract("\n".repeat(1_000_001).as_bytes()), Err(Error::Limit(LimitError { kind: LimitKind::Lines, .. }))));
    // truncated multi-byte, truncated declarations at every cut of a real file
    let src = std::fs::read(corpus_dir().join("browser_ja.dtd")).unwrap();
    for cut in 0..src.len() {
        check(&src[..cut]);
    }
    // invalid UTF-8, NUL, UTF-16, UTF-32
    for bad in [&b"<!ENTITY a \"\xc3\">"[..], b"<!ENTITY a \"\0\">", b"\xff\xfe<\0!\0", b"\xff\xfe\0\0<\0\0\0"] {
        assert!(extract(bad).is_err());
    }
}
