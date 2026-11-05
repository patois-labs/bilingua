//! Hostile input: random bytes, subtitle-shaped token soup, mutated fixtures, every truncation, deep and huge files.
//! Nothing panics; a file that is accepted merges back to its exact bytes, and a translated merge reads back (or is a
//! typed refusal). Then the structural attacks: a translation may not become a cue, a timecode, a control code, a
//! header line or a frame-rate line.
mod common;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

const FMTS: [&str; 2] = ["sbv", "sub"];

fn check(fmt: &str, data: &[u8]) {
    let _ = filter(fmt).sniff(data);
    match extract(fmt, data) {
        Ok(mut doc) => {
            assert_eq!(filter(fmt).merge(&doc).unwrap(), data, "untouched merge must be exact");
            let n = doc.units.len();
            for u in &mut doc.units {
                let mut t = vec![Inline::text("Zq ")];
                t.extend(u.source());
                u.segments[0].target = merge_text(t);
            }
            match filter(fmt).merge(&doc) {
                Ok(out) => {
                    let back = extract(fmt, &out).unwrap_or_else(|e| panic!("merged output unreadable: {e}"));
                    // a unit of a source that was already odd (a text line that reads as a timecode) is refused, not
                    // rewritten; everything written reads back with as many units
                    assert_eq!(back.units.len(), n, "unit count after a translated merge");
                }
                Err(Error::Write(_)) => {}
                Err(e) => panic!("unexpected merge error: {e}"),
            }
        }
        Err(Error::Parse { .. } | Error::Limit(_) | Error::Encoding(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn check_all(data: &[u8]) {
    for f in FMTS {
        check(f, data);
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            p.extension().is_some_and(|x| x == "sbv" || x == "sub").then(|| std::fs::read(p).unwrap())
        })
        .collect();
    v.sort();
    v
}

const SNIPPETS: &[&str] = &[
    "0:00:01.000,0:00:02.000", "00:00:01.00,00:00:04.00", "{1}{1}25", "{25}{50}", "{0}{}", "{}{}", "{y:i}", "{Y:b}", "{c:$0000FF}", "{P:1,2}", "{y:", "{", "}", "{{", "}}", "|", " | ", "[br]", "[BR]",
    "[INFORMATION]", "[END INFORMATION]", "[SUBTITLE]", "[00:00:01]", "{DEFAULT}", "word", "Caf\u{e9}", "\u{65e5}\u{672c}", "e\u{301}", "\u{3000}", "\u{a0}", "\u{661}:\u{660}\u{660}", "\n", "\n", "\n\n", "\r\n", "\r",
    " ", "\t", "  ", "%s", "{0}", "\u{27e6}x1\u{27e7}", "\u{27e6}", "\u{27e7}", ".", ",", ":", "9", "0:", "\u{feff}",
];

fn soup() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(prop::sample::select(SNIPPETS), 0..120).prop_map(|v| v.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..400)) {
        check_all(&data);
    }

    #[test]
    fn subtitle_shaped_soup_never_panics(data in soup()) {
        check_all(&data);
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
        check_all(&d);
    }

    /// Any translation text, any unit: a typed refusal or a file that reads back with the same units.
    #[test]
    fn any_translation_is_written_safely_or_refused(text in prop::collection::vec(prop::sample::select(SNIPPETS), 0..12).prop_map(|v| v.concat()), pick in 0usize..3) {
        let fx = fixtures();
        let data = &fx[pick % fx.len()];
        for f in FMTS {
            if let Ok(mut doc) = extract(f, data) {
                let n = doc.units.len();
                for u in &mut doc.units {
                    u.segments[0].target = vec![Inline::text(text.clone())];
                }
                if let Ok(out) = filter(f).merge(&doc) {
                    let back = extract(f, &out).unwrap_or_else(|e| panic!("{f}: output unreadable for {text:?}: {e}"));
                    prop_assert_eq!(back.units.len(), n, "{} {:?}", f, text);
                }
            }
        }
    }
}

#[test]
fn every_truncation_of_every_fixture() {
    for d in fixtures() {
        for n in 0..=d.len() {
            check_all(&d[..n]);
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
            check_all(a.concat().as_bytes());
            let mut b = lines.clone();
            b.insert(i, lines[i]);
            check_all(b.concat().as_bytes());
        }
    }
}

#[test]
fn deep_and_huge_inputs() {
    check_all("{".repeat(1_000_000).as_bytes());
    check_all("{y:".repeat(300_000).as_bytes());
    check_all(&format!("{{1}}{{2}}{}\n", "{y:i}".repeat(100_000)).into_bytes());
    check_all(&format!("{{1}}{{2}}{}\n", "a|".repeat(500_000)).into_bytes());
    check_all(&format!("{{1}}{{2}}{{y:{}}}x\n", "z".repeat(1_000_000)).into_bytes());
    check_all(&format!("{{1}}{{2}}{}x\n", "{y:i}{c:$1}".repeat(40_000)).into_bytes());
    check_all("x\n".repeat(200_000).as_bytes());
    check_all("\n".repeat(500_000).as_bytes());
    check_all("0:00:01.000,0:00:02.000\nx\n\n".repeat(30_000).as_bytes());
    check_all("0:00:01.000,0:00:02.000\n".repeat(100_000).as_bytes());
    check_all(&format!("0:00:01.000,0:00:02.000\n{}\n", "word ".repeat(300_000)).into_bytes());
    check_all(&format!("[INFORMATION]\n00:00:01.00,00:00:02.00\n{}\n", "a[br]".repeat(200_000)).into_bytes());
    check_all("{1}{2}x\n".repeat(60_000).as_bytes());
    check_all(&"9".repeat(1_000_000).into_bytes());
    // gate: many pairs on one line (nested, so a translated merge checks every pair) and many runs of codes
    // separated by spaces before the words were quadratic in extract and merge
    check_all(&format!("{{1}}{{2}}x{}\n", "{y:i}a".repeat(15_000)).into_bytes());
    check_all(&format!("{{1}}{{2}}x {}word\n", "{y:i} ".repeat(15_000)).into_bytes());
}

/// Gate: every line break inside open pairs writes their codes again (LocOS), so a translation that opens many
/// pairs and then breaks many lines would multiply the cue; it is refused instead.
#[test]
fn reopened_pairs_cannot_multiply_a_cue() {
    let body: String = "{y:i}w|".repeat(2_000);
    let src = format!("{{1}}{{2}}{body}end\n");
    let mut doc = extract("sub", src.as_bytes()).unwrap();
    let s = doc.units[0].source();
    let opens: Vec<Inline> = s.iter().filter(|i| matches!(i, Inline::Tag { kind: TagKind::Open, .. })).cloned().collect();
    let closes: Vec<Inline> = s.iter().filter(|i| matches!(i, Inline::Tag { kind: TagKind::Close, .. })).cloned().collect();
    assert_eq!(opens.len(), 2_000);
    let mut t = opens.clone();
    t.push(Inline::text(format!("w{}", "\nw".repeat(2_000))));
    t.extend(closes.into_iter().rev());
    doc.units[0].segments[0].target = t;
    assert!(matches!(filter("sub").merge(&doc), Err(Error::Write(m)) if m.contains("too many lines")));
    // a few pairs over a few lines are written as LocOS writes them
    let out = build("sub", b"{1}{2}{y:i}a {c:$FF}b c|d\n", &[(0, "\u{27e6}g1\u{27e7}A\nB \u{27e6}g2\u{27e7}C\nD\u{27e6}/g2\u{27e7}\u{27e6}/g1\u{27e7}|E")]);
    assert_eq!(String::from_utf8(out).unwrap(), "{1}{2}{y:i}A|{y:i}B {c:$FF}C|{y:i}{c:$FF}D|E\n");
}

/// Gate: "words before this pair" reads the line as LocOS does, tokens included: a literal half token in the text
/// is not completed by the text after a code.
#[test]
fn a_literal_half_token_is_words_as_in_locos() {
    let doc = extract("sub", "{1}{2}a|\u{27e6}x{P:1}1\u{27e7}{y:i}w\n".as_bytes()).unwrap();
    assert!(!doc.units[0].codes.contains_key("~heads"), "{:?}", doc.units[0].codes);
    assert_eq!(units("sub", "{1}{2}a|\u{27e6}x{P:1}1\u{27e7}{y:i}w\n".as_bytes()), vec!["a\n\u{27e6}x\u{27e6}x1\u{27e7}1\u{27e7}\u{27e6}g2\u{27e7}w\u{27e6}/g2\u{27e7}"]);
}

#[test]
fn limits_are_typed_errors() {
    let many: Vec<u8> = (0..100).flat_map(|i| format!("{{{i}}}{{{}}}Cue number {i}.\n", i + 1).into_bytes()).collect();
    let o = |l: Limits| ExtractOptions { limits: Some(l), ..opts() };
    let f = filter("sub");
    assert!(matches!(f.extract(&many, &o(Limits { max_units: 50, ..Limits::default() })), Err(Error::Limit(_))));
    assert!(matches!(f.extract(&many, &o(Limits { max_input_bytes: 100, ..Limits::default() })), Err(Error::Limit(_))));
    assert!(matches!(f.extract(&many, &o(Limits { max_lines: 10, ..Limits::default() })), Err(Error::Limit(_))));
    assert!(matches!(f.extract(&many, &o(Limits { max_unit_chars: 5, ..Limits::default() })), Err(Error::Limit(_))));
}

#[test]
fn a_foreign_skeleton_is_refused() {
    let mut doc = extract("sbv", b"0:00:01.000,0:00:02.000\nHi\n").unwrap();
    doc.skeleton = Skeleton::Text { parts: vec![SkelPart::Unit(7)], encoding: "utf-8".into(), parser: Some(ParserInfo::new("bilingua-other", "0.1.0", 1)) };
    assert!(matches!(filter("sbv").merge(&doc), Err(Error::ParserVersion { .. })));
    doc.skeleton = Skeleton::Text { parts: vec![SkelPart::Unit(7)], encoding: "utf-8".into(), parser: None };
    assert!(matches!(filter("sbv").merge(&doc), Err(Error::Write(_))));
    doc.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(matches!(filter("sbv").merge(&doc), Err(Error::Write(_))));
}

// ---------------------------------------------------------------------------------------- structural attacks

fn translate(fmt: &str, data: &[u8], idx: usize, text: &str) -> Result<Vec<u8>, Error> {
    let mut doc = extract(fmt, data)?;
    doc.units[idx].segments[0].target = vec![Inline::text(text)];
    filter(fmt).merge(&doc)
}

const SBV: &[u8] = b"0:00:01.000,0:00:02.000\nOne\n\n0:00:03.000,0:00:04.000\nTwo\n";
const SV2: &[u8] = b"[INFORMATION]\n[TITLE]t\n[END INFORMATION]\n00:00:01.00,00:00:02.00\nOne[br]x\n\n00:00:03.00,00:00:04.00\nTwo\n";
const MDVD: &[u8] = b"{1}{1}25\n{0}{1}Hello\n{25}{50}{y:i}Hello|There\n{60}{90}Plain\n";

#[test]
fn a_translation_cannot_make_a_new_cue() {
    for (fmt, data) in [("sbv", SBV), ("sub", SV2), ("sub", MDVD)] {
        let n = extract(fmt, data).unwrap().units.len();
        for text in [
            "A\n\n0:00:09.000,0:00:09.500\nB",
            "A\r\n\r\n{70}{80}B",
            "A\n{70}{80}B\n00:00:09.00,00:00:09.50\nC",
            "A\n\u{3000}\nB",
            "A\n \nB",
            "A\u{2028}B",
            "{70}{80}B",
            "A\n0:00:09.000,0:00:09.500",
            "0:00:09.000,0:00:09.500",
        ] {
            if let Ok(out) = translate(fmt, data, 0, text) {
                let back = extract(fmt, &out).unwrap_or_else(|e| panic!("{fmt} {text:?}: {e}"));
                assert_eq!(back.units.len(), n, "{fmt} {text:?} made or lost a cue: {:?}", String::from_utf8_lossy(&out));
                // the other cues are untouched
                assert_eq!(back.units.last().unwrap().source(), extract(fmt, data).unwrap().units.last().unwrap().source(), "{fmt} {text:?}");
            }
        }
    }
}

#[test]
fn a_translation_that_is_a_timecode_line_is_refused() {
    for (fmt, data) in [("sbv", SBV), ("sub", SV2)] {
        for text in ["0:00:09.000,0:00:09.500", "A\n0:00:09.000,0:00:09.500", " \t0:00:09.000 , 0:00:09.500 \nB", "\u{661}:\u{660}\u{660}:\u{660}\u{660}.\u{660}\u{660}\u{660},0:00:01.000"] {
            // a joiner that is a real newline (SBV, SubViewer without [br]) must refuse; [br] makes one line
            let r = translate(fmt, data, 1, text);
            match r {
                Err(Error::Write(m)) => assert!(m.contains("timecode"), "{m}"),
                Ok(out) => assert_eq!(extract(fmt, &out).unwrap().units.len(), 2, "{fmt} {text:?}"),
                Err(e) => panic!("{e}"),
            }
        }
    }
    assert!(matches!(translate("sbv", SBV, 0, "0:00:09.000,0:00:09.500"), Err(Error::Write(_))));
}

#[test]
fn a_translation_cannot_make_a_control_code() {
    for text in ["{y:i}", "Hi {y:i} there", "{c:$0000FF}x", "x{P:1,2}", "{Y:b}"] {
        assert!(matches!(translate("sub", MDVD, 2, text), Err(Error::Write(m)) if m.contains("control code")), "{text:?}");
    }
    // pieces that join into a code once a token is dropped
    let src: &[u8] = b"{1}{2}{y:}a{Y:b}{y:i}b|c\n";
    let doc = extract("sub", src).unwrap();
    assert!(!doc.units.is_empty());
    // plain braces, placeholders and half codes are text
    for text in ["{0} and %s", "{ y:i }", "{y:i", "y:i}", "{1}{2}", "{}"] {
        let out = translate("sub", MDVD, 2, text).unwrap();
        let back = extract("sub", &out).unwrap();
        assert_eq!(back.units.len(), 3, "{text:?}");
    }
    // an invented tag id cannot bring back the raw text or a hidden code
    let mut doc = extract("sub", MDVD).unwrap();
    doc.units[1].segments[0].target = vec![Inline::tag("~raw", TagKind::Standalone, "x"), Inline::text("ok")];
    let out = String::from_utf8(filter("sub").merge(&doc).unwrap()).unwrap();
    assert!(out.contains("{25}{50}{y:i}ok|There") || out.contains("{25}{50}ok"), "{out}");
    assert!(!out.contains("Hello|There"), "{out}");
}

#[test]
fn a_translation_cannot_make_a_frame_rate_line() {
    // the cue {0}{1} would read as a frame-rate declaration and drop out of the file's cues
    assert!(matches!(translate("sub", MDVD, 0, "25"), Err(Error::Write(m)) if m.contains("frame-rate")));
    assert!(matches!(translate("sub", MDVD, 0, "23.976"), Err(Error::Write(_))));
    assert!(translate("sub", MDVD, 0, "25 fps").is_ok());
    // another cue is not affected
    let out = translate("sub", MDVD, 2, "25").unwrap();
    assert_eq!(extract("sub", &out).unwrap().units.len(), 3);
}

#[test]
fn a_translation_cannot_make_a_header_or_mix_formats() {
    let out = translate("sub", SV2, 0, "[INFORMATION]\n[TITLE]evil").unwrap();
    let back = extract("sub", &out).unwrap();
    assert_eq!(back.units.len(), 2);
    assert!(String::from_utf8(out).unwrap().starts_with("[INFORMATION]\n[TITLE]t\n[END INFORMATION]\n"));
    // a MicroDVD-looking line in a SubViewer file would make the file ambiguous
    const SV2N: &[u8] = b"00:00:01.00,00:00:02.00\nOne\nx\n\n00:00:03.00,00:00:04.00\nTwo\n";
    assert!(matches!(translate("sub", SV2N, 0, "A\n{1}{2}B"), Err(Error::Write(m)) if m.contains("MicroDVD cue")));
    // with [br] as the line break the text stays on one line and cannot be a cue
    let out = translate("sub", SV2, 0, "A\n{1}{2}B").unwrap();
    assert_eq!(extract("sub", &out).unwrap().units.len(), 2);
}

#[test]
fn an_empty_or_blank_translation_is_refused_not_a_deleted_cue() {
    for (fmt, data) in [("sbv", SBV), ("sub", SV2), ("sub", MDVD)] {
        for text in [" ", "\n", "\u{3000}"] {
            assert!(matches!(translate(fmt, data, 1, text), Err(Error::Write(_))), "{fmt} {text:?}");
        }
    }
    // codes and breaks only
    assert!(matches!(translate("sub", MDVD, 1, "|"), Err(Error::Write(_))));
}

#[test]
fn unicode_space_only_lines_in_a_block_cue_are_dropped_not_written() {
    let out = translate("sbv", SBV, 0, "A\n\u{3000}\nB").unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "0:00:01.000,0:00:02.000\nA\nB\n\n0:00:03.000,0:00:04.000\nTwo\n");
}
