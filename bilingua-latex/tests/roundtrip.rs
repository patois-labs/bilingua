//! Round trips on the corpus (11 languages, CRLF, BOM, UTF-16, Windows-1252, a chapter fragment) and the Okapi test
//! files: an untouched merge is the original bytes, a pseudo-translated merge reads back with the same units and
//! tags, a target-locale merge changes nothing outside the units (LocOS does not guess a babel language).
mod common;
use bilingua_latex::Latex;
use bilingua_model::*;
use common::*;

fn files() -> Vec<(String, Vec<u8>)> {
    let mut v = Vec::new();
    for dir in [corpus_dir(), std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/okapi")] {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            v.push((p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()));
        }
    }
    v.sort();
    v
}

fn tags(v: &[Inline]) -> Vec<(String, TagKind)> {
    v.iter()
        .filter_map(|i| match i {
            Inline::Tag { id, kind, .. } => Some((id.clone(), *kind)),
            _ => None,
        })
        .collect()
}

/// Every text run gets a mark: accented vowels and a prefix, tags kept as they are.
fn pseudo(src: &[Inline]) -> Vec<Inline> {
    let mut out = vec![Inline::text("\u{ab}")];
    for i in src {
        out.push(match i {
            Inline::Text(t) => Inline::Text(
                t.chars()
                    .map(|c| match c {
                        'a' => '\u{e0}',
                        'e' => '\u{e9}',
                        'o' => '\u{f6}',
                        c => c,
                    })
                    .collect(),
            ),
            other => other.clone(),
        });
    }
    out.push(Inline::text("\u{bb}"));
    out
}

fn translate_all(doc: &mut Document) {
    for u in doc.units.iter_mut().filter(|u| u.translate) {
        let t = pseudo(&u.source());
        u.segments[0].target = t;
    }
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, data) in files() {
        if let Ok(doc) = extract(&data) {
            assert_eq!(Latex::default().merge(&doc).unwrap(), data, "{name}");
            assert!(!doc.units.is_empty() || name.starts_with("comments_only") || name.starts_with("unclosed_verbatim"), "{name} has units");
        }
    }
}

#[test]
fn pseudo_translated_merge_reads_back_with_the_same_units_and_tags() {
    let mut n = 0;
    for (name, data) in files() {
        let Ok(mut doc) = extract(&data) else { continue };
        let before: Vec<(usize, Vec<(String, TagKind)>)> = doc.units.iter().map(|u| (u.segments.len(), tags(&u.source()))).collect();
        translate_all(&mut doc);
        let out = Latex::default().merge(&doc).unwrap();
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.units.len(), doc.units.len(), "{name}: unit count");
        for (k, u) in back.units.iter().enumerate() {
            assert_eq!(tags(&u.source()), before[k].1, "{name}: unit {k} tags");
            assert_eq!(plain_text(&u.source()), format!("\u{ab}{}\u{bb}", plain_text(&pseudo_inner(&doc.units[k]))), "{name}: unit {k}");
        }
        n += 1;
    }
    assert!(n >= 25, "{n} files");
}

fn pseudo_inner(u: &Unit) -> Vec<Inline> {
    let p = pseudo(&u.source());
    p[1..p.len() - 1].to_vec()
}

#[test]
fn eleven_languages() {
    for code in ["en", "es", "fr", "it", "de", "nl", "pt", "ja", "zh", "ko", "ar"] {
        let data = std::fs::read(corpus_dir().join(format!("lumen_{code}.tex"))).unwrap();
        let doc = extract(&data).unwrap();
        assert!(doc.units.len() >= 4, "{code}: {}", doc.units.len());
        assert_identity(&data);
        let all: String = doc.units.iter().map(|u| plain_text(&u.source())).collect::<Vec<_>>().join(" ");
        assert!(all.contains("Lumen"), "{code}: {all}");
        // the language sentence itself is a unit with its own characters
        let mut d2 = doc.clone();
        translate_all(&mut d2);
        let out = Latex::default().merge(&d2).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains('\u{ab}'), "{code}");
        // the preamble (babel, polyglossia, CJK packages) is untouched
        for line in String::from_utf8(data).unwrap().lines().filter(|l| l.starts_with("\\usepackage") || l.starts_with("\\setmainlanguage")) {
            assert!(s.contains(line), "{code}: {line}");
        }
    }
}

#[test]
fn rtl_and_cjk_text_is_kept_and_not_escaped() {
    for code in ["ar", "ja", "zh", "ko"] {
        let data = std::fs::read(corpus_dir().join(format!("lumen_{code}.tex"))).unwrap();
        let doc = extract(&data).unwrap();
        let src = plain_text(&doc.units[1].source());
        let mut d2 = doc.clone();
        translate_all(&mut d2);
        let s = String::from_utf8(Latex::default().merge(&d2).unwrap()).unwrap();
        assert!(s.contains(&src), "{code}: {src}");
    }
}

#[test]
fn target_locale_changes_nothing_but_the_units() {
    for (name, data) in files() {
        let Ok(mut doc) = extract(&data) else { continue };
        translate_all(&mut doc);
        let plain = Latex::default().merge(&doc).unwrap();
        for lc in ["de-DE", "ar-SA", "ja-JP", "zh-Hant", "ko"] {
            doc.target_lang = Some(lc.into());
            assert_eq!(Latex::default().merge(&doc).unwrap(), plain, "{name}: {lc}");
        }
        // nothing translated: the original bytes, even with a target language (lead decision 11-04)
        let mut untouched = extract(&data).unwrap();
        untouched.target_lang = Some("de-DE".into());
        assert_eq!(Latex::default().merge(&untouched).unwrap(), data, "{name}");
    }
}

#[test]
fn legacy_windows_1252_keeps_its_encoding_when_the_text_fits() {
    let data = std::fs::read(corpus_dir().join("lumen_latin1.tex")).unwrap();
    assert!(std::str::from_utf8(&data).is_err());
    let doc = extract(&data).unwrap();
    assert!(plain_text(&doc.units[0].source()).contains("Caf\u{e9}"));
    let mut d = doc.clone();
    translate_all(&mut d);
    let out = Latex::default().merge(&d).unwrap();
    assert!(std::str::from_utf8(&out).is_err(), "still Windows-1252");
    assert!(out.windows(7).any(|w| w == b"latin1]"), "the inputenc option is kept");
    // a character the code page lacks: UTF-8, and inputenc no longer claims latin1
    let mut d = doc.clone();
    d.units[0].segments[0].target = vec![Inline::text("\u{65e5}\u{672c}")];
    let out = String::from_utf8(Latex::default().merge(&d).unwrap()).unwrap();
    assert!(out.contains("\\usepackage[utf8]{inputenc}") && !out.contains("latin1"), "{out}");
}

#[test]
fn utf16_keeps_its_byte_order_mark() {
    let data = std::fs::read(corpus_dir().join("lumen_utf16.tex")).unwrap();
    assert_identity(&data);
    let mut doc = extract(&data).unwrap();
    translate_all(&mut doc);
    let out = Latex::default().merge(&doc).unwrap();
    assert!(out.starts_with(&[0xFF, 0xFE]));
    assert_eq!(extract(&out).unwrap().units.len(), doc.units.len());
}

#[test]
fn units_ids_are_stable_and_paragraph_mode_works() {
    let data = std::fs::read(corpus_dir().join("features.tex")).unwrap();
    let a = extract(&data).unwrap();
    let b = Latex::default().extract(&data, &ExtractOptions { granularity: Granularity::Paragraph, ..Default::default() }).unwrap();
    let ids = |d: &Document| d.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&a), ids(&b));
    assert_eq!(ids(&a)[..3], ["u1", "u2", "u3"]);
    assert!(b.units.iter().all(|u| u.segments.len() == 1));
    assert_eq!(a.format, "tex");
    assert!(a.units.iter().all(|u| u.context.kind.is_some() && u.context.location.is_some()));
}

#[test]
fn include_locked_adds_units_without_letters() {
    let data = b"\\begin{document}\nHello.\n\n\\label{a}\\ref{b}\n\n12.5\n\\end{document}\n";
    let plain = extract(data).unwrap();
    assert_eq!(plain.units.len(), 1);
    let doc = Latex::default().extract(data, &ExtractOptions { include_locked: true, ..Default::default() }).unwrap();
    assert_eq!(doc.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1", "k1", "k2"].map(String::from));
    assert!(doc.units.iter().skip(1).all(|u| !u.translate));
    assert_eq!(Latex::default().merge(&doc).unwrap(), data);
}

#[test]
fn okapi_files_are_pinned() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/okapi");
    let t1 = units(&std::fs::read(dir.join("Test01.tex")).unwrap());
    assert_eq!(t1.len(), 8);
    assert_eq!(t1[0], "Installing \u{27E6}x1\u{27E7}");
    assert_eq!(t1[2], "translated");
    assert_eq!(&t1[5..], ["SMT", "NMT", "Automatic evaluation results"]);
    let t2 = units(&std::fs::read(dir.join("Test02.tex")).unwrap());
    assert_eq!(t2.len(), 5);
    assert_eq!(t2[1], "\u{41f}\u{440}\u{435}\u{434}\u{438}\u{441}\u{43b}\u{43e}\u{432}\u{438}\u{435}");
}
