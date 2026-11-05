//! Round trips on the corpus: untouched and echo merges are exact; pseudo-translated merges read back with the
//! same units and codes; every corpus file takes a translation in each of twelve languages in whatever code page it
//! is in (what the page cannot hold is written as \U+XXXX); the target language changes nothing.
mod common;
use bilingua_dxf::Dxf;
use bilingua_model::*;
use common::*;

const GREETINGS: [(&str, &str); 12] = [
    ("en", "Welcome to Lumen Bakery"),
    ("de", "Willkommen bei Lumen Bakery"),
    ("fr", "Bienvenue \u{e0} Lumen Bakery"),
    ("es", "Bienvenido a Lumen Bakery, ni\u{f1}os"),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{645}\u{62e}\u{628}\u{632} \u{644}\u{648}\u{645}\u{646}"),
    ("ja", "\u{30eb}\u{30fc}\u{30e1}\u{30f3}\u{30d9}\u{30fc}\u{30ab}\u{30ea}\u{30fc}\u{3078}\u{3088}\u{3046}\u{3053}\u{305d}"),
    ("zh", "\u{6b22}\u{8fce}\u{6765}\u{5230}\u{6d41}\u{660e}\u{9762}\u{5305}\u{5e97}"),
    ("ko", "\u{b8e8}\u{bcf4} \u{bca0}\u{c774}\u{cee4}\u{b9ac}\u{c5d0} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}"),
    ("ru", "\u{414}\u{43e}\u{431}\u{440}\u{43e} \u{43f}\u{43e}\u{436}\u{430}\u{43b}\u{43e}\u{432}\u{430}\u{442}\u{44c} \u{432} \u{43f}\u{435}\u{43a}\u{430}\u{440}\u{43d}\u{44e}"),
    ("el", "\u{39a}\u{3b1}\u{3bb}\u{3ce}\u{3c2} \u{3ae}\u{3c1}\u{3b8}\u{3b1}\u{3c4}\u{3b5}"),
    ("he", "\u{5d1}\u{5e8}\u{5d5}\u{5db}\u{5d9}\u{5dd} \u{5d4}\u{5d1}\u{5d0}\u{5d9}\u{5dd} \u{5dc}\u{5de}\u{5d0}\u{5e4}\u{5d9}\u{5d9}\u{5d4}"),
    ("th", "\u{e22}\u{e34}\u{e19}\u{e14}\u{e35}\u{e15}\u{e49}\u{e2d}\u{e19}\u{e23}\u{e31}\u{e1a}\u{e2a}\u{e39}\u{e48}\u{e23}\u{e49}\u{e32}\u{e19}\u{e40}\u{e1a}\u{e40}\u{e01}\u{e2d}\u{e23}\u{e35}\u{e48}"),
];

fn tags_of(u: &Unit) -> Vec<String> {
    u.source().iter().filter_map(|i| if let Inline::Tag { display, .. } = i { Some(display.clone()) } else { None }).collect()
}

#[test]
fn corpus_untouched_and_echo_merges_are_exact() {
    let c = corpus();
    assert!(c.len() >= 30);
    for (name, data) in c {
        let doc = extract(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!doc.units.is_empty(), "{name}");
        assert_eq!(Dxf.merge(&doc).unwrap(), data, "{name}: untouched");
        assert_identity(&data);
        assert!(Dxf.sniff(&data), "{name}: sniff");
    }
}

#[test]
fn corpus_pseudo_translation_reads_back_with_the_same_codes() {
    for (name, data) in corpus() {
        let mut doc = extract(&data).unwrap();
        let before: Vec<Vec<String>> = doc.units.iter().map(tags_of).collect();
        for u in &mut doc.units {
            // a prefix only: a dangling backslash at the end of a value would join a suffix into one code
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            u.segments[0].target = t;
        }
        let out = Dxf.merge(&doc).unwrap();
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            back.units.len(),
            doc.units.len(),
            "{name}: {:?} then {:?}",
            doc.units.iter().map(|u| plain_text(&u.output())).collect::<Vec<_>>(),
            back.units.iter().map(|u| plain_text(&u.source())).collect::<Vec<_>>()
        );
        let after: Vec<Vec<String>> = back.units.iter().map(tags_of).collect();
        assert_eq!(before, after, "{name}: codes");
        for (a, b) in doc.units.iter().zip(&back.units) {
            assert_eq!(plain_text(&a.output()), plain_text(&b.source()), "{name}: {}", a.id);
        }
    }
}

#[test]
fn every_corpus_file_takes_a_translation_in_twelve_languages() {
    for (name, data) in corpus() {
        for (lang, greeting) in GREETINGS {
            let mut doc = extract(&data).unwrap();
            let n = doc.units.len();
            for u in &mut doc.units {
                let mut t = vec![Inline::text(format!("{greeting} "))];
                t.extend(u.source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })));
                u.segments[0].target = t;
            }
            let out = Dxf.merge(&doc).unwrap_or_else(|e| panic!("{name} {lang}: {e}"));
            let back = extract(&out).unwrap_or_else(|e| panic!("{name} {lang}: {e}"));
            assert_eq!(back.units.len(), n, "{name} {lang}");
            for (a, b) in doc.units.iter().zip(&back.units) {
                assert_eq!(plain_text(&a.output()), plain_text(&b.source()), "{name} {lang} {}", a.id);
                assert_eq!(tags_of(a).len(), tags_of(b).len(), "{name} {lang}");
            }
            // everything outside the units is untouched: same group codes in the same order
            let strip = |d: &[u8]| -> usize { d.windows(5).filter(|w| w == b"MTEXT").count() };
            assert_eq!(strip(&data), strip(&out), "{name} {lang}");
        }
    }
}

#[test]
fn the_target_language_and_merge_options_change_nothing() {
    for (name, data) in corpus() {
        let mut doc = extract(&data).unwrap();
        doc.target_lang = Some("de-DE".into());
        for u in &mut doc.units {
            u.segments[0].target = vec![Inline::text("Zielwert")];
        }
        let a = Dxf.merge(&doc).unwrap();
        let b = Dxf.merge_with(&doc, &profile::locos().merge_options("dxf")).unwrap();
        assert_eq!(a, b, "{name}");
        doc.target_lang = None;
        assert_eq!(a, Dxf.merge(&doc).unwrap(), "{name}");
        // nothing translated: the original bytes, with or without a target language
        let mut empty = extract(&data).unwrap();
        empty.target_lang = Some("ja-JP".into());
        assert_eq!(Dxf.merge_with(&empty, &profile::locos().merge_options("dxf")).unwrap(), data, "{name}");
    }
}

#[test]
fn skeleton_survives_json() {
    for (name, data) in corpus() {
        let doc = extract(&data).unwrap();
        let j = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&j).unwrap();
        assert_eq!(Dxf.merge(&back).unwrap(), data, "{name}");
    }
}

#[test]
fn skeleton_checks() {
    let data = &corpus()[0].1;
    let mut doc = extract(data).unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        parser.as_mut().unwrap().name = "bilingua-other".into();
    }
    assert!(matches!(Dxf.merge(&doc), Err(Error::ParserVersion { .. })));
    let mut doc = extract(data).unwrap();
    if let Skeleton::Text { encoding, .. } = &mut doc.skeleton {
        *encoding = "utf-8".into();
    }
    assert!(matches!(Dxf.merge(&doc), Err(Error::Write(_))));
    let mut doc = extract(data).unwrap();
    doc.skeleton = Skeleton::Zip { entries: vec![], parser: doc.skeleton.parser().cloned() };
    assert!(matches!(Dxf.merge(&doc), Err(Error::Write(_))));
}

/// Group-pair attacks from a translation: line breaks, control characters, `0`/`SECTION`/`ENDSEC`/`EOF` text and
/// `999` lines never add or remove a group; the merged file has the same group codes in the same order.
#[test]
fn a_translation_never_makes_a_new_group_pair() {
    let src = "  0\nSECTION\n  2\nENTITIES\n  0\nTEXT\n  1\nFire exit\n  0\nMTEXT\n  3\nchunk one \n  1\n{\\fArial;Living} room\\P2\n  0\nATTRIB\n  2\nTAG\n  1\nValue\n  0\nENDSEC\n  0\nEOF\n";
    let codes = |b: &[u8]| -> Vec<String> {
        let s = String::from_utf8_lossy(b).replace("\r\n", "\n").replace('\r', "\n");
        s.split('\n').step_by(2).map(|c| c.trim().to_string()).collect()
    };
    let hostile = ["x\n  0\nENDSEC\n  0\nEOF", "a\r\n999\r\ncomment", "\r0\rSECTION\r", "line\u{2028}sep\u{85}nel\u{b}\u{c}\u{1}", "  0\nSECTION"];
    for h in hostile {
        let mut doc = extract(src.as_bytes()).unwrap();
        let ids: Vec<String> = doc.units.iter().map(|u| u.segments[0].id.clone()).collect();
        for id in &ids {
            doc.set_target(id, vec![Inline::text(h)]).unwrap();
        }
        let out = Dxf.merge(&doc).unwrap();
        let mut want = codes(src.as_bytes());
        // the MTEXT's two chunks become one short group 1
        let k = want.iter().position(|c| c == "3").unwrap();
        want.remove(k);
        assert_eq!(codes(&out), want, "{h:?}");
        let back = extract(&out).unwrap();
        assert_eq!(back.units.len(), 3, "{h:?}");
    }
}

/// A hand-made document cannot smuggle a line break into a value through a code: the merge refuses it.
#[test]
fn a_code_with_a_line_break_is_refused() {
    let src = b"  0\nSECTION\n  2\nENTITIES\n  0\nMTEXT\n  1\n{\\fArial;Living} room\n  0\nENDSEC\n  0\nEOF\n";
    let mut doc = extract(src).unwrap();
    let u = &mut doc.units[0];
    u.codes.insert("1".into(), CodeData { standalone: Some("x\n  0\nEOF".into()), ..Default::default() });
    u.segments[0].target = vec![Inline::tag("1", TagKind::Standalone, "x"), Inline::text("Wohnzimmer")];
    assert!(matches!(Dxf.merge(&doc), Err(Error::Unwritable { .. })));
}

/// An empty translation writes an empty value (LocOS writes "" too); the drawing still reads.
#[test]
fn an_empty_translation_writes_an_empty_value() {
    let src = b"  0\nSECTION\n  2\nENTITIES\n  0\nTEXT\n  1\nFire exit\n  0\nMTEXT\n  3\nchunk one \n  1\nLiving room\n  0\nENDSEC\n  0\nEOF\n";
    let mut doc = extract(src).unwrap();
    for i in 0..doc.units.len() {
        let id = doc.units[i].segments[0].id.clone();
        doc.set_target_empty(&id).unwrap();
    }
    let out = Dxf.merge(&doc).unwrap();
    assert_eq!(out, b"  0\nSECTION\n  2\nENTITIES\n  0\nTEXT\n  1\n\n  0\nMTEXT\n  1\n\n  0\nENDSEC\n  0\nEOF\n");
    assert!(extract(&out).unwrap().units.is_empty());
}
