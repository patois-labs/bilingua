//! Opus gate attacks: markup and line injection through translations, column handling, header stamping, encodings,
//! merge options and speed on pathological lines.
mod common;
use bilingua_ass::AssFilter;
use bilingua_model::*;
use common::*;

const FMT: &str = "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text";

fn one(text: &str) -> String {
    format!("[Script Info]\nTitle: Show\n\n[Events]\n{FMT}\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{text}\nComment: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,note\n")
}

fn set_text(data: &[u8], unit: usize, inl: Vec<Inline>, mo: &MergeOptions) -> String {
    let mut doc = extract(data).unwrap();
    let id = doc.units[unit].segments[0].id.clone();
    doc.set_target(&id, inl).unwrap();
    s(AssFilter.merge_with(&doc, mo).unwrap())
}

/// Every line of `out` but the translated one equals the source; returns the translated line's Text field.
fn translated_text(src: &str, out: &str, lineno: usize) -> String {
    let a: Vec<&str> = src.split('\n').collect();
    let b: Vec<&str> = out.split('\n').collect();
    assert_eq!(a.len(), b.len(), "line count changed: {out:?}");
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        if i != lineno {
            assert_eq!(x, y, "line {i} changed");
        }
    }
    let l = b[lineno];
    l.splitn(10, ',').nth(9).unwrap().to_string()
}

#[test]
fn typed_markup_never_becomes_an_override_or_escape() {
    let src = one("Hello {\\i1}there{\\i0} friend");
    let attacks = [
        "{\\an8}Up{\\pos(1,1)}",
        "a\\Nb\\nc\\hd",
        "trailing\\",
        "\\{not\\}",
        "x}{\\p1}m 0 0 l 9 9",
        "{",
        "}{\\r}",
        "\\\\N",
    ];
    for at in attacks {
        let out = set_text(src.as_bytes(), 1, vec![Inline::text(at)], &MergeOptions::default());
        let t = translated_text(&src, &out, 5);
        assert!(!t.contains('{'), "{at:?} -> {t:?}");
        for esc in ["\\N", "\\n", "\\h", "\\{", "\\}"] {
            assert!(!t.contains(esc), "{at:?} -> {t:?}");
        }
        assert!(!t.ends_with('\\'), "{at:?} -> {t:?}");
        let back = extract(out.as_bytes()).unwrap();
        assert_eq!(back.units.len(), 2, "{at:?}");
        // the unit reads back as plain text: no code was made
        assert!(back.units[1].source().iter().all(|i| matches!(i, Inline::Text(_))), "{at:?} -> {t:?}");
    }
    // a backslash right before a kept code (its own or the trailing skeleton) is made safe too
    let src2 = one("{\\an8}Hello {\\i1}there{\\i0}\\N");
    let doc = extract(src2.as_bytes()).unwrap();
    let src_inl = doc.units[1].source();
    let mut t: Vec<Inline> = vec![Inline::text("A\\")];
    t.extend(src_inl.iter().filter(|i| !matches!(i, Inline::Text(_))).cloned());
    t.push(Inline::text("B\\"));
    let out = set_text(src2.as_bytes(), 1, t, &MergeOptions::default());
    let line = out.split('\n').nth(5).unwrap();
    // the emptied pair is dropped, so the two runs meet: `\B` is no escape; the last backslash (before `\N`) is
    assert!(line.ends_with(",,{\\an8}A\\B\u{ff3c}\\N"), "{line:?}");
}

#[test]
fn typed_line_breaks_never_make_a_line() {
    let src = one("Hello");
    for at in ["a\nDialogue: 0,0:00:09.00,0:00:10.00,Default,,0,0,0,,EVIL", "a\r\n[Script Info]\r\nTitle: X", "a\rb", "a\u{2028}b\u{85}c\u{c}d"] {
        let out = set_text(src.as_bytes(), 1, vec![Inline::text(at)], &MergeOptions::default());
        let back = extract(out.as_bytes()).unwrap();
        assert_eq!(back.units.len(), 2, "{at:?} -> {out:?}");
        assert_eq!(out.matches('\n').count(), src.matches('\n').count(), "{at:?}");
        assert!(!out.contains('\r'));
    }
    // the Title is one line: a break (raw or typed `\N`) is a space, and it cannot open a section or a header
    for at in ["Part\nTwo", "a\r\n[Events]\r\nDialogue: 0,0,0,Default,,0,0,0,,EVIL", "x\\Ny", "a\rLanguage: xx"] {
        let out = set_text(src.as_bytes(), 0, vec![Inline::text(at)], &MergeOptions::default());
        let back = extract(out.as_bytes()).unwrap();
        assert_eq!(back.units.len(), 2, "{at:?} -> {out:?}");
        assert_eq!(out.matches('\n').count(), src.matches('\n').count(), "{at:?}");
        assert!(out.lines().nth(1).unwrap().starts_with("Title: "));
    }
}

#[test]
fn invented_and_misused_tags_write_nothing_new() {
    let src = one("{\\an8}One {\\b1}two{\\b0} three\\Nfour");
    let data = src.as_bytes();
    let doc = extract(data).unwrap();
    let u = &doc.units[1];
    let codes: Vec<(String, TagKind)> = u
        .source()
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { id, kind, .. } => Some((id.clone(), *kind)),
            _ => None,
        })
        .collect();
    assert_eq!(codes.len(), 3, "{codes:?}"); // open, close, `\N`
    let br = codes.iter().find(|c| c.1 == TagKind::Standalone).unwrap().0.clone();
    let pair = codes.iter().find(|c| c.1 == TagKind::Open).unwrap().0.clone();
    // a close before its open, a doubled open and a doubled standalone, the ~raw code named as a tag
    let mut d = extract(data).unwrap();
    let id = d.units[1].segments[0].id.clone();
    let t = vec![
        Inline::tag(pair.clone(), TagKind::Close, "x"),
        Inline::text("A"),
        Inline::tag(br.clone(), TagKind::Standalone, "x"),
        Inline::tag(br.clone(), TagKind::Standalone, "x"),
        Inline::tag(pair.clone(), TagKind::Open, "x"),
        Inline::text("B"),
        Inline::tag(pair.clone(), TagKind::Close, "x"),
        Inline::tag("~raw", TagKind::Standalone, "x"),
    ];
    // the model may refuse a doubled tag; when it does not, the writer writes each code once
    if d.set_target(&id, t).is_ok() {
        let out = s(AssFilter.merge(&d).unwrap());
        let line = out.split('\n').nth(5).unwrap();
        assert_eq!(line.matches("\\N").count(), 1, "{line:?}");
        assert!(!line.contains("One {\\b1}"), "{line:?}");
    }
}

#[test]
fn text_column_follows_the_format_line() {
    // Text in the middle: no unit; Text last after a short Format: the commas belong to the text
    let src = "[Events]\nFormat: Start, End, Text\nDialogue: 0:00:01.00,0:00:02.00,Hello, world, again\nFormat: Text, Start\nDialogue: Hi,0\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,1,2,S,N,0,0,0,,a,b\nDialogue: 0,1,2,S,N,0,0,0\n";
    let u = units(src.as_bytes());
    assert_eq!(u, vec!["Hello, world, again", "a,b"]);
    let out = s(build(src.as_bytes(), &[(0, "Bonjour, monde"), (1, "x, y, z")]));
    assert!(out.contains("Dialogue: 0:00:01.00,0:00:02.00,Bonjour, monde\n"));
    assert!(out.contains("Dialogue: 0,1,2,S,N,0,0,0,,x, y, z\n"));
    assert!(out.contains("Dialogue: Hi,0\n"));
    // a translation full of commas reads back as one Text field
    let back = units(out.as_bytes());
    assert_eq!(back, vec!["Bonjour, monde", "x, y, z"]);
}

#[test]
fn script_info_language_stamp() {
    let src = "[Script Info]\r\nTitle: Show\r\nScriptType: v4.00+\r\n\r\n[V4+ Styles]\r\nLanguage: keep\r\n[Events]\r\nDialogue: 0,0,0,Default,,0,0,0,,Hello\r\n";
    let data = src.as_bytes();
    let mut doc = extract(data).unwrap();
    doc.target_lang = Some("pt-BR".into());
    // nothing translated, default options: the exact bytes
    assert_eq!(AssFilter.merge(&doc).unwrap(), data);
    // stamp always: Language added after the header with the file's line end, other sections untouched
    let mo = MergeOptions { stamp_target_always: true, ..Default::default() };
    let out = s(AssFilter.merge_with(&doc, &mo).unwrap());
    assert!(out.starts_with("[Script Info]\r\nLanguage: pt\r\nTitle: Show\r\n"), "{out:?}");
    assert!(out.contains("[V4+ Styles]\r\nLanguage: keep\r\n"));
    // something translated: stamped by default
    let id = doc.units[1].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Olá")]).unwrap();
    let out2 = s(AssFilter.merge(&doc).unwrap());
    assert_eq!(out2.matches("Language: pt").count(), 1);
    // an existing header is rewritten in place, keeping its spacing; a three-letter target drops it
    let src3 = "[Script Info]\nlanguage :  en  \nTitle: Show\n[Events]\nDialogue: 0,0,0,Default,,0,0,0,,Hello\n";
    let o3 = s(build_localized(src3.as_bytes(), &[(1, "Hallo")], Some("de-DE")));
    assert!(o3.starts_with("[Script Info]\nlanguage :  de  \nTitle"), "{o3:?}");
    let o4 = s(build_localized(src3.as_bytes(), &[(1, "Hallo")], Some("fil-PH")));
    assert!(!o4.to_lowercase().contains("language"), "{o4:?}");
    let o5 = s(build_localized(src3.as_bytes(), &[(1, "Hallo")], Some("not a tag!")));
    assert!(o5.contains("language :  en  \n"), "{o5:?}");
    // a translated Title cannot fake a Language header
    let o6 = s(build_localized(src3.as_bytes(), &[(0, "x\nLanguage: zz")], Some("de")));
    assert_eq!(o6.lines().filter(|l| l.to_lowercase().starts_with("language")).count(), 1, "{o6:?}");
    assert!(o6.contains("\nTitle: x Language: zz\n"), "{o6:?}");
}

fn legacy(enc: &'static Encoding, lang: &str, text: &str, tr: &str) {
    let src = one(text);
    let (bytes, _, bad) = enc.encode(&src);
    assert!(!bad);
    let bytes = bytes.into_owned();
    let o = ExtractOptions { source_lang: Some(lang.into()), ..Default::default() };
    let mut doc = AssFilter.extract(&bytes, &o).unwrap();
    assert_eq!(doc.units[1].source(), vec![Inline::text(text)], "{}", enc.name());
    assert_eq!(AssFilter.merge(&doc).unwrap(), bytes, "{} untouched", enc.name());
    let id = doc.units[1].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text(tr)]).unwrap();
    let out = AssFilter.merge(&doc).unwrap();
    let want_text = src.replace(text, tr);
    let (want, _, _) = enc.encode(&want_text);
    assert_eq!(out, want.into_owned(), "{} keeps its code page", enc.name());
    let utf = AssFilter.merge_with(&doc, &MergeOptions { output_encoding: OutputEncoding::Utf8, ..Default::default() }).unwrap();
    assert_eq!(s(utf), src.replace(text, tr));
}

use bilingua_model::encoding_rs::{Encoding, EUC_KR, GBK, SHIFT_JIS, WINDOWS_1252};

#[test]
fn legacy_cjk_and_western_code_pages() {
    legacy(SHIFT_JIS, "ja-JP", "日本語の字幕です", "翻訳された字幕");
    legacy(GBK, "zh-CN", "中文字幕", "翻译字幕");
    legacy(EUC_KR, "ko-KR", "한국어 자막", "번역 자막");
    legacy(WINDOWS_1252, "fr-FR", "Café crème", "Thé glacé");
}

#[test]
fn utf16_and_line_ends_survive_a_translation() {
    let body = "[Script Info]\r\nTitle: Show\n[Events]\rDialogue: 0,0,0,Default,,0,0,0,,Hello\r\nDialogue: 0,0,0,Default,,0,0,0,,Bye";
    for (bom, le) in [(&[0xFF, 0xFE][..], true), (&[0xFE, 0xFF][..], false)] {
        let mut data = bom.to_vec();
        for c in body.encode_utf16() {
            data.extend_from_slice(&if le { c.to_le_bytes() } else { c.to_be_bytes() });
        }
        assert_identity(&data);
        let out = build(&data, &[(1, "Hallo"), (2, "Tschüss")]);
        let want = body.replace("Hello", "Hallo").replace("Bye", "Tschüss");
        let mut w = bom.to_vec();
        for c in want.encode_utf16() {
            w.extend_from_slice(&if le { c.to_le_bytes() } else { c.to_be_bytes() });
        }
        assert_eq!(out, w);
    }
    // CR only, with a BOM and no final line end
    let cr = "\u{feff}[Events]\rDialogue: 0,0,0,Default,,0,0,0,,Hello\rDialogue: 0,0,0,Default,,0,0,0,,Bye";
    assert_identity(cr.as_bytes());
    assert_eq!(s(build(cr.as_bytes(), &[(1, "Ciao\nmondo")])), cr.replace("Bye", "Ciao\\Nmondo"));
}

#[test]
fn locked_comments_are_capped_too() {
    let big = format!("[Events]\nComment: 0,0,0,Default,,0,0,0,,{}x\nComment: 0,0,0,Default,,0,0,0,,small\nDialogue: 0,0,0,Default,,0,0,0,,Hi\n", "{\\i1}".repeat(30_000));
    let o = ExtractOptions { include_locked: true, ..Default::default() };
    let t = std::time::Instant::now();
    let doc = extract_with(big.as_bytes(), &o).unwrap();
    assert!(t.elapsed().as_secs() < 5);
    let ids: Vec<(&str, bool)> = doc.units.iter().map(|u| (u.id.as_str(), u.translate)).collect();
    assert_eq!(ids, vec![("l3", false), ("l4", true)]);
    assert_eq!(AssFilter.merge(&doc).unwrap(), big.as_bytes());
}

#[test]
fn pathological_lines_stay_fast() {
    let head = "[Events]\nDialogue: 0,0,0,Default,,0,0,0,,";
    let cases = [
        format!("{head}{}x{}\n", "{\\i1}".repeat(19_000), "{\\b0}".repeat(1_000)),
        format!("{head}{}x{}\n", "{\\u1}".repeat(10_000), "{\\i1}{\\i1\\b0}".repeat(4_000)),
        format!("{head}x{}\n", "{\\p".repeat(30_000)),
        format!("{head}x{{\\p{}1}}\n", " ".repeat(99_000)),
        format!("{head}{}\n", "\\".repeat(99_000)),
        format!("[Script Info]\n{}", "Title: x\n".repeat(49_000)),
        format!("[Events]\n{}", "Dialogue: 0,0,0,Default,,0,0,0,,a {\\i1}b{\\i0} c\\Nd\n".repeat(49_000)),
    ];
    for c in &cases {
        let t = std::time::Instant::now();
        if let Ok(mut doc) = extract(c.as_bytes()) {
            let items: Vec<(String, Vec<Inline>)> = doc.units.iter().map(|u| (u.segments[0].id.clone(), u.source())).collect();
            doc.set_targets(items).unwrap();
            doc.target_lang = Some("de".into());
            let _ = AssFilter.merge(&doc).unwrap();
        }
        assert!(t.elapsed().as_secs() < 10, "{:?} for {:?}", t.elapsed(), &c[..80.min(c.len())]);
    }
}
