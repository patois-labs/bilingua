//! Round trips: untouched merge gives the original bytes in every encoding, a pseudo-translated merge reads back with
//! the same units and tags, eleven languages, legacy code pages, paragraph mode, locked units, the target language.
mod common;
use bilingua_adoc::Adoc;
use bilingua_model::*;
use bilingua_model::encoding_rs;
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
    v
}

fn tag_count(doc: &Document) -> usize {
    doc.units.iter().flat_map(|u| u.source()).filter(|i| matches!(i, Inline::Tag { .. })).count()
}

/// Every translatable unit becomes `prefix + source` (tags kept).
fn pseudo(doc: &mut Document, prefix: &str) {
    let ids: Vec<(String, Vec<Inline>)> = doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), u.source())).collect();
    for (id, src) in ids {
        let mut t = vec![Inline::text(prefix)];
        t.extend(src);
        doc.set_target(&id, t).unwrap();
    }
}

#[test]
fn every_corpus_file_round_trips() {
    let files = corpus();
    assert!(files.len() >= 10);
    for (name, bytes) in &files {
        let doc = extract(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!doc.units.is_empty(), "{name}");
        assert_eq!(Adoc.merge(&doc).unwrap(), *bytes, "{name}: untouched merge");
        // paragraph mode: one segment per unit
        assert!(doc.units.iter().all(|u| u.segments.len() == 1), "{name}");
        let mut t = doc.clone();
        t.target_lang = Some("de-DE".into());
        pseudo(&mut t, "Zz ");
        let out = Adoc.merge(&t).unwrap();
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: merged output unreadable: {e}"));
        assert_eq!(back.units.len(), doc.units.len(), "{name}: unit count");
        assert_eq!(tag_count(&back), tag_count(&doc), "{name}: tags");
        let text = if bytes.starts_with(&[0xFF, 0xFE]) { encoding_rs::UTF_16LE.decode(&out).0.into_owned() } else { String::from_utf8_lossy(&out).into_owned() };
        assert!(text.contains(":lang: de-DE"), "{name}: language not written");
        assert!(text.contains("Zz "), "{name}");
        // the encoding and the line endings are kept
        let nl_in = enc::newline_style(&String::from_utf8_lossy(bytes));
        if bytes.starts_with(&[0xFF, 0xFE]) {
            assert!(out.starts_with(&[0xFF, 0xFE]), "{name}: UTF-16 BOM kept");
        } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]), "{name}: UTF-8 BOM kept");
        }
        if nl_in == "\r\n" && !bytes.starts_with(&[0xFF, 0xFE]) {
            assert!(!text.replace("\r\n", "").contains('\n'), "{name}: CRLF kept");
        }
    }
}

#[test]
fn eleven_languages_translate_and_read_back() {
    let base = std::fs::read(corpus_dir().join("orders_en.adoc")).unwrap();
    let samples = [
        ("fr-FR", "Bienvenue dans notre boulangerie"),
        ("de-DE", "Willkommen in unserer Bäckerei"),
        ("es-ES", "Bienvenidos a nuestra panadería"),
        ("pt-BR", "Bem-vindo à nossa padaria"),
        ("ru-RU", "Добро пожаловать в нашу пекарню"),
        ("ar-SA", "مرحبا بكم في مخبزنا"),
        ("he-IL", "ברוכים הבאים למאפייה שלנו"),
        ("ja-JP", "ようこそベーカリーへ"),
        ("zh-CN", "欢迎来到我们的面包店"),
        ("ko-KR", "저희 베이커리에 오신 것을 환영합니다"),
        ("hi-IN", "हमारी बेकरी में आपका स्वागत है"),
    ];
    assert_eq!(samples.len(), 11);
    for (lang, text) in samples {
        let mut doc = extract(&base).unwrap();
        doc.target_lang = Some(lang.into());
        let ids: Vec<(String, Vec<Inline>)> = doc.units.iter().map(|u| (u.segments[0].id.clone(), u.source())).collect();
        for (id, src) in ids {
            // every piece of text becomes the sample; the tags stay where they were
            let t: Vec<Inline> = src.into_iter().map(|i| if matches!(i, Inline::Text(_)) { Inline::text(format!(" {text} ")) } else { i }).collect();
            doc.set_target(&id, t).unwrap();
        }
        let out = Adoc.merge(&doc).unwrap();
        let s = String::from_utf8(out.clone()).unwrap();
        assert!(s.contains(text), "{lang}");
        assert!(s.contains(&format!(":lang: {lang}\n")), "{lang}: {s}");
        let back = extract(&out).unwrap();
        // listing, include and attribute entries survive in every language
        assert!(s.contains("print(\"not translated\")") && s.contains("include::partials/legal.adoc[]"), "{lang}");
        assert_eq!(back.units.len(), doc.units.len(), "{lang}");
    }
}

#[test]
fn legacy_code_pages_keep_their_bytes() {
    let src = "= Titre du guide\n\nCeci est un paragraphe de caf\u{e9}.\n";
    let cases: [(&str, &'static Encoding, &str, &str); 5] = [
        ("ja", bilingua_model::encoding_rs::SHIFT_JIS, "= \u{30ac}\u{30a4}\u{30c9}\n\n\u{3053}\u{308c}\u{306f}\u{6bb5}\u{843d}\u{3067}\u{3059}\u{3002}\n", "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{6bb5}\u{843d}\u{3002}"),
        ("zh-CN", bilingua_model::encoding_rs::GBK, "= \u{6307}\u{5357}\n\n\u{8fd9}\u{662f}\u{4e00}\u{4e2a}\u{6bb5}\u{843d}\u{3002}\n", "\u{4e2d}\u{6587}\u{6bb5}\u{843d}\u{3002}"),
        ("ko", bilingua_model::encoding_rs::EUC_KR, "= \u{c548}\u{b0b4}\n\n\u{c774}\u{ac83}\u{c740} \u{b2e8}\u{b77d}\u{c785}\u{b2c8}\u{b2e4}.\n", "\u{d55c}\u{ad6d}\u{c5b4} \u{b2e8}\u{b77d}."),
        ("ru", bilingua_model::encoding_rs::WINDOWS_1251, "= \u{420}\u{443}\u{43a}\u{43e}\u{432}\u{43e}\u{434}\u{441}\u{442}\u{432}\u{43e}\n\n\u{42d}\u{442}\u{43e} \u{430}\u{431}\u{437}\u{430}\u{446}.\n", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43c}\u{438}\u{440}."),
        ("en", bilingua_model::encoding_rs::WINDOWS_1252, src, "Caf\u{e9} cr\u{e8}me."),
    ];
    use bilingua_model::encoding_rs::Encoding;
    for (lang, enc_, text, tr) in cases {
        let (bytes, _, bad) = enc_.encode(text);
        assert!(!bad, "{lang}");
        let o = ExtractOptions { source_lang: Some(lang.into()), ..Default::default() };
        let mut doc = Adoc.extract(&bytes, &o).unwrap_or_else(|e| panic!("{lang}: {e}"));
        assert!(doc.units.len() >= 2, "{lang}");
        assert_eq!(Adoc.merge(&doc).unwrap(), bytes.as_ref(), "{lang}: untouched");
        let id = doc.units[1].segments[0].id.clone();
        doc.set_target(&id, vec![Inline::text(tr)]).unwrap();
        let out = Adoc.merge(&doc).unwrap();
        let expect = text.replacen(&plain_text(&doc.units[1].source()), tr, 1);
        let (want, _, bad2) = enc_.encode(&expect);
        assert!(!bad2, "{lang}");
        assert_eq!(out, want.as_ref(), "{lang}: written in the file's code page");
        // a character the code page lacks makes the file UTF-8
        doc.set_target(&id, vec![Inline::text("emoji \u{1f35e} bread")]).unwrap();
        let out = Adoc.merge(&doc).unwrap();
        let dec = if enc_ == encoding_rs::GBK { encoding_rs::GB18030 } else { enc_ };
        let s = String::from_utf8(out.clone()).unwrap_or_else(|_| dec.decode(&out).0.into_owned());
        assert!(s.contains("\u{1f35e}"), "{lang}");
    }
}

#[test]
fn utf16_and_bom_round_trip() {
    let text = "= Titel\n\nEin Absatz mit *Fettschrift*.\n";
    let mut le = vec![0xFF, 0xFE];
    for u in text.encode_utf16() {
        le.extend_from_slice(&u.to_le_bytes());
    }
    let mut be = vec![0xFE, 0xFF];
    for u in text.encode_utf16() {
        be.extend_from_slice(&u.to_be_bytes());
    }
    for data in [le, be, [b"\xef\xbb\xbf".as_slice(), text.as_bytes()].concat(), text.replace('\n', "\r\n").into_bytes()] {
        assert_identity(&data);
        let out = build(&data, &[(1, "Un paragraphe")]);
        for bom in [&[0xFFu8, 0xFE][..], &[0xFE, 0xFF], &[0xEF, 0xBB, 0xBF]] {
            if data.starts_with(bom) {
                assert!(out.starts_with(bom), "BOM kept");
            }
        }
        assert!(extract(&out).is_ok());
    }
}

#[test]
fn locked_units_and_the_skeleton() {
    let src = "== 2026\n\nText here.\n\n* 100\n\n:attr: value\n";
    let o = ExtractOptions { include_locked: true, ..Default::default() };
    let doc = Adoc.extract(src.as_bytes(), &o).unwrap();
    assert_eq!(doc.units.iter().filter(|u| !u.translate).count(), 2);
    assert_eq!(Adoc.merge(&doc).unwrap(), src.as_bytes());
    let plain = extract(src.as_bytes()).unwrap();
    assert_eq!(plain.units.len(), 1);
    // context: a unit knows its kind and its heading
    let d = extract(b"== Intro\n\nBody text.\n\n* an item\n").unwrap();
    let kinds: Vec<_> = d.units.iter().map(|u| u.context.kind.clone().unwrap()).collect();
    assert_eq!(kinds, ["heading", "paragraph", "list-item"]);
    assert_eq!(d.units[1].context.heading.as_deref(), Some("Intro"));
    assert_eq!(d.units[1].context.location.as_deref(), Some("line 3"));
}

#[test]
fn invalid_skeleton_is_refused() {
    let mut doc = extract(b"Hello world.\n").unwrap();
    doc.skeleton = Skeleton::zip(vec![], ParserInfo::new("bilingua-adoc", env!("CARGO_PKG_VERSION"), 1));
    assert!(Adoc.merge(&doc).is_err());
}
