//! Round trips, languages, encodings and stamping over made-up packages.

mod common;
use bilingua_model::{Error, ExtractOptions, Inline, SegState};
use common::*;

const LANGS: [(&str, &str); 11] = [
    ("ar-SA", "\u{645}\u{631}\u{62D}\u{628}\u{627} \u{628}\u{643}"),
    ("ja-JP", "\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}\u{4E16}\u{754C}"),
    ("zh-CN", "\u{4F60}\u{597D}\u{FF0C}\u{4E16}\u{754C}"),
    ("ko-KR", "\u{C548}\u{B155}\u{D558}\u{C138}\u{C694}"),
    ("de-DE", "Gr\u{FC}\u{DF} Gott, \u{DC}bung"),
    ("fr-FR", "Cr\u{E8}me br\u{FB}l\u{E9}e"),
    ("es-ES", "\u{BF}Qu\u{E9} tal, se\u{F1}or?"),
    ("ru-RU", "\u{41F}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43C}\u{438}\u{440}"),
    ("he-IL", "\u{5E9}\u{5DC}\u{5D5}\u{5DD} \u{5E2}\u{5D5}\u{5DC}\u{5DD}"),
    ("hi-IN", "\u{928}\u{92E}\u{938}\u{94D}\u{924}\u{947}"),
    ("pt-BR", "A\u{E7}\u{E3}o r\u{E1}pida"),
];

/// An XLIFF 1.2 file with no `target-language`, two segments, one with a tag pair.
fn plain_xlf(name: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\">\n  <file original=\"{name}\" source-language=\"en-US\" datatype=\"plaintext\">\n    <body>\n      <trans-unit id=\"1\"><source>Click <g id=\"1\">Save</g> now.</source></trans-unit>\n      <trans-unit id=\"2\"><source>Second.</source></trans-unit>\n    </body>\n  </file>\n</xliff>\n"
    )
}

fn pkg_plain() -> Vec<u8> {
    let a = plain_xlf("a.txt");
    let b = plain_xlf("b.txt");
    zip_of(&[("a.xlf", a.as_bytes(), true), ("data/blob.bin", &skeleton_bytes(), false), ("b.xliff", b.as_bytes(), true)], b"hello")
}

#[test]
fn eleven_languages_round_trip_and_stamp() {
    let f = filter();
    for (lang, word) in LANGS {
        let pkg = pkg_plain();
        let mut d = extract_to(&*f, &pkg, lang);
        assert_eq!(d.target_lang.as_deref(), Some(lang));
        assert_eq!(d.segment_count(), 4);
        translate(&mut d, "a.xlf|1", |_| word.to_string());
        let src = d.find("a.xlf|1").unwrap().1.source.clone();
        let w = word.to_string();
        d.set_target("a.xlf|1", src.iter().map(|i| if let Inline::Text(_) = i { Inline::text(w.clone()) } else { i.clone() }).collect()).unwrap();
        d.set_state("a.xlf|1", SegState::Translated).unwrap();
        let out = f.merge(&d).unwrap();
        // reopen: same segment and tag count, the word is the target
        let back = f.extract(&out, &ExtractOptions::default()).unwrap();
        assert_eq!(back.segment_count(), 4, "{lang}");
        let got = back.find("a.xlf|1").unwrap().1;
        assert!(locos(&got.target).contains(word), "{lang}");
        assert_eq!(got.target.iter().filter(|i| matches!(i, Inline::Tag { .. })).count(), 2, "{lang}");
        // every XLIFF member lacking a language is stamped, the blob and the comment are kept
        let want = format!("target-language=\"{lang}\"");
        assert!(text(&member(&out, "a.xlf")).contains(&want), "{lang}");
        assert!(text(&member(&out, "b.xliff")).contains(&want), "{lang}");
        assert_eq!(member(&out, "data/blob.bin"), skeleton_bytes());
        assert_eq!(comment(&out), b"hello");
        // the second member has no translated segment: its segments still read back untranslated
        assert!(back.find("b.xliff|2").unwrap().1.target.is_empty());
    }
}

#[test]
fn nothing_translated_returns_the_original_bytes_and_no_stamp() {
    let f = filter();
    let pkg = pkg_plain();
    let d = extract_to(&*f, &pkg, "fr-FR");
    assert_eq!(f.merge(&d).unwrap(), pkg);
}

#[test]
fn a_files_own_language_is_never_changed() {
    let f = filter();
    let pkg = xlz();
    let mut d = extract_to(&*f, &pkg, "ja-JP");
    translate(&mut d, "content.xlf|1", |_| "x".into());
    let out = text(&member(&f.merge(&d).unwrap(), "content.xlf"));
    assert!(out.contains("target-language=\"fr-FR\"") && !out.contains("ja-JP"));
}

#[test]
fn bad_language_codes_do_not_stamp() {
    let f = filter();
    for bad in ["", "x y", "\"><evil", "-"] {
        let mut d = extract(&*f, &pkg_plain());
        d.target_lang = Some(bad.into());
        translate(&mut d, "a.xlf|1", |_| "x".into());
        let out = f.merge(&d).unwrap();
        assert!(!text(&member(&out, "a.xlf")).contains("target-language"), "{bad:?}");
    }
}

#[test]
fn member_order_names_dates_and_compression_are_kept() {
    let f = filter();
    let pkg = pkg_plain();
    let mut d = extract(&*f, &pkg);
    translate(&mut d, "b.xliff|2", |_| "Deuxieme.".into());
    let out = f.merge(&d).unwrap();
    let (a, b) = (members(&pkg), members(&out));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!((&x.0, x.1, x.2), (&y.0, y.1, y.2));
    }
    assert_eq!(a[0], b[0], "the first XLIFF member is untouched");
    assert_eq!(a[1], b[1]);
    assert_ne!(a[2].3, b[2].3);
}

fn utf16(s: &str, le: bool) -> Vec<u8> {
    let mut v = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
    for u in s.encode_utf16() {
        v.extend(if le { u.to_le_bytes() } else { u.to_be_bytes() });
    }
    v
}

#[test]
fn utf16_and_bom_members_keep_their_encoding() {
    let f = filter();
    let src = plain_xlf("u.txt").replace("encoding=\"UTF-8\"", "encoding=\"UTF-16\"");
    let bom8: Vec<u8> = [&[0xEF, 0xBB, 0xBF][..], plain_xlf("v.txt").as_bytes()].concat();
    for le in [true, false] {
        let pkg = zip_of(&[("u.xlf", &utf16(&src, le), true), ("v.xlf", &bom8, true)], b"");
        let mut d = extract_to(&*f, &pkg, "ar-SA");
        assert_eq!(d.segment_count(), 4);
        assert_eq!(f.merge(&d).unwrap(), pkg);
        translate(&mut d, "u.xlf|2", |_| "\u{62B}\u{627}\u{646}\u{64A}".into());
        translate(&mut d, "v.xlf|2", |_| "\u{62B}\u{627}\u{646}\u{64A}".into());
        let out = f.merge(&d).unwrap();
        let u = member(&out, "u.xlf");
        assert!(u.starts_with(if le { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] }), "BOM kept");
        let back = f.extract(&out, &ExtractOptions::default()).unwrap();
        assert!(!back.find("u.xlf|2").unwrap().1.target.is_empty());
        assert!(!back.find("v.xlf|2").unwrap().1.target.is_empty());
        assert!(member(&out, "v.xlf").starts_with(&[0xEF, 0xBB, 0xBF]));
        assert!(member(&out, "v.xlf").windows(15).any(|w| w == b"target-language"));
    }
}

#[test]
fn a_bar_in_a_path_cannot_mix_two_members() {
    let f = filter();
    let x = |id: &str| format!("<?xml version=\"1.0\"?>\n<xliff version=\"1.2\"><file source-language=\"en\" target-language=\"ja\" original=\"o\"><body><trans-unit id=\"{id}\"><source>Hello</source></trans-unit></body></file></xliff>");
    let (a, b) = (x("b.xlf|1"), x("1"));
    let pkg = zip_of(&[("a.xlf", a.as_bytes(), true), ("a.xlf|b.xlf", b.as_bytes(), true)], b"");
    let mut d = extract(&*f, &pkg);
    let all = ids(&d);
    assert_eq!(all.len(), 2);
    assert_ne!(all[0], all[1]);
    translate(&mut d, &all[1], |_| "\u{3053}\u{3093}".into());
    let out = f.merge(&d).unwrap();
    assert_eq!(member(&out, "a.xlf"), a.as_bytes());
    assert!(text(&member(&out, "a.xlf|b.xlf")).contains("\u{3053}\u{3093}"));
}

#[test]
fn locked_segments_are_never_written() {
    let f = filter();
    let mut d = extract(&*f, &xlz());
    let before = d.clone();
    let _ = d.set_target("content.xlf|3", vec![Inline::text("x")]);
    assert_eq!(d.units[2], before.units[2]);
}

#[test]
fn mxliff_and_sdlxlf_members_are_routed() {
    let f = filter();
    let sdl = SDL;
    let pkg = zip_of(&[("a.SDLXLF", sdl, true), ("b/c.Xliff", WS, true)], b"");
    let d = extract(&*f, &pkg);
    assert!(d.units.iter().any(|u| u.context.location.as_deref() == Some("a.SDLXLF")));
    assert!(d.units.iter().any(|u| u.context.location.as_deref() == Some("b/c.Xliff")));
}

#[test]
fn skeleton_survives_json() {
    let f = filter();
    let pkg = wsxz();
    let d = extract(&*f, &pkg);
    let j = serde_json::to_string(&d).unwrap();
    let d2: bilingua_model::Document = serde_json::from_str(&j).unwrap();
    assert_eq!(f.merge(&d2).unwrap(), pkg);
}

#[test]
fn a_foreign_skeleton_is_refused() {
    let f = filter();
    let mut d = extract(&*f, &xlz());
    d.skeleton = bilingua_model::Skeleton::default();
    assert!(matches!(f.merge(&d), Err(Error::Write(_))));
}

#[test]
fn corpus_files_round_trip_untouched_and_pseudo_translated() {
    let f = filter();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/xliffpkg");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let pkg = std::fs::read(&p).unwrap();
        let mut d = extract_to(&*f, &pkg, "es-ES");
        assert_eq!(f.merge(&d).unwrap(), pkg, "{p:?}");
        let (units, tags): (usize, usize) = (d.segment_count(), d.segments().map(|(_, s)| s.source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum());
        for u in d.units.iter_mut().filter(|u| u.translate) {
            let mut t = vec![Inline::text("[")];
            t.extend(u.segments[0].source.clone());
            t.push(Inline::text("]"));
            u.segments[0].target = t;
            u.segments[0].state = SegState::Translated;
        }
        let out = f.merge(&d).unwrap();
        let back = f.extract(&out, &ExtractOptions::default()).unwrap();
        assert_eq!(back.segment_count(), units, "{p:?}");
        let tags2: usize = back.segments().map(|(_, s)| s.source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum();
        assert_eq!(tags2, tags, "{p:?}");
        assert!(back.segments().any(|(_, s)| locos(&s.target).starts_with('[')), "{p:?}");
        n += 1;
    }
    assert!(n >= 4);
}
