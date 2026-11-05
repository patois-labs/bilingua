//! Round trips, languages, encodings, paragraph mode, locked units, serde, parser stamp.
mod common;
use bilingua_ini::{IniFilter, Limits};
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

fn pseudo(doc: &mut Document) {
    let ids: Vec<(String, String)> =
        doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), plain_text(&u.source()))).collect();
    for (id, t) in ids {
        doc.set_target(&id, vec![Inline::text(format!("[[{t}]]"))]).unwrap();
    }
}

#[test]
fn corpus_untouched_merge_is_byte_exact() {
    let files = corpus();
    assert!(files.len() >= 20, "corpus has {} files", files.len());
    for (name, data) in &files {
        let doc = extract(data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(IniFilter.merge(&doc).unwrap(), *data, "{name}");
        assert_identity(data);
    }
}

#[test]
fn corpus_pseudo_translation_reads_back() {
    for (name, data) in &corpus() {
        let mut doc = extract(data).unwrap();
        let before = units(data);
        pseudo(&mut doc);
        let out = IniFilter.merge(&doc).unwrap();
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.units.len(), before.len(), "{name}: unit count");
        for (b, src) in back.units.iter().zip(&before) {
            assert_eq!(plain_text(&b.source()), format!("[[{src}]]"), "{name}");
            assert!(b.codes.keys().all(|k| k.starts_with('$')), "{name}: no inline tags in INI");
            assert!(b.segments[0].source.iter().all(|i| matches!(i, Inline::Text(_))));
        }
        // keys, comments and sections are untouched: same record keys
        let k1: Vec<_> = bilingua_ini::records(data).unwrap().into_iter().map(|r| (r.section, r.key, r.comment)).collect();
        let k2: Vec<_> = bilingua_ini::records(&out).unwrap().into_iter().map(|r| (r.section, r.key, r.comment)).collect();
        assert_eq!(k1, k2, "{name}");
    }
}

#[test]
fn eleven_languages() {
    let langs = ["en", "de", "fr", "es", "ja", "zh", "ko", "ar", "ru", "pt", "tr"];
    let dir = corpus_dir();
    let mut seen = Vec::new();
    for l in langs {
        let data = std::fs::read(dir.join(format!("joomla_{l}.ini"))).unwrap();
        let u = units(&data);
        assert_eq!(u.len(), 5, "{l}: {u:?}");
        assert!(u.iter().all(|t| !t.is_empty()));
        assert_eq!(u[2].matches('%').count(), 2, "{l}: placeholders kept as text");
        assert!(u[1].contains('"'), "{l}: escaped quote resolved");
        seen.push(u[0].clone());
        assert_identity(&data);
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 11);
    // Arabic and CJK as translations of the English file, in every encoding the format has.
    let en = std::fs::read(dir.join("joomla_en.ini")).unwrap();
    let ar = std::fs::read_to_string(dir.join("joomla_ar.ini")).unwrap();
    let ar_units = units(ar.as_bytes());
    let mapping: Vec<(usize, &str)> = ar_units.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
    let out = build(&en, &mapping);
    assert_eq!(units(&out), ar_units);
    for l in ["ja", "zh", "ko"] {
        let d = std::fs::read(dir.join(format!("generic_{l}.ini"))).unwrap_or_default();
        if !d.is_empty() {
            assert_identity(&d);
        }
    }
}

#[test]
fn encodings_keep_their_form() {
    let dir = corpus_dir();
    let be = std::fs::read(dir.join("generic_ko_utf16be.ini")).unwrap();
    assert!(be.starts_with(&[0xFE, 0xFF]));
    let out = build(&be, &[(0, "제목")]);
    assert!(out.starts_with(&[0xFE, 0xFF]));
    assert_eq!(units(&out)[0], "제목");
    let le = std::fs::read(dir.join("generic_zh_utf16le.ini")).unwrap();
    assert!(build(&le, &[(0, "标题")]).starts_with(&[0xFF, 0xFE]));
    let bom = std::fs::read(dir.join("generic_de_utf8bom.ini")).unwrap();
    assert!(build(&bom, &[(0, "Titel")]).starts_with(&[0xEF, 0xBB, 0xBF]));
    // Windows-1252 stays Windows-1252 while every character fits (euro sign is 0x80), else becomes UTF-8.
    let cp = std::fs::read(dir.join("generic_fr_cp1252.ini")).unwrap();
    assert!(std::str::from_utf8(&cp).is_err());
    let out = build(&cp, &[(0, "Prix \u{20ac} \u{e9}t\u{e9}")]);
    assert!(out.windows(6).any(|w| w == b"Prix \x80"), "euro written as 0x80");
    let out = build(&cp, &[(0, "\u{65e5}\u{672c}")]);
    assert!(std::str::from_utf8(&out).is_ok());
    assert_eq!(units(&out)[0], "\u{65e5}\u{672c}");
    // newlines are kept as they were
    let mixed = std::fs::read(dir.join("mixed_newlines.ini")).unwrap();
    assert_eq!(units(&mixed), vec!["One", "Two", "Three"]);
    let out = build(&mixed, &[(1, "Zwei")]);
    assert_eq!(out, b"[S]\r\na=One\nb=Zwei\rc=Three\r\n");
}

#[test]
fn target_locale_and_paragraph_mode() {
    let data = std::fs::read(corpus_dir().join("joomla_en.ini")).unwrap();
    let opts = ExtractOptions {
        source_lang: Some("en".into()),
        target_lang: Some("ar".into()),
        granularity: Granularity::Paragraph,
        include_locked: false, ..Default::default() };
    let mut doc = IniFilter.extract(&data, &opts).unwrap();
    assert_eq!(doc.granularity, Granularity::Paragraph);
    assert_eq!(doc.target_lang.as_deref(), Some("ar"));
    assert!(doc.units.iter().all(|u| u.segments.len() == 1 && u.separators.is_empty()));
    pseudo(&mut doc);
    // INI has no language or direction field: the merge changes values only.
    let out = String::from_utf8(IniFilter.merge(&doc).unwrap()).unwrap();
    assert!(out.starts_with("; Lumen Bakery language file (en)\n"));
    assert!(out.contains("COM_LUMEN_LOGIN=\"[[Log in]]\""));
}

#[test]
fn locked_units_and_context() {
    let data = b"[S]\n; the note\nA=%d\nB=---\nC=Hello\nC=Hello again\nD=\"\"\n";
    assert_eq!(units(data), vec!["Hello", "Hello again"]);
    let opts = ExtractOptions { include_locked: true, ..Default::default() };
    let doc = IniFilter.extract(data, &opts).unwrap();
    let flags: Vec<(String, bool)> = doc.units.iter().map(|u| (plain_text(&u.source()), u.translate)).collect();
    assert_eq!(
        flags,
        vec![("%d".into(), false), ("---".into(), false), ("Hello".into(), true), ("Hello again".into(), true)]
    );
    assert_eq!(IniFilter.merge(&doc).unwrap(), data);
    assert_eq!(doc.units[0].context.notes, vec!["the note".to_string()]);
    assert_eq!(doc.units[0].context.extra.get("section").map(String::as_str), Some("S"));
    assert_eq!(doc.units[3].context.extra.get("duplicate").map(String::as_str), Some("true"));
    assert_eq!(doc.units[2].context.key.as_deref(), Some("C"));
    // a locked unit refuses a target
    let mut doc = doc;
    let id = doc.units[0].segments[0].id.clone();
    assert!(doc.set_target(&id, vec![Inline::text("x")]).is_err());
}

#[test]
fn ids_are_stable_and_serde_round_trips() {
    let data = std::fs::read(corpus_dir().join("generic_en.ini")).unwrap();
    let a = extract(&data).unwrap();
    let b = extract(&data).unwrap();
    assert_eq!(a, b);
    let ids: Vec<&str> = a.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, vec!["u1", "u2", "u3", "u4", "u5"]);
    let mut doc: Document = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(doc, a);
    pseudo(&mut doc);
    let out = IniFilter.merge(&doc).unwrap();
    assert_eq!(units(&out).len(), 5);
}

#[test]
fn parser_stamp_is_checked() {
    let mut doc = extract(b"a=Hello\n").unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        *parser = Some(ParserInfo::new("bilingua-json", "0.1.0", 1));
    }
    assert!(matches!(IniFilter.merge(&doc), Err(Error::ParserVersion { .. })));
}

#[test]
fn detection() {
    let f = IniFilter;
    assert_eq!(f.id(), "ini");
    assert_eq!(f.extensions(), &["ini"]);
    assert!(f.sniff(b"[General]\ntitle = App\n"));
    assert!(f.sniff(b"COM_A=\"One\"\nCOM_B=\"Two\"\n"));
    assert!(!f.sniff(b"just some prose\nwith lines\n"));
    assert!(!f.sniff(b"\x00\x01\x02"));
    assert!(!f.sniff(b"{\"a\": 1}"));
    assert!(!f.bilingual());
}

#[test]
fn limits_and_quirks() {
    // a value over the character cap
    let long = format!("k={}\n", "x".repeat(101));
    let tight = Limits { max_value_chars: 100, ..Limits::default() };
    assert!(IniFilter.extract_with(long.as_bytes(), &ExtractOptions::default(), &tight).is_err());
    // too many lines (non-translatable lines count too)
    let many = "\n".repeat(30);
    let tight = Limits { max_lines: 20, ..Limits::default() };
    assert!(IniFilter.extract_with(many.as_bytes(), &ExtractOptions::default(), &tight).is_err());
    // UTF-32 and broken UTF-16 are typed errors
    assert!(matches!(extract(b"\xff\xfe\x00\x00a\x00\x00\x00"), Err(Error::Unsupported(_))));
    assert!(extract(b"\xff\xfea\x00b").is_err());
    // empty file
    assert!(units(b"").is_empty());
    // unchanged translation keeps the original raw spelling (escapes and padding)
    let src = b"k=\"Say \\\"hi\\\"\"  \n";
    assert_eq!(build(src, &[(0, "Say \"hi\"")]), src);
    // an escaped quote typed by a translator is not doubled
    assert_eq!(build(b"k=\"One\"\n", &[(0, "A \\\"b\\\"")]), b"k=\"A \\\"b\\\"\"\n");
    // unquoted values take a quote as it is
    assert_eq!(build(b"k=One\n", &[(0, "A \"b\"")]), b"k=A \"b\"\n");
}
