//! The made-up corpus (`oracle/corpus/plist`): 11 languages (English, German, French, Spanish, Japanese, Chinese,
//! Korean, Arabic, Hindi, Russian, Portuguese), UTF-8 with BOM and CRLF, UTF-16 LE and BE, binary plists, Settings
//! bundles and the edge files. Round trips, the model contract (paragraph and sentence modes, stable ids, one LocOS
//! token per tag) and the plural fill per target language.
mod common;
use bilingua_model::locos::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_plist::stringsdict_records;
use bilingua_seg::{segment, Rules};
use common::*;
use std::path::PathBuf;

fn corpus() -> Vec<(String, Vec<u8>)> {
    // The oracle corpus, plus the stringsdict files whose plurals the es-ES plural fill extends (the oracle's pseudo
    // round trip merges for es-ES and wants the same number of units back; here the fill is tested on purpose).
    let mut v: Vec<(String, Vec<u8>)> = Vec::new();
    for sub in ["../oracle/corpus/plist", "tests/data/fill"] {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(sub);
        for f in std::fs::read_dir(dir).unwrap().flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                v.push((name, std::fs::read(f.path()).unwrap()));
            }
        }
    }
    v.sort();
    assert!(v.len() >= 30, "corpus went missing");
    v
}

fn kind(name: &str) -> F {
    if name.ends_with(".stringsdict") {
        F::Sd
    } else {
        F::Plist
    }
}

fn open(name: &str, bytes: &[u8], mode: Granularity, target: Option<&str>) -> Document {
    let opts = ExtractOptions { source_lang: Some("en".into()), target_lang: target.map(String::from), granularity: mode, include_locked: true, ..Default::default() };
    let mut d = ex_with(kind(name), bytes, &opts);
    segment(&mut d, "en", mode, Rules::bundled());
    d
}

#[test]
fn every_file_round_trips_untouched_and_echoed() {
    for (name, bytes) in corpus() {
        let f = kind(&name);
        for mode in [Granularity::Paragraph, Granularity::Sentence] {
            for target in [None, Some("es-ES"), Some("ar")] {
                let d = open(&name, &bytes, mode, target);
                assert_eq!(merged(f, &d), bytes, "{name} {mode:?} {target:?}: an untouched merge must be the original bytes");
            }
        }
        assert_eq!(echo(f, &bytes), bytes, "{name}: echo");
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (name, bytes) in corpus() {
        let f = kind(&name);
        let d = ex(f, &bytes);
        let p = prefixed(&d, "[x] ");
        let out = merged(f, &p);
        assert_ne!(out, bytes, "{name}");
        let back = ex(f, &out);
        assert_eq!(back.units.len(), d.units.len(), "{name}");
        for (a, b) in d.units.iter().zip(&back.units) {
            assert_eq!(b.segments[0].source[0], Inline::text(format!("[x] {}", shown_head(&a.source()))), "{name}: {}", a.id);
            let tags = |u: &Unit| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
            assert_eq!(tags(a), tags(b), "{name}: {}", a.id);
        }
        // binary files keep their other values (the reader would refuse a damaged file)
        if bytes.starts_with(b"bplist00") {
            assert!(out.starts_with(b"bplist00"), "{name}");
        } else {
            // everything outside the strings is untouched: same text once the values are cut out
            let strip = |b: &[u8]| String::from_utf8_lossy(b).lines().filter(|l| !l.contains("<string>")).count();
            assert_eq!(strip(&bytes), strip(&out), "{name}");
        }
    }
}

/// The first text piece of a source (the pseudo translation puts its prefix before it).
fn shown_head(v: &[Inline]) -> String {
    match v.first() {
        Some(Inline::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

#[test]
fn paragraph_and_sentence_modes_agree_on_ids_and_text() {
    for (name, bytes) in corpus() {
        let sent = open(&name, &bytes, Granularity::Sentence, None);
        let para = open(&name, &bytes, Granularity::Paragraph, None);
        let ids = |d: &Document| d.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&sent), ids(&para), "{name}");
        for u in &para.units {
            assert_eq!(u.segments.len(), 1, "{name}");
            assert_eq!(u.segments[0].id, u.id);
            assert!(u.separators.is_empty());
        }
        for (a, b) in sent.units.iter().zip(&para.units) {
            assert_eq!(a.source(), b.source(), "{name}");
        }
        let a = open(&name, &bytes, Granularity::Sentence, None);
        let b = open(&name, &bytes, Granularity::Sentence, None);
        assert_eq!(a, b, "{name}: two extractions differ");
        let mut seen = std::collections::HashSet::new();
        assert!(a.units.iter().all(|u| seen.insert(u.id.clone())), "{name}: duplicate unit ids");
    }
}

#[test]
fn every_tag_is_one_locos_token_and_positional_ones_are_free() {
    let (mut tags, mut free) = (0, 0);
    for (name, bytes) in corpus() {
        for mode in [Granularity::Paragraph, Granularity::Sentence] {
            let d = open(&name, &bytes, mode, None);
            for (_, s) in d.segments() {
                let n = s.source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
                let (t, map) = to_locos_text(&s.source);
                assert_eq!(t.matches('\u{27E6}').count(), n, "{name}");
                assert_eq!(from_locos_text(&t, &map), merge_text(s.source.clone()), "{name}");
                tags += n;
                let nf = s.source.iter().filter(|i| i.is_free()).count();
                free += nf;
                assert_eq!(t.matches("\u{27E6}p").count(), nf);
                for i in &s.source {
                    if let Inline::Tag { display, free, kind, .. } = i {
                        assert_eq!(*kind, TagKind::Standalone);
                        assert_eq!(*free, display.contains('$'), "{name}: {display}");
                    }
                }
            }
        }
    }
    assert!(tags > 20 && free > 0, "{tags} {free}");
}

fn names(out: &[u8], key: &str) -> Vec<String> {
    stringsdict_records(out).unwrap().into_iter().find(|r| r.key == key && r.context.is_empty()).unwrap().plural_forms.into_iter().map(|f| f.0).collect()
}

#[test]
fn plural_fill_follows_each_of_eleven_target_languages() {
    let en = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/fill/lang_en.stringsdict")).unwrap();
    let want: &[(&str, &[&str])] = &[
        ("en", &["one", "other"]),
        ("de", &["one", "other"]),
        ("fr", &["one", "many", "other"]),
        ("es", &["one", "many", "other"]),
        ("pt-BR", &["one", "many", "other"]),
        ("ja", &["one", "other"]),
        ("zh-Hans", &["one", "other"]),
        ("ko", &["one", "other"]),
        ("hi", &["one", "other"]),
        ("ru", &["one", "few", "many", "other"]),
        ("ar", &["one", "zero", "two", "few", "many", "other"]),
    ];
    for (lang, forms) in want {
        let d = ex_with(F::Sd, &en, &ExtractOptions { target_lang: Some((*lang).into()), ..Default::default() });
        let out = merged(F::Sd, &prefixed(&d, "T "));
        assert_eq!(names(&out, "files_count"), *forms, "{lang}");
        let recs = stringsdict_records(&out).unwrap();
        let f = &recs[0].plural_forms;
        let other = &f.iter().find(|x| x.0 == "other").unwrap().1;
        assert!(f.iter().all(|(k, v)| k == "one" || v == other), "{lang}: the added forms carry the translated other text");
    }
    // a file already written for its language keeps exactly its forms
    for lang in ["en", "de", "fr", "es", "ja", "zh", "ko", "ar", "hi", "ru", "pt"] {
        let sub = if matches!(lang, "fr" | "es" | "pt" | "ar" | "ru") { "../oracle/corpus/plist" } else { "tests/data/fill" };
        let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(sub).join(format!("lang_{lang}.stringsdict"))).unwrap();
        let d = ex_with(F::Sd, &bytes, &ExtractOptions { target_lang: Some(lang.into()), ..Default::default() });
        let out = merged(F::Sd, &prefixed(&d, "T "));
        let before: Vec<String> = names(&bytes, "files_count");
        let mut after = names(&out, "files_count");
        let mut b = before.clone();
        b.sort();
        after.sort();
        assert_eq!(b, after, "{lang}");
        assert_eq!(names(&out, "files_count"), before, "{lang}: order kept");
    }
}

#[test]
fn the_eleven_languages_extract_in_their_own_scripts() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/plist");
    let first = |lang: &str| {
        let b = std::fs::read(dir.join(format!("lang_{lang}.plist"))).unwrap();
        units_of(F::Plist, &b)
    };
    for lang in ["en", "de", "fr", "es", "ja", "zh", "ko", "ar", "hi", "ru", "pt"] {
        let u = first(lang);
        assert_eq!(u.len(), 4, "{lang}");
        assert!(u.iter().all(|t| !t.trim().is_empty()));
    }
    assert!(first("ar")[0].chars().all(|c| c == ' ' || ('\u{600}'..='\u{6ff}').contains(&c)));
    assert!(first("ja")[0].chars().all(|c| ('\u{3040}'..='\u{30ff}').contains(&c)));
    assert!(first("zh")[0].chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
    assert!(first("ko")[1].contains('\u{d2b8}'));
    assert!(first("hi")[0].contains('\u{91f}'));
    assert_eq!(first("ru")[0], "\u{417}\u{430}\u{43c}\u{435}\u{442}\u{43a}\u{438} \u{43e} \u{442}\u{440}\u{43e}\u{43f}\u{430}\u{445}");
    // the same file in other envelopes reads the same
    assert_eq!(first("ja"), units_of(F::Plist, &std::fs::read(dir.join("lang_ja_utf16le.plist")).unwrap()));
    assert_eq!(first("ar"), units_of(F::Plist, &std::fs::read(dir.join("lang_ar_utf16be.plist")).unwrap()));
    assert_eq!(first("de"), units_of(F::Plist, &std::fs::read(dir.join("lang_de_utf8bom_crlf.plist")).unwrap()));
    // a target in each script survives the merge and the reopen
    for (lang, text) in [("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627}"), ("ja", "\u{65e5}\u{672c}\u{8a9e}"), ("ko", "\u{d55c}\u{ad6d}\u{c5b4}"), ("zh", "\u{4e2d}\u{6587}"), ("hi", "\u{939}\u{93f}\u{928}\u{94d}\u{926}\u{940}")] {
        for name in ["lang_en.plist", "lang_ja_utf16le.plist", "lang_ar_utf16be.plist", "binary_info.plist"] {
            let bytes = std::fs::read(dir.join(name)).unwrap();
            let mut d = ex(F::Plist, &bytes);
            set_nth(&mut d, 0, text);
            let out = merged(F::Plist, &d);
            assert_eq!(units_of(F::Plist, &out)[0], text, "{lang} {name}");
        }
    }
}

#[test]
fn unit_counts_are_pinned() {
    let want: &[(&str, usize)] = &[
        ("lang_en.plist", 4),
        ("settings.plist", 5),
        ("binary_info.plist", 3),
        ("edge_generic.plist", 9),
        ("edge_usage.plist", 4),
        ("lang_en.stringsdict", 2),
        ("lang_ar.stringsdict", 6),
        ("lang_ru.stringsdict", 4),
        ("binary_ru.stringsdict", 6),
        ("edge.stringsdict", 12),
    ];
    let all = corpus();
    for (file, n) in want {
        let (name, bytes) = all.iter().find(|(f, _)| f == file).unwrap();
        assert_eq!(texts(&ex(kind(name), bytes)).len(), *n, "{file}");
    }
}
