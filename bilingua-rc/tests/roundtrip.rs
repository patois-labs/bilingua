//! Round trips on the oracle corpus and the sample: untouched merges are exact, translated merges read back
//! with the same units and codes, eleven languages in every encoding, the target language written.
mod common;
use bilingua_model::*;
use bilingua_rc::Rc;
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap())
        })
        .collect();
    v.sort();
    assert!(v.len() >= 10);
    v
}

fn tags_of(u: &Unit) -> Vec<String> {
    u.source().iter().filter_map(|i| if let Inline::Tag { display, .. } = i { Some(display.clone()) } else { None }).collect()
}

#[test]
fn corpus_untouched_and_echo_merges_are_exact() {
    for (name, data) in corpus() {
        let doc = extract(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!doc.units.is_empty(), "{name}");
        assert_eq!(Rc.merge(&doc).unwrap(), data, "{name}: untouched");
        assert_identity(&data);
    }
}

#[test]
fn corpus_pseudo_translation_reads_back_with_the_same_codes() {
    for (name, data) in corpus() {
        let mut doc = extract(&data).unwrap();
        let before: Vec<Vec<String>> = doc.units.iter().map(tags_of).collect();
        for u in &mut doc.units {
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            t.push(Inline::text("]"));
            u.segments[0].target = t;
        }
        let out = Rc.merge(&doc).unwrap();
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.units.len(), doc.units.len(), "{name}");
        let after: Vec<Vec<String>> = back.units.iter().map(tags_of).collect();
        assert_eq!(before, after, "{name}: codes");
        for (a, b) in doc.units.iter().zip(&back.units) {
            let want = plain_text(&a.output());
            let got = plain_text(&b.source());
            // the accelerator rule may add or move one `&`
            assert_eq!(want.replace('&', ""), got.replace('&', ""), "{name}: {:?}", a.id);
        }
    }
}

/// A greeting per language (11 with English): Arabic, Japanese, Chinese, Korean, Russian, Greek, Hebrew, Thai,
/// German, French.
const LANGS: &[(&str, &str, &str)] = &[
    ("en-US", "Welcome back", "en"),
    ("ar-SA", "\u{645}\u{631}\u{62d}\u{628}\u{627}\u{64b} \u{628}\u{639}\u{648}\u{62f}\u{62a}\u{643}", "ar"),
    ("ja-JP", "\u{304a}\u{5e30}\u{308a}\u{306a}\u{3055}\u{3044}", "ja"),
    ("zh-CN", "\u{6b22}\u{8fce}\u{56de}\u{6765}", "zh"),
    ("ko-KR", "\u{b2e4}\u{c2dc} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}", "ko"),
    ("ru-RU", "\u{414}\u{43e}\u{431}\u{440}\u{43e} \u{43f}\u{43e}\u{436}\u{430}\u{43b}\u{43e}\u{432}\u{430}\u{442}\u{44c}", "ru"),
    ("el-GR", "\u{39a}\u{3b1}\u{3bb}\u{3ce}\u{3c2} \u{3ae}\u{3c1}\u{3b8}\u{3b1}\u{3c4}\u{3b5}", "el"),
    ("he-IL", "\u{5d1}\u{5e8}\u{5d5}\u{5da} \u{5e9}\u{5d1}\u{5ea}", "he"),
    ("th-TH", "\u{e22}\u{e34}\u{e19}\u{e14}\u{e35}\u{e15}\u{e49}\u{e2d}\u{e19}\u{e23}\u{e31}\u{e1a}", "th"),
    ("de-DE", "Willkommen zur\u{fc}ck", "de"),
    ("fr-FR", "Bienvenue \u{e0} nouveau, caf\u{e9}", "fr"),
];

fn pragma_file(codepage: &str, enc: &'static bilingua_model::encoding_rs::Encoding, text: &str) -> Vec<u8> {
    let src = format!("#pragma code_page({codepage})\r\nSTRINGTABLE\r\nBEGIN\r\n IDS_A \"{text}\"\r\nEND\r\n");
    let (b, _, bad) = enc.encode(&src);
    assert!(!bad);
    b.into_owned()
}


#[test]
fn eleven_languages_in_every_envelope() {
    let sources: Vec<(&str, Vec<u8>)> = vec![
        ("utf-8", "STRINGTABLE\nBEGIN\n IDS_A \"Hello\"\nEND\n".as_bytes().to_vec()),
        ("utf-8-bom", [&[0xEF, 0xBB, 0xBF][..], "STRINGTABLE\r\nBEGIN\r\n IDS_A \"Hello\"\r\nEND\r\n".as_bytes()].concat()),
        ("utf-16le", utf16le("STRINGTABLE\r\nBEGIN\r\n IDS_A \"Hello\"\r\nEND\r\n", true)),
        ("utf-16be", utf16be("STRINGTABLE\r\nBEGIN\r\n IDS_A \"Hello\"\r\nEND\r\n", true)),
        // LocOS finds BOM-less UTF-16 by counting zero bytes, so the file needs enough ASCII around the text
        ("utf-16le-nobom", utf16le(&format!("// {}\r\nSTRINGTABLE\r\nBEGIN\r\n IDS_A \"Hello\"\r\nEND\r\n", "padding ".repeat(80)), false)),
        ("cp1252", pragma_file("1252", bilingua_model::encoding_rs::WINDOWS_1252, "Hello")),
        ("cp1251", pragma_file("1251", bilingua_model::encoding_rs::WINDOWS_1251, "Hello")),
        ("cp1253", pragma_file("1253", bilingua_model::encoding_rs::WINDOWS_1253, "Hello")),
        ("cp1255", pragma_file("1255", bilingua_model::encoding_rs::WINDOWS_1255, "Hello")),
        ("cp874", pragma_file("874", bilingua_model::encoding_rs::WINDOWS_874, "Hello")),
        ("cp932", pragma_file("932", bilingua_model::encoding_rs::SHIFT_JIS, "Hello")),
        ("cp936", pragma_file("936", bilingua_model::encoding_rs::GBK, "Hello")),
        ("cp949", pragma_file("949", bilingua_model::encoding_rs::EUC_KR, "Hello")),
    ];
    for (env, data) in &sources {
        for (loc, text, _) in LANGS {
            let out = build_for(data, &[(0, *text)], Some(loc));
            let reread = extract(&out).unwrap_or_else(|e| panic!("{env} -> {loc}: {e}"));
            assert_eq!(reread.units.len(), 1, "{env} -> {loc}: {:?}", String::from_utf8_lossy(&out));
            assert_eq!(plain_text(&reread.units[0].source()), *text, "{env} -> {loc}");
            // UTF-16 stays UTF-16; the BOM stays; a translation that fits keeps the legacy page
            match *env {
                "utf-16le" | "utf-16be" => assert_eq!(&out[..2], &data[..2], "{env} -> {loc}"),
                "utf-8-bom" => assert_eq!(&out[..3], &[0xEF, 0xBB, 0xBF]),
                "utf-16le-nobom" => assert!(out.len().is_multiple_of(2) && out[1] == 0),
                _ => {}
            }
        }
    }
    // a Greek translation fits cp1253 and the file stays cp1253; Arabic does not and becomes UTF-8 under 65001
    let el = build(&sources.iter().find(|(e, _)| *e == "cp1253").unwrap().1, &[(0, LANGS[6].1)]);
    assert!(std::str::from_utf8(&el).is_err() && String::from_utf8_lossy(&el).contains("code_page(1253)"));
    let ar = build(&sources.iter().find(|(e, _)| *e == "cp1253").unwrap().1, &[(0, LANGS[1].1)]);
    let ars = String::from_utf8(ar).unwrap();
    assert!(ars.contains("code_page(65001)") && !ars.contains("code_page(1253)"));
    // a Japanese translation in a cp932 file stays cp932; Thai does not fit and becomes UTF-8
    let ja = build(&sources.iter().find(|(e, _)| *e == "cp932").unwrap().1, &[(0, LANGS[2].1)]);
    assert!(std::str::from_utf8(&ja).is_err());
    let ru = build(&sources.iter().find(|(e, _)| *e == "cp932").unwrap().1, &[(0, LANGS[8].1)]);
    assert!(String::from_utf8(ru).unwrap().contains("code_page(65001)"));
}

#[test]
fn target_language_ids_for_eleven_locales() {
    let src = SAMPLE.as_bytes();
    let table = [
        ("en-US", "0x09, 0x01", "0409"),
        ("de-DE", "0x07, 0x01", "0407"),
        ("fr-FR", "0x0c, 0x01", "040c"),
        ("ar-SA", "0x01, 0x01", "0401"),
        ("ja-JP", "0x11, 0x01", "0411"),
        ("zh-CN", "0x04, 0x02", "0804"),
        ("zh-TW", "0x04, 0x01", "0404"),
        ("ko-KR", "0x12, 0x01", "0412"),
        ("ru-RU", "0x19, 0x01", "0419"),
        ("el-GR", "0x08, 0x01", "0408"),
        ("he-IL", "0x0d, 0x01", "040d"),
        ("th-TH", "0x1e, 0x01", "041e"),
        ("es_419", "0x0a, 0x01", "040a"),
        ("pt-BR", "0x16, 0x01", "0416"),
        ("pt-PT", "0x16, 0x02", "0816"),
    ];
    for (loc, lang, block) in table {
        let out = String::from_utf8(build_for(src, &[(5, "X")], Some(loc))).unwrap();
        assert!(out.contains(&format!("LANGUAGE {lang}\n")), "{loc}: {}", &out[..500]);
        assert!(out.contains(&format!("BLOCK \"{block}04b0\"")), "{loc}");
    }
    // an unknown language, an empty one and a malformed tag stamp nothing
    for loc in ["xx-YY", "", "en us", "<b>"] {
        let out = String::from_utf8(build_for(src, &[(5, "X")], Some(loc))).unwrap();
        assert!(out.contains("LANGUAGE LANG_ENGLISH, SUBLANG_ENGLISH_US"), "{loc}");
    }
}

#[test]
fn nothing_translated_means_the_original_bytes_even_with_a_target_language() {
    for (name, data) in corpus() {
        let mut doc = extract(&data).unwrap();
        doc.target_lang = Some("de-DE".into());
        assert_eq!(Rc.merge(&doc).unwrap(), data, "{name}");
    }
}

#[test]
fn utf8_file_with_non_ascii_text_gets_a_pragma_only_when_edited() {
    let src = "STRINGTABLE\nBEGIN\n IDS_A \"Caf\u{e9}\"\n IDS_B \"Tea\"\nEND\n";
    assert_eq!(build(src.as_bytes(), &[]), src.as_bytes());
    let out = String::from_utf8(build(src.as_bytes(), &[(1, "Th\u{e9}")])).unwrap();
    assert!(out.starts_with("#pragma code_page(65001)\n"), "{out}");
    // reading it back gives UTF-8 and no second pragma on the next edit
    let again = String::from_utf8(build(out.as_bytes(), &[(1, "Tee")])).unwrap();
    assert_eq!(again.matches("code_page").count(), 1);
}

#[test]
fn locked_units_on_request_stay_untouched() {
    let src = "STRINGTABLE\nBEGIN\n IDS_A \"%d%%\"\n IDS_B \"Tea\"\n IDS_C \"\"\nEND\n";
    let doc = Rc.extract(src.as_bytes(), &ExtractOptions { include_locked: true, ..Default::default() }).unwrap();
    assert_eq!(doc.units.len(), 2);
    assert!(!doc.units[0].translate && doc.units[0].lock_reason.is_some() && doc.units[1].translate);
    assert_eq!(doc.units[0].id, "k1");
    let mut doc = doc;
    doc.units[1].segments[0].target = vec![Inline::text("Th\u{e9}")];
    let out = String::from_utf8(Rc.merge(&doc).unwrap()).unwrap();
    assert!(out.contains("IDS_A \"%d%%\"") && out.contains("IDS_C \"\"") && out.contains("Th\u{e9}"));
    // the default skips them, as LocOS does
    assert_eq!(extract(src.as_bytes()).unwrap().units.len(), 1);
}

#[test]
fn segments_of_one_unit_are_joined_on_merge() {
    let src = "STRINGTABLE\nBEGIN\n IDS_A \"One. Two.\"\nEND\n";
    let mut doc = extract(src.as_bytes()).unwrap();
    let u = &mut doc.units[0];
    u.segments = vec![
        Segment { id: "u1#1".into(), source: vec![Inline::text("One.")], target: vec![Inline::text("Un.")], state: SegState::Draft, origin: None },
        Segment { id: "u1#2".into(), source: vec![Inline::text("Two.")], target: vec![Inline::text("Deux.")], state: SegState::Draft, origin: None },
    ];
    u.separators = vec![" ".into()];
    let out = String::from_utf8(Rc.merge(&doc).unwrap()).unwrap();
    assert!(out.contains("\"Un. Deux.\""), "{out}");
}

#[test]
fn foreign_skeleton_is_refused() {
    let mut doc = extract(SAMPLE.as_bytes()).unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    }
    assert!(matches!(Rc.merge(&doc), Err(Error::ParserVersion { .. })));
}

#[test]
fn sniffing() {
    assert!(Rc.sniff(SAMPLE.as_bytes()));
    assert!(Rc.sniff(&utf16le(SAMPLE, true)));
    assert!(Rc.sniff(b"IDD_X DIALOGEX 0, 0, 10, 10\nBEGIN\nEND\n"));
    assert!(Rc.sniff(b"STRINGTABLE\nBEGIN\n1 \"a\"\nEND\n"));
    assert!(!Rc.sniff(b"[General]\nkey=value\n"));
    assert!(!Rc.sniff(b"{\"a\": 1}"));
    assert!(!Rc.sniff(b"# just a note\nhello\n"));
    assert!(!Rc.sniff(b""));
}

#[test]
fn unit_context_and_ids_are_stable() {
    let a = extract(SAMPLE.as_bytes()).unwrap();
    let b = extract(SAMPLE.as_bytes()).unwrap();
    assert_eq!(a.units.iter().map(|u| &u.id).collect::<Vec<_>>(), b.units.iter().map(|u| &u.id).collect::<Vec<_>>());
    assert_eq!(a.units[0].context.kind.as_deref(), Some("menu"));
    assert_eq!(a.units[0].context.location.as_deref(), Some("line 19"));
    assert_eq!(a.units[5].context.kind.as_deref(), Some("caption"));
    assert_eq!(a.format, "rc");
}

#[test]
fn json_round_trip_of_the_document_merges_the_same() {
    let doc = extract(SAMPLE.as_bytes()).unwrap();
    let j = serde_json::to_string(&doc).unwrap();
    let doc2: Document = serde_json::from_str(&j).unwrap();
    assert_eq!(Rc.merge(&doc2).unwrap(), SAMPLE.as_bytes());
}

#[test]
fn ansi_pages_that_python_and_the_web_disagree_on() {
    // Python's cp932 reads 0xA0 as U+F8F0; LocOS accepts the file, so do we, and the bytes come back
    let data = b"#pragma code_page(932)\nSTRINGTABLE\nBEGIN\n 1 \"a\xa0b\"\n 2 \"x\"\nEND\n".to_vec();
    assert_identity(&data);
    // an undefined byte of the declared page is refused with its place
    let bad = b"#pragma code_page(1253)\nSTRINGTABLE\nBEGIN\n 1 \"a\xaab\"\nEND\n";
    match extract(bad) {
        Err(Error::Encoding(m)) => assert!(m.contains("1253") && m.contains("0xaa"), "{m}"),
        other => panic!("{other:?}"),
    }
    // true ISO-8859-1 (code page 28591) reads the C1 range as controls, like Python's latin-1
    let l1 = b"#pragma code_page(28591)\nSTRINGTABLE\nBEGIN\n 1 \"caf\xe9 \x93x\x94\"\nEND\n".to_vec();
    assert!(units(&l1)[0].starts_with("caf\u{e9} \u{93}x"));
    assert_identity(&l1);
}

#[test]
fn pragma_number_is_trimmed_like_python_bytes_strip() {
    // LocOS strips the number with bytes.strip(), which also drops VT and FF
    let data = b"#pragma code_page(\x0b 1251\x0c)\nSTRINGTABLE\nBEGIN\n 1 \"\xcf\xf0\xe8\"\nEND\n".to_vec();
    assert_eq!(units(&data), vec!["\u{41f}\u{440}\u{438}".to_string()]);
    assert_identity(&data);
}
