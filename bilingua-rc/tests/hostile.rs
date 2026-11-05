//! Hostile input: random bytes, rc-shaped token soup, mutated fixtures, every truncation, deep and huge files.
//! Nothing panics; a file that is accepted merges back to its exact bytes (but for a multi-byte page whose
//! bytes have several spellings, which the document warns about).
mod common;
use bilingua_model::*;
use bilingua_rc::Rc;
use common::*;
use proptest::prelude::*;

fn exact_expected(doc: &Document) -> bool {
    !doc.warnings.iter().any(|w| w.message.contains("more than one spelling"))
}

fn check(data: &[u8]) {
    let _ = Rc.sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            if exact_expected(&doc) {
                assert_eq!(Rc.merge(&doc).unwrap(), data, "untouched merge must be exact");
            }
            let ids: Vec<(String, String)> =
                doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), plain_text(&u.source()))).collect();
            let n = doc.units.len();
            doc.target_lang = Some("de-DE".into());
            for (id, t) in ids {
                doc.set_target(&id, vec![Inline::text(format!("~{t}~"))]).unwrap();
            }
            let out = match Rc.merge(&doc) {
                Ok(out) => out,
                // a translation that would not read back as the same unit is refused, never written
                Err(Error::Write(m)) if m.contains("format placeholder") => return,
                Err(e) => panic!("translated merge failed: {e}"),
            };
            // a BOM-less UTF-16 file is found by its zero bytes; a translation may change that, so only
            // the other envelopes promise the same unit count
            if !matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == "utf-16le-nobom") {
                let back = extract(&out).unwrap_or_else(|e| panic!("merged output unreadable: {e}"));
                assert_eq!(back.units.len(), n, "unit count after translation");
            }
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(corpus_dir()).unwrap().map(|e| std::fs::read(e.unwrap().path()).unwrap()).collect();
    v.push(SAMPLE.as_bytes().to_vec());
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn rc_shaped_soup_never_panics(words in proptest::collection::vec(
        "(BEGIN|END|\\{|\\}|STRINGTABLE|MENU|MENUEX|DIALOG|DIALOGEX|VERSIONINFO|POPUP|MENUITEM|CAPTION|LTEXT|CONTROL|VALUE|BLOCK|LANGUAGE|#pragma code_page\\([0-9]{3,5}\\)\n|#include \"x\\.h\"\n|//[a-z\"]{0,5}\n|/\\*[a-z\"]{0,5}\\*?/?|L?\"[A-Za-z %d&\\\\n\\\\t\"\\\\]{0,12}\"?|[0-9]{1,3}|0x[0-9a-f]{1,4}|,|[A-Z_]{1,8}|\n| )", 0..120),
        nl in prop::sample::select(vec!["\n", "\r\n", "\r"])) {
        check(words.join(" ").replace('\n', nl).as_bytes());
    }
}

#[test]
fn every_truncation_of_the_fixtures_is_safe() {
    for d in fixtures() {
        // every cut point of the small ones, a stride through the rest
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
    let deep = format!("STRINGTABLE {}1 \"x\" {}", "BEGIN ".repeat(200_000), "END ".repeat(200_000));
    check(deep.as_bytes());
    let braces = format!("X MENU {}POPUP \"&a\" {}", "{ ".repeat(200_000), "} ".repeat(200_000));
    check(braces.as_bytes());
    let comments = "/*".repeat(100_000);
    check(comments.as_bytes());
    let slashes = "//\n".repeat(100_000);
    check(slashes.as_bytes());
    let strs = "L\"".repeat(100_000);
    check(strs.as_bytes());
    let lang = "LANGUAGE 1, ".repeat(100_000);
    check(lang.as_bytes());
    let unknown = "A B ,\n".repeat(100_000);
    check(unknown.as_bytes());
    let pre = format!("#define X \\\n{}", "a \\\n".repeat(100_000));
    check(pre.as_bytes());
    let bs = format!("STRINGTABLE BEGIN 1 \"{}\" END", "\\".repeat(300_001));
    check(bs.as_bytes());
    let ctrl = format!("DIALOG D BEGIN CONTROL \"a\" {} END", "SS_BITMAP ".repeat(100_000));
    check(ctrl.as_bytes());
    assert!(t0.elapsed().as_secs() < 30, "took {:?}", t0.elapsed());
}

#[test]
fn limits_are_typed_errors() {
    let mut many = String::from("STRINGTABLE\nBEGIN\n");
    for i in 0..200 {
        many.push_str(&format!(" {i} \"Text {i}\"\n"));
    }
    many.push_str("END\n");
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(100));
    assert!(matches!(Rc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_input_bytes(50));
    assert!(matches!(Rc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_lines(10));
    assert!(matches!(Rc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Lines));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_unit_chars(10));
    assert!(matches!(Rc.extract(b"STRINGTABLE BEGIN 1 \"a very long string indeed\" END", &o), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
}

#[test]
fn encoding_errors_are_typed_and_say_where() {
    for (data, needle) in [
        (&b"\xff\xfeS\x00T"[..], "UTF-16"),
        (&b"\xef\xbb\xbf\xff\xfe"[..], "UTF-8"),
        (&b"#pragma code_page(65001)\n\xe9\n"[..], "65001"),
        (&b"#pragma code_page(99999)\n"[..], "not supported"),
        (&b"STRINGTABLE\x81"[..], "Windows-1252"),
    ] {
        match extract(data) {
            Err(Error::Encoding(m)) => assert!(m.contains(needle), "{m}"),
            other => panic!("{other:?}"),
        }
    }
}


/// Proptest seed 5c14c7cb: `STRINGTABLE { "%\aE"\`. The source is `%`, the `\a` escape, `E`; a translation of
/// `~%E~` (the escape dropped) writes `"~%E~"`, whose only letter joins the `%` into the printf placeholder `%E`,
/// so the string read back as codes only and the unit vanished. Such a merge is refused with a typed error; a
/// placeholder formed next to other letters is still written byte for byte, as LocOS does.
#[test]
fn translation_whose_letters_become_a_placeholder_is_refused() {
    for (src, target) in [
        (&br#"STRINGTABLE { "%\aE"\"#[..], "~%E~"),
        (&br#"STRINGTABLE { "%\aE" }"#[..], "~%E~"),
        (&br#"STRINGTABLE { L"%\aE" }"#[..], "%d"),
    ] {
        let mut doc = extract(src).unwrap();
        assert_eq!(doc.units.len(), 1);
        doc.target_lang = Some("de-DE".into());
        doc.set_target("u1", vec![Inline::text(target)]).unwrap();
        match Rc.merge(&doc) {
            Err(Error::Write(m)) => assert!(m.contains("format placeholder") && m.contains("u1"), "{m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
        check(src);
    }
    let src = br#"STRINGTABLE { 1 "%\aE" }"#;
    let mut doc = extract(src).unwrap();
    doc.set_target("u1", vec![Inline::text("50%d Rabatt")]).unwrap();
    let out = Rc.merge(&doc).unwrap();
    assert_eq!(out, br#"STRINGTABLE { 1 "50%d Rabatt" }"#);
    assert_eq!(extract(&out).unwrap().units.len(), 1);
}
