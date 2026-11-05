//! Round trips, languages, BOM and line endings, paragraph mode, target locale, serde, parser stamp.
mod common;
use bilingua_ftl::{structure, FtlFilter};
use bilingua_model::*;
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
        .expect("corpus folder")
        .map(|e| {
            let p = e.unwrap().path();
            (p.file_name().unwrap().to_string_lossy().to_string(), std::fs::read(&p).unwrap())
        })
        .collect();
    v.sort();
    v
}

/// Every unit's text wrapped in [[ ]], tags kept.
fn pseudo(doc: &mut Document) {
    let ids: Vec<(String, Vec<Inline>)> = doc.units.iter().map(|u| (u.segments[0].id.clone(), u.segments[0].source.clone())).collect();
    for (id, src) in ids {
        let mut t = vec![Inline::text("[[")];
        t.extend(src);
        t.push(Inline::text("]]"));
        doc.set_target(&id, merge_text(t)).unwrap();
    }
}

#[test]
fn corpus_untouched_merge_is_byte_exact() {
    let files = corpus();
    assert!(files.len() >= 14, "corpus has {} files", files.len());
    for (name, data) in &files {
        let doc = extract(data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(FtlFilter.merge(&doc).unwrap(), *data, "{name}");
        assert_identity(data);
    }
}

#[test]
fn corpus_pseudo_translation_reads_back() {
    for (name, data) in &corpus() {
        let mut doc = extract(data).unwrap();
        let before = extract(data).unwrap();
        pseudo(&mut doc);
        let out = FtlFilter.merge(&doc).unwrap();
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.units.len(), before.units.len(), "{name}: unit count");
        for (b, src) in back.units.iter().zip(&before.units) {
            // LocOS writes continuation lines at the indent of the first indented one, so relative indentation of a
            // multi-line text can shift (a kept LocOS quirk); the words and line breaks do not.
            let words = |t: &str| t.lines().map(|l| l.trim_start().to_string()).collect::<Vec<_>>();
            assert_eq!(words(&flat(b)), words(&format!("[[{}]]", flat(src))), "{name}");
            let tags = |u: &Unit| u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
            assert_eq!(tags(b), tags(src), "{name}: tags kept");
            assert_eq!(b.context.style, src.context.style, "{name}");
        }
        // entries, attributes and Junk are untouched
        assert_eq!(structure(data).unwrap(), structure(&out).unwrap(), "{name}");
    }
}

#[test]
fn eleven_languages() {
    let langs = ["en", "de", "fr", "es", "ja", "zh", "ko", "ar", "ru", "pt", "tr"];
    let dir = corpus_dir();
    let mut seen = Vec::new();
    for l in langs {
        let data = std::fs::read(dir.join(format!("app_{l}.ftl"))).unwrap();
        let doc = extract(&data).unwrap();
        // login, 2 attributes, welcome, 2 plural variants, tagline = 7 (the term is not translated)
        assert_eq!(doc.units.len(), 7, "{l}: {:?}", units(&data));
        let u = units(&data);
        assert!(u[3].contains("{ $name }") && u[3].contains("{ -brand-name }"), "{l}: placeables kept: {}", u[3]);
        assert_eq!(doc.units[3].segments[0].source.iter().filter(|i| i.is_free()).count(), 2);
        seen.push(u[0].clone());
        assert_identity(&data);
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 11);
    // Arabic and CJK translations written into the English file.
    let en = std::fs::read(dir.join("app_en.ftl")).unwrap();
    for l in ["ar", "ja", "zh", "ko", "ru"] {
        let tr = units(&std::fs::read(dir.join(format!("app_{l}.ftl"))).unwrap());
        let m: Vec<(usize, &str)> = tr.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
        let out = build(&en, &m);
        assert_eq!(units(&out), tr, "{l}");
        assert_eq!(structure(&out).unwrap().junk, 0);
    }
}

#[test]
fn bom_and_line_endings_keep_their_form() {
    let data = std::fs::read(corpus_dir().join("app_de_crlf_bom.ftl")).unwrap();
    assert!(data.starts_with(&[0xEF, 0xBB, 0xBF]));
    let out = build(&data, &[(0, "Neu"), (6, "Zeile eins\nZeile zwei")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    let s = String::from_utf8_lossy(&out[3..]).to_string();
    assert!(!s.replace("\r\n", "").contains('\n'), "no bare LF");
    assert!(s.contains("Zeile eins\r\n    Zeile zwei"));
    // a BOM-less LF file stays LF
    let lf = std::fs::read(corpus_dir().join("app_en.ftl")).unwrap();
    let out = build(&lf, &[(0, "Neu")]);
    assert!(!out.contains(&b'\r') && !out.starts_with(&[0xEF]));
}

#[test]
fn target_locale_and_paragraph_mode() {
    let data = std::fs::read(corpus_dir().join("app_en.ftl")).unwrap();
    let opts = ExtractOptions::new().with_source_lang("en").with_target_lang("ar").with_granularity(Granularity::Paragraph);
    let mut doc = FtlFilter.extract(&data, &opts).unwrap();
    assert_eq!(doc.granularity, Granularity::Paragraph);
    assert_eq!(doc.target_lang.as_deref(), Some("ar"));
    assert!(doc.units.iter().all(|u| u.segments.len() == 1 && u.separators.is_empty()));
    pseudo(&mut doc);
    let out = String::from_utf8(FtlFilter.merge_localized(&doc, None).unwrap()).unwrap();
    // Arabic has six plural categories: zero, two, few, many are added before the default variant
    for cat in ["zero", "two", "few", "many"] {
        assert!(out.contains(&format!("[{cat}] [[{{ $count }}")), "{cat}\n{out}");
    }
    assert_eq!(structure(out.as_bytes()).unwrap().junk, 0);
    // Fluent has no language header: nothing else changes. Without a target locale there are no added variants.
    let mut plain = extract(&data).unwrap();
    pseudo(&mut plain);
    let out = String::from_utf8(FtlFilter.merge(&plain).unwrap()).unwrap();
    assert!(!String::from_utf8(FtlFilter.merge(&doc).unwrap()).unwrap().contains("[few]"), "plain merge adds no variants");
    assert!(!out.contains("[few]"));
    assert!(out.contains("login = [[Log in]]"));
}

#[test]
fn context_serde_and_parser_stamp() {
    let data = std::fs::read(corpus_dir().join("app_en.ftl")).unwrap();
    let doc = extract(&data).unwrap();
    let u = &doc.units[0];
    assert_eq!(u.context.key.as_deref(), Some("login"));
    assert_eq!(u.context.style.as_deref(), Some("ftl:login"));
    assert_eq!(u.context.notes, vec!["Shown on the login button".to_string()]);
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(FtlFilter.merge(&back).unwrap(), data);
    assert!(doc.skeleton.parser().is_some());
    // a skeleton from another parser version is refused
    let mut stale = back;
    if let Skeleton::Text { parser, .. } = &mut stale.skeleton {
        *parser = Some(ParserInfo::new("bilingua-ftl", "0.0.0", 999));
    }
    assert!(FtlFilter.merge(&stale).is_err());
}

#[test]
fn sniff_is_conservative() {
    let dir = corpus_dir();
    assert!(FtlFilter.sniff(&std::fs::read(dir.join("app_fr.ftl")).unwrap()));
    assert!(FtlFilter.sniff(&std::fs::read(dir.join("edge.ftl")).unwrap()));
    assert!(!FtlFilter.sniff(b"[section]\nkey = value\n"));
    assert!(!FtlFilter.sniff(b"key = value\nother = thing\n"));
    assert!(!FtlFilter.sniff(b"# Title\n\nSome prose here.\n"));
    assert!(!FtlFilter.sniff(b"\xff\xfe\x00"));
}

#[test]
fn edge_cases_match_the_reference_parser() {
    // trailing blanks after the last line are dropped from a translated unit (as LocOS does)
    let out = build(b"a = Text   \nb = Next\n", &[(0, "Neu")]);
    assert_eq!(out, b"a = Neu\nb = Next\n");
    // a value that starts on the next line keeps the block form and its indent
    let out = build(b"a =\n        deep\n    shallow\nb = x\n", &[(0, "Neu\nzwei")]);
    assert_eq!(String::from_utf8(out).unwrap(), "a =\n    Neu\n    zwei\nb = x\n");
    // text before and after a select stays two units
    let src = b"n = Before { $c ->\n    [one] One\n   *[other] Many\n} \n";
    assert_eq!(styles(src), ["ftl:n", "ftl:n[one]", "ftl:n[other]"]);
}

#[test]
fn translations_cannot_break_where_localization_os_could() {
    // Deliberate differences from LocOS, which writes Junk here: a text that starts a line must not start with
    // `[`, `*` or `.`, must be indented, and a placeable that spans lines is written as it was.
    let out = build(b"a =\n    Text\n    more\nb = x\n", &[(0, "[x] y")]);
    assert_eq!(String::from_utf8(out.clone()).unwrap(), "a =\n    {\"[\"}x] y\nb = x\n");
    assert_eq!(structure(&out).unwrap().junk, 0);
    let out = build(b"a = \n{ $n } and text\nb = x\n", &[(0, "Neu")]);
    assert_eq!(String::from_utf8(out.clone()).unwrap(), "a = \n    Neu\nb = x\n");
    assert_eq!(structure(&out).unwrap().junk, 0);
    let out = build(b"a = Hi {\n  $n\n} there\r\n", &[(0, "Hallo { $n }")]);
    assert_eq!(structure(&out).unwrap().junk, 0);
    assert_eq!(units(&out).len(), 1);
}

#[test]
fn odd_target_locales_never_panic() {
    let data = std::fs::read(corpus_dir().join("app_en.ftl")).unwrap();
    let mut doc = extract(&data).unwrap();
    pseudo(&mut doc);
    for loc in ["en-\u{f1}ab", "pl-\u{65e5}", "-", "_", "", " ", "pl--PL", "pl_PL_", "zh-Hant-TW", "sr-Latn", "\u{e9}", "pl-x\u{301}y"] {
        let out = FtlFilter.merge_localized(&doc, Some(loc)).unwrap();
        assert_eq!(structure(&out).unwrap().junk, 0, "{loc}");
    }
}
