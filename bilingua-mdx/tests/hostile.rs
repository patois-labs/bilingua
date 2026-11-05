//! Hostile input: malformed, truncated, deep and huge files give typed errors or an exact round trip, never a
//! panic and never a long stall; a property test over random bytes, random MDX-looking text and mutated fixtures.
mod common;
use bilingua_mdx::Mdx;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;
use std::time::{Duration, Instant};

/// Reads or refuses; when it reads, an untouched merge is the original and a pseudo translation merges.
fn check(data: &[u8]) -> Result<Document, Error> {
    match extract(data) {
        Ok(doc) => {
            assert_eq!(Mdx.merge(&doc).unwrap(), data, "untouched merge");
            let mut d2 = doc.clone();
            for u in &mut d2.units {
                u.segments[0].target = vec![Inline::text("{x} <y> `z` \u{ab}T\u{bb}\n- 1. #")];
                u.segments[0].state = SegState::Draft;
            }
            let out = Mdx.merge(&d2).unwrap();
            let _ = extract(&out);
            Ok(doc)
        }
        Err(e) => {
            // a typed error, with a message a person can read
            let msg = e.to_string();
            assert!(!msg.is_empty());
            Err(e)
        }
    }
}

#[test]
fn truncated_at_every_byte() {
    let src = guide();
    for n in 0..=src.len() {
        let _ = check(&src[..n]);
    }
    let crlf = String::from_utf8(src).unwrap().replace('\n', "\r\n").into_bytes();
    for n in (0..=crlf.len()).step_by(7) {
        let _ = check(&crlf[..n]);
    }
}

fn timed(name: &str, data: &[u8]) {
    let t = Instant::now();
    let _ = check(data);
    let el = t.elapsed();
    if el > Duration::from_secs(1) {
        eprintln!("slow: {name}: {el:?} for {} bytes", data.len());
    }
    assert!(el < Duration::from_secs(60), "{name}: {el:?} for {} bytes", data.len());
}

#[test]
fn pathological_shapes_finish() {
    let n = 40_000;
    let shapes: Vec<(&str, String)> = vec![
        ("open braces", "{".repeat(n)),
        ("open braces by line", "x {\n".repeat(n)),
        ("open tags", "<a ".repeat(n)),
        ("open tags by line", "<A b={\n".repeat(n)),
        ("nested tags", "<A>".repeat(n)),
        ("nested braces closed", format!("{}{}", "{".repeat(n), "}".repeat(n))),
        ("tag then brace", "<A b={".repeat(n)),
        ("backticks", "`".repeat(n)),
        ("backtick runs", "`` ` ``` ".repeat(n / 4)),
        ("images", "![".repeat(n)),
        ("images 2", "![a](b ".repeat(n / 4)),
        ("ref defs", "[a]: b \"\n".repeat(n / 4)),
        ("ref defs 2", "[a]:\n".repeat(n)),
        ("comments", "<!--".repeat(n)),
        ("comment lines", "<!-- x -->\ntext <!--\n".repeat(n / 4)),
        ("esm", "import x from 'y'\n".repeat(n / 4)),
        ("rules", "---\n".repeat(n)),
        ("front matter", format!("---\n{}---\n", "a: b\n".repeat(n))),
        ("yaml nesting", format!("---\na:\n{}\n---\n", "  - b: c\n".repeat(n / 4))),
        ("yaml anchors", format!("---\n{}---\n", "a: &x v\n".repeat(n / 4))),
        ("toml", format!("+++\n{}+++\n", "a = 'b'\n".repeat(n / 4))),
        ("bullets", "- ".repeat(n)),
        ("quotes", "> ".repeat(n)),
        ("hashes", "#".repeat(n)),
        ("backslashes", "\\".repeat(n)),
        ("strings in braces", "{'".repeat(n)),
        ("comments in braces", "{/*".repeat(n)),
        ("quote in tag", "<A b=\"".repeat(n)),
        ("tabs", "\t".repeat(n)),
        ("footnotes", "[^a]: x\n".repeat(n / 4)),
        ("emphasis", "*_*_".repeat(n)),
        ("autolinks", "https://a.b ".repeat(n / 4)),
        ("pipes", "| a \\| b ".repeat(n / 4)),
        ("lone cr", "a\r".repeat(n)),
        ("unicode lines", "a\u{2028}b\u{85}c\x0b".repeat(n / 4)),
        ("fence", "```\n".repeat(n / 2)),
        ("one huge line", "word ".repeat(n)),
        ("braces and tags", "{<A>{<B>".repeat(n / 4)),
    ];
    for (name, s) in &shapes {
        timed(name, s.as_bytes());
    }
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    let d = 200_000;
    let s = format!("{}{}\n", "{".repeat(d), "}".repeat(d));
    timed("deep braces", s.as_bytes());
    let s = format!("{}x{}\n", "<A>".repeat(d / 3), "</A>".repeat(d / 3));
    timed("deep tags", s.as_bytes());
    let s = format!("---\n{}\n---\nbody\n", "[".repeat(d));
    timed("deep yaml", s.as_bytes());
    let s = format!("+++\na = {}\n+++\nbody\n", "[".repeat(d));
    timed("deep toml", s.as_bytes());
}

#[test]
fn limits_give_typed_errors() {
    let mut o = opts();
    o.limits = Some(Limits { max_input_bytes: 100, ..Limits::default() });
    assert!(matches!(Mdx.extract(&[b'a'; 101], &o), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    let mut o = opts();
    o.limits = Some(Limits { max_lines: 10, ..Limits::default() });
    assert!(matches!(Mdx.extract("a\n".repeat(20).as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Lines));
    let mut o = opts();
    o.limits = Some(Limits { max_units: 3, ..Limits::default() });
    let many = "<A title=\"t\" />\n".repeat(4);
    assert!(matches!(Mdx.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let mut o = opts();
    o.limits = Some(Limits { max_unit_chars: 10, ..Limits::default() });
    assert!(matches!(Mdx.extract(b"a very long line of words\n", &o), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
    // binary and unreadable encodings
    let bin: Vec<u8> = [0u8, 1, 2, 0xff].iter().copied().cycle().take(200).collect();
    assert!(matches!(extract(&bin), Err(Error::Encoding(_))));
    let _ = check(&[0xFF, 0xFE, 0x41]); // a half UTF-16 unit: read or refused, never a panic
    // a skeleton from another crate is refused by merge
    let mut doc = extract(b"Hello\n").unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    }
    assert!(matches!(Mdx.merge(&doc), Err(Error::ParserVersion { .. })));
    // a zip skeleton is not ours
    doc.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(Mdx.merge(&doc).is_err());
}

#[test]
fn a_unicode_space_in_a_tag_is_a_separator() {
    // LocOS raises an AttributeError there; we read the space as a separator and keep the bytes
    let src = "<a\u{a0}b title=\"Hi\">text</a> and more\n";
    let doc = check(src.as_bytes()).unwrap();
    assert!(doc.units.iter().any(|u| locos_text(u) == "Hi"));
}

#[test]
fn quadratic_cases_stay_linear_enough() {
    // the same shape at 1x and 4x: the larger run must not take more than ~40x
    for shape in ["x {\n", "<A b={\n", "`` ` ``` ", "<!-- x -->\ntext <!--\n", "[a]:\n"] {
        let small = shape.repeat(2_000);
        let big = shape.repeat(8_000);
        let t1 = Instant::now();
        let _ = extract(small.as_bytes());
        let a = t1.elapsed().max(Duration::from_millis(5));
        let t2 = Instant::now();
        let _ = extract(big.as_bytes());
        let b = t2.elapsed();
        assert!(b < a * 40 + Duration::from_millis(500), "{shape:?}: {a:?} -> {b:?}");
    }
}

fn mdx_alphabet() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just("{"), Just("}"), Just("<"), Just(">"), Just("</"), Just("/>"), Just("\""), Just("'"), Just("`"), Just("\\"),
            Just("\n"), Just("\r\n"), Just(" "), Just("- "), Just("# "), Just("> "), Just("| "), Just("---\n"), Just("+++\n"),
            Just("import "), Just("export "), Just("title"), Just(": "), Just("="), Just("["), Just("]"), Just("("), Just(")"),
            Just("!"), Just("*"), Just("_"), Just("&"), Just("<A"), Just("</A>"), Just("<!--"), Just("-->"), Just("```"),
            Just("[a]: "), Just("[^a]: "), Just("word "), Just("\u{65e5}\u{672c}"), Just("\u{2028}"), Just("\x0b"), Just("\u{a0}"),
            Just("//"), Just("/*"), Just("*/"), Just("${"), Just("1. "), Just("\t"),
        ],
        0..120,
    )
    .prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, max_shrink_iters: 200, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..1500)) {
        let _ = check(&data);
    }

    #[test]
    fn random_mdx_text_round_trips(s in mdx_alphabet()) {
        let _ = check(s.as_bytes());
    }

    #[test]
    fn mutated_fixture_never_panics(edits in proptest::collection::vec((0usize..1000, 0u8..4, any::<u8>()), 1..12)) {
        let mut data = guide();
        for (pos, kind, byte) in edits {
            let at = pos % (data.len() + 1);
            match kind {
                0 if at < data.len() => data[at] = byte,
                1 => data.insert(at, byte),
                2 if at < data.len() => { data.remove(at); }
                _ => { let end = (at + 1 + byte as usize % 40).min(data.len()); data.drain(at.min(end)..end); }
            }
        }
        let _ = check(&data);
    }

    #[test]
    fn any_target_text_merges(s in mdx_alphabet(), t in mdx_alphabet()) {
        if let Ok(mut doc) = extract(s.as_bytes()) {
            for u in &mut doc.units {
                u.segments[0].target = vec![Inline::text(t.clone())];
                u.segments[0].state = SegState::Draft;
            }
            let out = Mdx.merge(&doc);
            prop_assert!(out.is_ok());
        }
    }
}
