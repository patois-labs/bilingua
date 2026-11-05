//! Round trips on every corpus file, the 11 languages, encodings, and the target-language stamp.
mod common;
use bilingua_mif::Mif;
use bilingua_model::*;
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(p).unwrap()))
        .collect();
    v.sort();
    v
}

#[test]
fn untouched_and_echo_merges_are_exact() {
    let files = corpus();
    assert!(files.len() >= 8);
    for (name, data) in &files {
        let doc = extract(data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(Mif.merge(&doc).unwrap() == *data, "{name}: untouched");
        assert_identity(data);
    }
}

/// Pseudo-translation: every text piece gets a prefix and a suffix; tags stay. The result reads back with the same
/// units, the same tags (same displays) and the pseudo text.
#[test]
fn pseudo_translated_merge_reads_back() {
    for (name, data) in corpus() {
        let mut doc = extract(&data).unwrap();
        let n = doc.units.len();
        for u in doc.units.iter_mut() {
            let tr: Vec<Inline> = u
                .source()
                .iter()
                .map(|i| match i {
                    Inline::Text(t) => Inline::Text(format!("[{t}]")),
                    tag => tag.clone(),
                })
                .collect();
            u.segments[0].target = tr;
        }
        let out = Mif.merge(&doc).unwrap();
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: reopen {e}"));
        assert_eq!(back.units.len(), n, "{name}");
        for (a, b) in doc.units.iter().zip(&back.units) {
            let want = a.output();
            let got = b.source();
            assert_eq!(want.len(), got.len(), "{name}: {:?} vs {:?}", want, got);
            for (w, g) in want.iter().zip(&got) {
                match (w, g) {
                    (Inline::Text(x), Inline::Text(y)) => assert_eq!(x, y, "{name}"),
                    (Inline::Tag { id: i, kind: k, .. }, Inline::Tag { id: j, kind: l, .. }) => {
                        assert_eq!((i, k), (j, l), "{name}")
                    }
                    _ => panic!("{name}: {w:?} vs {g:?}"),
                }
            }
        }
    }
}

const LANGS: [(&str, &str); 11] = [
    ("en", "USEnglish"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("ja", "Japanese"),
    ("zh-CN", "SimplifiedChinese"),
    ("ko", "Korean"),
    ("ar", "Arabic"),
    ("he", "Hebrew"),
    ("ru", "NoLanguage"),
    ("hi", "NoLanguage"),
];

#[test]
fn eleven_languages_extract_and_translate_between_each_other() {
    let data = std::fs::read(corpus_dir().join("languages.mif")).unwrap();
    let doc = extract(&data).unwrap();
    assert_eq!(doc.units.len(), 11);
    let texts: Vec<String> = doc.units.iter().map(locos_text).collect();
    for (i, (code, _)) in LANGS.iter().enumerate() {
        assert!(texts[i].contains("\u{27e6}g1\u{27e7}"), "{code}: {}", texts[i]);
        assert_eq!(doc.units[i].context.style.as_deref(), Some(format!("Body_{}", code.split('-').next().unwrap()).as_str()));
    }
    // each paragraph receives the next language's text (tags moved along unchanged)
    let map: Vec<(usize, String)> = (0..11).map(|i| (i, texts[(i + 1) % 11].clone())).collect();
    let refs: Vec<(usize, &str)> = map.iter().map(|(i, s)| (*i, s.as_str())).collect();
    let out = build(&data, &refs);
    let back = self::texts(&out);
    for i in 0..11 {
        assert_eq!(back[i], texts[(i + 1) % 11]);
    }
    // the output is valid UTF-8 with the native characters written as they are (MIF 8 and later)
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("ユーザーガイド") && s.contains("مرحبًا") && s.contains("사용자") && s.contains("欢迎"));
}

#[test]
fn target_locale_stamps_the_language_for_each_language() {
    let data = std::fs::read(corpus_dir().join("language-words.mif")).unwrap();
    let t = texts(&data)[0].clone();
    for (loc, kw) in LANGS {
        let out = String::from_utf8(build_for(&data, &[(0, &format!("{t} Z"))], Some(loc))).unwrap();
        assert_eq!(out.matches(&format!("<PgfLanguage {kw}>")).count(), 1, "{loc}");
        assert_eq!(out.matches(&format!("<FLanguage {kw}>")).count(), 2, "{loc}");
    }
}

#[test]
fn bom_and_crlf_are_kept() {
    let data = std::fs::read(corpus_dir().join("suite-crlf-bom.mif")).unwrap();
    assert!(data.starts_with(&[0xEF, 0xBB, 0xBF]) && data.windows(2).any(|w| w == b"\r\n"));
    let out = build(&data, &[(2, "Erste Schritte")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    let s = String::from_utf8(out[3..].to_vec()).unwrap();
    assert!(!s.replace("\r\n", "").contains('\n'), "every newline stays CRLF");
    assert!(s.contains("<String `Erste Schritte'>"));
}

#[test]
fn legacy_escapes_are_read_and_written_as_utf8() {
    // \xNN (Mac Roman based, one space follows) and \uXXXX are read; the rebuilt string is UTF-8
    let src = "<MIFFile 8.00>\n<Para <ParaLine <String `caf\\x8e  \\u65e5\\u672c \\x80  \\x87 end'>>>\n";
    let t = texts(src.as_bytes());
    assert_eq!(t, vec!["caf\u{e9} \u{65e5}\u{672c} \u{c4} \u{e1}end".to_string()]);
    let out = String::from_utf8(build(src.as_bytes(), &[(0, "caf\u{e9} \u{65e5}\u{672c} \u{c4} \u{e1}end!")])).unwrap();
    assert!(out.contains("<String `caf\u{e9} \u{65e5}\u{672c} \u{c4} \u{e1}end!'>"), "{out}");
    // unknown escapes are kept as written
    let src = "<MIFFile 2019>\n<Para <ParaLine <String `a\\zb \\xZZ c'>>>\n";
    assert_eq!(texts(src.as_bytes()), vec!["a\\zb \\xZZ c".to_string()]);
}

#[test]
fn non_utf8_and_old_versions_are_refused_with_a_reason() {
    for enc in [bilingua_model::encoding_rs::SHIFT_JIS, bilingua_model::encoding_rs::WINDOWS_1251] {
        let (b, _, _) = enc.encode("<MIFFile 2019>\n<Para <ParaLine <String `\u{65e5}\u{672c}\u{8a9e} \u{442}\u{435}\u{43a}\u{441}\u{442}'>>>\n");
        match extract(&b) {
            Err(Error::Encoding(m)) => assert!(m.contains("UTF-8")),
            other => panic!("{other:?}"),
        }
    }
    // UTF-16 is not MIF 8 either
    let mut u16 = vec![0xFF, 0xFE];
    for c in "<MIFFile 2019>\n".encode_utf16() {
        u16.extend(c.to_le_bytes());
    }
    assert!(extract(&u16).is_err());
}

#[test]
fn unit_ids_are_stable_and_context_is_filled() {
    let a = extract(SUITE).unwrap();
    let b = extract(SUITE).unwrap();
    let ids: Vec<&str> = a.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, vec!["u1", "u2", "u3", "u4", "u5"]);
    assert_eq!(a, b);
    assert_eq!(a.units[0].context.style.as_deref(), Some("CellBody"));
    assert_eq!(a.units[2].context.kind.as_deref(), Some("paragraph"));
    assert!(a.units[2].context.location.as_deref().unwrap().starts_with("line "));
    // paragraph mode: one segment per unit, as the parsers always give
    assert!(a.units.iter().all(|u| u.segments.len() == 1 && u.translate));
}

#[test]
fn skeleton_from_another_parser_is_refused() {
    let mut d = extract(SUITE).unwrap();
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    }
    assert!(matches!(Mif.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn point_tags_are_standalone_and_pairs_are_open_close() {
    let doc = extract(SUITE).unwrap();
    let inl = doc.units[3].source();
    let kinds: Vec<(String, TagKind)> = inl
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { id, kind, .. } => Some((id.clone(), *kind)),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("1".into(), TagKind::Standalone),
            ("2".into(), TagKind::Open),
            ("2".into(), TagKind::Close),
            ("3".into(), TagKind::Standalone)
        ]
    );
    // ids are the LocOS token numbers, so the model's own conversion gives the LocOS text
    let (text, map) = bilingua_model::locos::to_locos_text(&inl);
    assert!(text.contains("\u{27e6}g2\u{27e7}Save\u{27e6}/g2\u{27e7}"));
    assert_eq!(map.len(), 3);
}
