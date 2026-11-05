//! Round trips, languages, encodings, paragraph mode, locked units, serde, parser stamp, detection.
mod common;
use bilingua_csv::{CsvFilter, TsvFilter};
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

fn call_for(name: &str) -> Call {
    let c = if name.ends_with(".csv") { Call::csv() } else { Call::tsv() };
    c.langs("en", &["de", "fr"], None)
}

fn pseudo(doc: &mut Document) {
    let ids: Vec<(String, String)> = doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), plain_text(&u.source()))).collect();
    for (id, t) in ids {
        doc.set_target(&id, vec![Inline::text(format!("[[{t}]]"))]).unwrap();
    }
}

/// True when every unit is written where it was read (no target column).
fn in_place(doc: &Document) -> bool {
    doc.units.iter().all(|u| {
        let l = u.codes["$csv"].label.clone().unwrap();
        let p: Vec<&str> = l.split(':').collect();
        p[1] == p[2]
    })
}

#[test]
fn corpus_untouched_merge_is_byte_exact() {
    let files = corpus();
    assert!(files.len() >= 30, "corpus has {} files", files.len());
    for (name, data) in &files {
        for locale in [None, Some("de"), Some("fr")] {
            let c = call_for(name).langs("en", &["de", "fr"], locale);
            let doc = c.extract(data).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(c.merge(&doc).unwrap(), *data, "{name} {locale:?}");
        }
        // the facade path: no mapping, no languages
        let out = if name.ends_with(".csv") {
            let d = CsvFilter::default().extract(data, &opts()).unwrap();
            CsvFilter::default().merge(&d).unwrap()
        } else {
            let d = TsvFilter::default().extract(data, &opts()).unwrap();
            TsvFilter::default().merge(&d).unwrap()
        };
        assert_eq!(out, *data, "{name} (facade defaults)");
    }
}

#[test]
fn corpus_pseudo_translation_reads_back() {
    for (name, data) in &corpus() {
        for locale in [None, Some("de")] {
            let c = call_for(name).langs("en", &["de", "fr"], locale);
            let before = c.extract(data).unwrap();
            let src: Vec<String> = before.units.iter().map(|u| plain_text(&u.source())).collect();
            let mut doc = before.clone();
            pseudo(&mut doc);
            let out = c.merge(&doc).unwrap_or_else(|e| panic!("{name}: {e}"));
            let back = c.extract(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
            if name == "unterminated.csv" || src.is_empty() {
                continue; // an open quote swallows the rest of the file: units may differ
            }
            if in_place(&before) {
                assert_eq!(back.units.len(), src.len(), "{name} {locale:?}: unit count");
                for (b, s) in back.units.iter().zip(&src) {
                    assert_eq!(plain_text(&b.source()), format!("[[{s}]]"), "{name}");
                    assert!(b.segments[0].source.iter().all(|i| matches!(i, Inline::Text(_))), "{name}: no inline tags in CSV");
                }
            } else {
                assert_eq!(back.units.iter().map(|u| plain_text(&u.source())).collect::<Vec<_>>(), src, "{name}: sources unchanged");
                assert_ne!(out, *data, "{name}: translations written");
            }
        }
    }
}

#[test]
fn eleven_languages() {
    let langs = ["en", "de", "fr", "es", "ja", "zh", "ko", "ar", "ru", "el", "tr"];
    let dir = corpus_dir();
    let mut seen = Vec::new();
    for l in langs {
        let data = std::fs::read(dir.join(format!("strings_{l}.csv"))).unwrap();
        let c = Call::csv().langs(l, &[], None);
        let u = c.units(&data);
        assert_eq!(u.len(), 2, "{l}: {u:?}");
        assert!(u[1].ends_with(&format!("({l})")));
        seen.push(u[0].clone());
        assert_eq!(c.merge(&c.extract(&data).unwrap()).unwrap(), data);
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 11);
    // the multilingual sheet: every language column is a target of the English column
    let data = std::fs::read(dir.join("multilingual_semicolon_bom.csv")).unwrap();
    for l in ["de", "ja", "ar", "zh", "ko", "ru"] {
        let c = Call::csv().langs("en", &["de", "fr", "ja", "ar", "zh", "ko", "ru", "es", "el", "tr"], Some(l));
        let doc = c.extract(&data).unwrap();
        assert_eq!(doc.units.len(), 3, "{l}");
        let out = c.build(&data, &[(1, "See you")]);
        let text = String::from_utf8(out.clone()).unwrap();
        assert!(text.starts_with('\u{feff}'));
        // the language's own column received the text, the others kept theirs
        let rows: Vec<&str> = text.lines().collect();
        let header: Vec<&str> = rows[0].trim_start_matches('\u{feff}').split(';').collect();
        let col = header.iter().position(|h| *h == l).unwrap();
        let row2: Vec<&str> = rows[2].split(';').collect();
        assert_eq!(row2[col], "See you", "{l}");
    }
    // Arabic and CJK translations of an English file
    let en = std::fs::read(dir.join("strings_en.csv")).unwrap();
    let ar = std::fs::read(dir.join("strings_ar.csv")).unwrap();
    let ar_units = Call::csv().langs("ar", &[], None).units(&ar);
    let mapping: Vec<(usize, &str)> = ar_units.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
    let out = Call::csv().langs("en", &[], None).build(&en, &mapping);
    assert_eq!(Call::csv().langs("ar", &[], None).units(&out), ar_units);
}

#[test]
fn legacy_encodings_keep_their_form() {
    let cases: &[(&str, &str, &str)] = &[
        ("ru", "windows-1251", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43c}\u{438}\u{440}"),
        ("ja", "shift_jis", "\u{30c6}\u{30b9}\u{30c8}\u{3067}\u{3059}"),
        ("zh", "gbk", "\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}"),
        ("ko", "euc-kr", "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}"),
        ("el", "windows-1253", "\u{39a}\u{3b1}\u{3bb}\u{3b7}\u{3bc}\u{3ad}\u{3c1}\u{3b1} \u{3ba}\u{3cc}\u{3c3}\u{3bc}\u{3b5}"),
        ("ar", "windows-1256", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}"),
        ("tr", "windows-1254", "G\u{fc}nayd\u{131}n d\u{fc}nya"),
        ("cs", "windows-1250", "P\u{159}\u{ed}li\u{161} \u{17e}lu\u{165}ou\u{10d}k\u{fd} k\u{16f}\u{148}"),
        ("zh-TW", "big5", "\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}"),
        ("de", "windows-1252", "Gr\u{fc}\u{df}e aus K\u{f6}ln \u{20ac}"),
    ];
    for (lang, label, sentence) in cases {
        let e = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
        let (head, _, _) = e.encode("id,text\n1,");
        let (a, _, bad) = e.encode(sentence);
        assert!(!bad, "{lang}");
        let mut data = head.into_owned();
        data.extend_from_slice(&a);
        data.extend_from_slice(b"\n2,second one\n");
        assert!(std::str::from_utf8(&data).is_err(), "{lang}: legacy bytes");
        let c = Call::csv().langs(lang, &[], None);
        let doc = c.extract(&data).unwrap_or_else(|er| panic!("{lang}: {er}"));
        assert_eq!(plain_text(&doc.units[0].source()), *sentence, "{lang}");
        assert!(doc.warnings.is_empty(), "{lang}: {:?}", doc.warnings);
        assert_eq!(c.merge(&doc).unwrap(), data, "{lang}: untouched");
        // a translation that fits the code page goes back in it
        let fits = format!("{sentence} 2");
        let out = c.build(&data, &[(0, &fits)]);
        let (want, _, _) = e.encode(&fits);
        assert!(out.windows(want.len()).any(|w| w == &*want), "{lang}: written in its code page");
        assert!(!out.starts_with(&[0xEF, 0xBB, 0xBF]));
        // one that does not becomes UTF-8 with a BOM (GB18030 writes every character, so a GBK file never does)
        if *label == "gbk" {
            continue;
        }
        let out = c.build(&data, &[(0, "x \u{1f600}")]);
        assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]), "{lang}");
        assert_eq!(Call::csv().langs("en", &[], None).units(&out)[0], "x \u{1f600}");
    }
}

#[test]
fn bom_and_endings_are_kept() {
    let data = b"\xef\xbb\xbfsource;target\r\nOne;\r\nTwo;\r\n";
    let c = Call::csv().langs("en", &["de"], Some("de"));
    let out = c.build(data, &[(1, "Zwei")]);
    assert_eq!(out, b"\xef\xbb\xbfsource;target\r\nOne;\r\nTwo;Zwei\r\n");
    // a written line break stays a line break; the record's own ending is untouched
    let out = c.build(data, &[(0, "Eins\nund mehr")]);
    assert_eq!(out, b"\xef\xbb\xbfsource;target\r\nOne;\"Eins\nund mehr\"\r\nTwo;\r\n");
    let mixed = b"source,target\nOne,\r\nTwo,\rThree,\n";
    let out = c.build(mixed, &[(1, "Zwei")]);
    assert_eq!(out, b"source,target\nOne,\r\nTwo,Zwei\rThree,\n");
}

#[test]
fn target_locale_and_paragraph_mode() {
    let data = std::fs::read(corpus_dir().join("menu_crlf.csv")).unwrap();
    let o = ExtractOptions {
        source_lang: Some("en".into()),
        target_lang: Some("ar".into()),
        granularity: Granularity::Paragraph,
        include_locked: false,
        ..Default::default()
    };
    let mut doc = Call::csv().extract_with(&data, &o).unwrap();
    assert_eq!(doc.granularity, Granularity::Paragraph);
    assert_eq!(doc.target_lang.as_deref(), Some("ar"));
    assert!(doc.units.iter().all(|u| u.segments.len() == 1 && u.separators.is_empty()));
    pseudo(&mut doc);
    // CSV has no language or direction field: the merge changes cells only
    let out = String::from_utf8(Call::csv().merge(&doc).unwrap()).unwrap();
    assert!(out.starts_with("key,source,target,context,max length\r\n"));
    assert!(out.contains("btn.save,Save,[[Save]],button on the order form,12\r\n"));
    // sentence granularity: the unit's sentences are joined back into the cell
    let sent = ExtractOptions { granularity: Granularity::Sentence, ..o.clone() };
    let mut doc = Call::csv().extract_with(&data, &sent).unwrap();
    bilingua_seg::segment(&mut doc, "en", Granularity::Sentence, bilingua_seg::Rules::bundled());
    assert_eq!(Call::csv().merge(&doc).unwrap(), data, "split units merge back to the same bytes");
    let i = doc.units.iter().position(|u| u.segments.len() > 1).expect("a two-sentence cell");
    for k in 0..doc.units[i].segments.len() {
        let id = doc.units[i].segments[k].id.clone();
        let src = plain_text(&doc.units[i].segments[k].source);
        doc.set_target(&id, vec![Inline::text(format!("<{src}>"))]).unwrap();
    }
    let out = String::from_utf8(Call::csv().merge(&doc).unwrap()).unwrap();
    assert!(out.contains('<') && out.contains('>'));
}

#[test]
fn locked_units_context_and_ids() {
    let data = b"key,source,target,notes,max length\nbtn,Save,,the button,12\nnum,12.50,,,\nf,=SUM(A1),,,\nm,Hello,,,\n";
    let c = Call::csv().langs("en", &["de"], Some("de"));
    assert_eq!(c.units(data), ["Save", "Hello"]);
    let o = ExtractOptions { include_locked: true, source_lang: Some("en".into()), target_lang: Some("de".into()), ..Default::default() };
    let doc = c.extract_with(data, &o).unwrap();
    let flags: Vec<(String, bool)> = doc.units.iter().map(|u| (plain_text(&u.source()), u.translate)).collect();
    assert_eq!(flags, [("Save".into(), true), ("12.50".into(), false), ("=SUM(A1)".into(), false), ("Hello".into(), true)]);
    assert_eq!(c.merge(&doc).unwrap(), data);
    let ids: Vec<&str> = doc.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["u1", "k1", "k2", "u2"], "locked units never move the ids of translatable ones");
    assert_eq!(doc.units[0].context.key.as_deref(), Some("btn"));
    assert_eq!(doc.units[0].context.notes, ["Key: btn", "notes: the button", "Max length: 12"]);
    assert_eq!(doc.units[0].context.max_len, Some(12));
    assert_eq!(doc.units[0].context.extra.get("column").map(String::as_str), Some("source"));
    assert_eq!(doc.units[2].lock_reason.as_deref(), Some("formula"));
    let mut doc = doc;
    let id = doc.units[1].segments[0].id.clone();
    assert!(doc.set_target(&id, vec![Inline::text("x")]).is_err(), "a locked unit refuses a target");
    // the translatable ones still merge
    let id = doc.units[3].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Hallo")]).unwrap();
    assert_eq!(
        c.merge(&doc).unwrap(),
        b"key,source,target,notes,max length\nbtn,Save,,the button,12\nnum,12.50,,,\nf,=SUM(A1),,,\nm,Hello,Hallo,,\n"
    );
}

#[test]
fn serde_round_trips_and_ids_are_stable() {
    let data = std::fs::read(corpus_dir().join("menu_crlf.csv")).unwrap();
    let c = Call::csv().langs("en", &["de"], Some("de"));
    let a = c.extract(&data).unwrap();
    let b = c.extract(&data).unwrap();
    assert_eq!(a, b);
    let mut doc: Document = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(doc, a);
    pseudo(&mut doc);
    let out = c.merge(&doc).unwrap();
    assert_eq!(c.units(&out).len(), a.units.len());
}

#[test]
fn parser_stamp_is_checked() {
    let mut doc = extract(b"source\nHello\n").unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        *parser = Some(ParserInfo::new("bilingua-json", "0.1.0", 1));
    }
    assert!(matches!(CsvFilter::default().merge(&doc), Err(Error::ParserVersion { .. })));
    let mut doc = extract(b"source\nHello\n").unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        parser.as_mut().unwrap().schema += 1;
    }
    assert!(matches!(CsvFilter::default().merge(&doc), Err(Error::ParserVersion { .. })));
}

#[test]
fn detection() {
    let f = CsvFilter::default();
    assert!(f.sniff(b"a,b,c\n1,2,3\n4,5,6\n7,8,9\n"));
    assert!(f.sniff(b"a;b\nc;d\ne;f\n"));
    assert!(f.sniff(b"a\tb\nc\td\ne\tf\n"));
    assert!(!f.sniff(b"just some prose\nwith lines, and a comma\nthat does not align\n"));
    assert!(!f.sniff(b"\x00\x01\x02"));
    assert!(!f.sniff(b"{\"a\": 1, \"b\": 2}\n{\"a\": 1, \"b\": 2}\n{\"a\": 1, \"b\": 2}\n"));
    assert!(!f.sniff(b"a,b\n"));
    assert!(TsvFilter::default().sniff(b"a\tb\nc\td\ne\tf\n"));
    assert!(!TsvFilter::default().sniff(b"a,b\nc,d\ne,f\n"));
    assert!(!f.bilingual());
}

#[test]
fn limits_and_quirks() {
    // empty and header-only files
    assert!(Call::csv().units(b"").is_empty());
    assert!(Call::csv().units(b"key,source,target\r\n").is_empty());
    // a file of only blank lines
    assert!(Call::csv().units(b"\n\n\n").is_empty());
    // too many lines
    let many = "\n".repeat(30);
    let o = ExtractOptions { limits: Some(Limits { max_lines: 20, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Call::csv().extract_with(many.as_bytes(), &o), Err(Error::Limit(_))));
    // too large an input
    let o = ExtractOptions { limits: Some(Limits { max_input_bytes: 10, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Call::csv().extract_with(b"source\nHello world, hello\n", &o), Err(Error::Limit(_))));
    // broken UTF-16 (odd length, lone surrogate) is a typed error; a UTF-32 BOM reads as UTF-16, as LocOS does
    assert!(matches!(Call::csv().extract(b"\xff\xfea\x00b"), Err(Error::Encoding(_))));
    assert!(matches!(Call::csv().extract(b"\xff\xfe\x00\xd8k\x00"), Err(Error::Encoding(_))));
    let _ = Call::csv().extract(b"\xff\xfe\x00\x00a\x00\x00\x00");
    // a quote in the middle of an unquoted field is literal (as Python's csv reads it)
    assert_eq!(Call::csv().units(b"source\nTall 6\" loaf\n"), ["Tall 6\" loaf"]);
    // text after a closing quote stays in the field
    assert_eq!(Call::csv().units(b"source\n\"ab\"cd\n"), ["abcd"]);
    // an unchanged translation writes nothing
    assert_eq!(Call::csv().build(b"source\n\"Hi\"\n", &[(0, "Hi")]), b"source\n\"Hi\"\n");
    // a half-filled target column keeps its text when the unit has no translation of its own
    let c = Call::csv().langs("en", &["de"], Some("de"));
    assert_eq!(c.build(b"source,de\nOne,Eins\nTwo,\n", &[(0, "One"), (1, "Zwei")]), b"source,de\nOne,Eins\nTwo,Zwei\n");
}
