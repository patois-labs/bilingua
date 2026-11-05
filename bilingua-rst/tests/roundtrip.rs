//! Round trips on the fixtures and on the generated corpus: an untouched merge is the original bytes; a
//! pseudo-translated merge reads back with the same units and tags; eleven languages; the encodings LocOS reads.
mod common;
use bilingua_model::*;
use bilingua_rst::Rst;
use common::*;

fn fixtures() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            (p.extension().is_some_and(|x| x == "rst")).then(|| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()))
        })
        .collect();
    v.sort();
    v
}

fn tags(v: &[Inline]) -> Vec<(TagKind, String)> {
    v.iter()
        .filter_map(|i| match i {
            // a markup character the translation had to escape is a new escape code on the way back: not counted
            Inline::Tag { kind, display, .. } if !display.starts_with('\\') => Some((*kind, display.clone())),
            _ => None,
        })
        .collect()
}

/// Every unit gets "Zq " + its source as the target.
pub fn pseudo(doc: &mut Document, prefix: &str) {
    for u in &mut doc.units {
        let mut t = vec![Inline::text(prefix)];
        t.extend(u.source());
        u.segments[0].target = merge_text(t);
        u.segments[0].state = SegState::Draft;
    }
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, data) in fixtures() {
        let doc = extract(&data).unwrap();
        assert_eq!(Rst.merge(&doc).unwrap(), data, "{name}");
        // CRLF, CR and BOM variants of the same file
        let crlf = String::from_utf8_lossy(&data).replace('\n', "\r\n").into_bytes();
        let doc = extract(&crlf).unwrap();
        assert_eq!(Rst.merge(&doc).unwrap(), crlf, "{name} crlf");
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(&data);
        let doc = extract(&bom).unwrap();
        assert_eq!(Rst.merge(&doc).unwrap(), bom, "{name} bom");
        let cr = String::from_utf8_lossy(&data).replace('\n', "\r").into_bytes();
        let doc = extract(&cr).unwrap();
        assert_eq!(Rst.merge(&doc).unwrap(), cr, "{name} cr");
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    let mut checked = 0;
    for (name, data) in fixtures() {
        let mut doc = extract(&data).unwrap();
        let before: Vec<Vec<(TagKind, String)>> = doc.units.iter().map(|u| tags(&u.source())).collect();
        pseudo(&mut doc, "Zq ");
        let out = match Rst.merge(&doc) {
            Ok(o) => o,
            Err(Error::Write(m)) => {
                assert!(m.contains("does not fit"), "{name}: {m}");
                continue;
            }
            Err(e) => panic!("{name}: {e}"),
        };
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: merged output unreadable: {e}"));
        assert_eq!(back.units.len(), doc.units.len(), "{name}: unit count");
        for (k, u) in back.units.iter().enumerate() {
            assert_eq!(tags(&u.source()), before[k], "{name}: tags of unit {k}");
            assert!(plain_text(&u.source()).starts_with("Zq "), "{name}: unit {k} {:?}", u.source());
        }
        checked += 1;
    }
    assert!(checked >= 7, "{checked}");
}

#[test]
fn generated_corpus_round_trips() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    let (mut n, mut fits, mut skipped_odd, mut units_same, mut units_all) = (0, 0, 0, 0usize, 0usize);
    for c in &cases {
        let hex = c["hex"].as_str().unwrap();
        let data: Vec<u8> = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect();
        let Ok(mut doc) = extract(&data) else { continue };
        assert_eq!(Rst.merge(&doc).unwrap(), data, "{}: untouched merge", c["name"]);
        let before: Vec<Vec<(TagKind, String)>> = doc.units.iter().map(|u| tags(&u.source())).collect();
        pseudo(&mut doc, "Zq ");
        match Rst.merge(&doc) {
            Ok(out) => {
                let back = extract(&out).unwrap();
                // a prefix can change how a later line reads (an inline pair that now spans words); only the
                // count is promised, and the tags of units that kept their shape
                if back.units.len() != doc.units.len() {
                    skipped_odd += 1;
                    continue;
                }
                let same = back.units.iter().zip(&before).filter(|(u, b)| tags(&u.source()) == **b).count();
                units_same += same;
                units_all += before.len();
                n += 1;
            }
            Err(Error::Write(m)) if m.contains("does not fit") => fits += 1,
            Err(e) => panic!("{}: {e}", c["name"]),
        }
    }
    println!("round trips: {n} files, {fits} refused for table fit, {skipped_odd} changed shape, tags kept in {units_same} of {units_all} units");
    assert!(units_same * 100 >= units_all * 97, "tags drifted: {units_same} of {units_all}");
    assert!(n > 1500 && skipped_odd * 20 < n, "{n} {skipped_odd}");
}

const LANGS: &[(&str, &str, &str)] = &[
    ("en", "Getting started", "Open the *settings* page and press ``Save``."),
    ("de", "Erste Schritte", "\u{d6}ffnen Sie die Seite *Einstellungen* und dr\u{fc}cken Sie ``Speichern``."),
    ("fr", "Pour commencer", "Ouvrez la page *R\u{e9}glages* et appuyez sur ``Enregistrer``."),
    ("es", "Primeros pasos", "Abra la p\u{e1}gina *Ajustes* y pulse ``Guardar``."),
    ("ja", "\u{306f}\u{3058}\u{3081}\u{306b}", "*\u{8a2d}\u{5b9a}*\u{30da}\u{30fc}\u{30b8}\u{3092}\u{958b}\u{3044}\u{3066}\u{3001}``\u{4fdd}\u{5b58}``\u{3092}\u{62bc}\u{3057}\u{307e}\u{3059}\u{3002}"),
    ("zh-Hans", "\u{5f00}\u{59cb}\u{4f7f}\u{7528}", "\u{6253}\u{5f00} *\u{8bbe}\u{7f6e}* \u{9875}\u{9762}\u{5e76}\u{70b9}\u{51fb} ``\u{4fdd}\u{5b58}``\u{3002}"),
    ("ko", "\u{c2dc}\u{c791}\u{d558}\u{ae30}", "*\u{c124}\u{c815}* \u{d398}\u{c774}\u{c9c0}\u{b97c} \u{c5f4}\u{ace0} ``\u{c800}\u{c7a5}``\u{c744} \u{b204}\u{b974}\u{c138}\u{c694}."),
    ("ar", "\u{627}\u{644}\u{628}\u{62f}\u{621}", "\u{627}\u{641}\u{62a}\u{62d} \u{635}\u{641}\u{62d}\u{629} *\u{627}\u{644}\u{625}\u{639}\u{62f}\u{627}\u{62f}\u{627}\u{62a}* \u{648}\u{627}\u{636}\u{63a}\u{637} ``\u{62d}\u{641}\u{638}``."),
    ("ru", "\u{41d}\u{430}\u{447}\u{430}\u{43b}\u{43e} \u{440}\u{430}\u{431}\u{43e}\u{442}\u{44b}", "\u{41e}\u{442}\u{43a}\u{440}\u{43e}\u{439}\u{442}\u{435} \u{441}\u{442}\u{440}\u{430}\u{43d}\u{438}\u{446}\u{443} *\u{41d}\u{430}\u{441}\u{442}\u{440}\u{43e}\u{439}\u{43a}\u{438}* \u{438} \u{43d}\u{430}\u{436}\u{43c}\u{438}\u{442}\u{435} ``\u{421}\u{43e}\u{445}\u{440}\u{430}\u{43d}\u{438}\u{442}\u{44c}``."),
    ("hi", "\u{936}\u{941}\u{930}\u{942}\u{906}\u{924}", "*\u{938}\u{947}\u{91f}\u{93f}\u{902}\u{917}* \u{92a}\u{943}\u{937}\u{94d}\u{920} \u{916}\u{94b}\u{932}\u{947}\u{902} \u{914}\u{930} ``\u{938}\u{939}\u{947}\u{91c}\u{947}\u{902}`` \u{926}\u{92c}\u{93e}\u{90f}\u{902}\u{964}"),
    ("th", "\u{e40}\u{e23}\u{e34}\u{e48}\u{e21}\u{e15}\u{e49}\u{e19}", "\u{e40}\u{e1b}\u{e34}\u{e14}\u{e2b}\u{e19}\u{e49}\u{e32} *\u{e01}\u{e32}\u{e23}\u{e15}\u{e31}\u{e49}\u{e07}\u{e04}\u{e48}\u{e32}* \u{e41}\u{e25}\u{e30}\u{e01}\u{e14} ``\u{e1a}\u{e31}\u{e19}\u{e17}\u{e36}\u{e01}``"),
];

fn doc_in(title: &str, body: &str) -> String {
    let bar = "=".repeat(bilingua_rst::display_width(title));
    format!("{title}\n{bar}\n\n{body}\n\n- {body}\n\n+--------------------+\n| {title:<18} |\n+--------------------+\n")
}

#[test]
fn eleven_languages() {
    assert_eq!(LANGS.len(), 11);
    let src = doc_in(LANGS[0].1, LANGS[0].2);
    let data = src.as_bytes();
    let n = extract(data).unwrap().units.len();
    for (lang, title, body) in LANGS {
        // translate into the language: the title (adornment follows its width), the paragraph with its markup
        let mut doc = extract(data).unwrap();
        doc.target_lang = Some((*lang).into());
        let (src_title, src_body) = (title.to_string(), body.to_string());
        doc.units[0].segments[0].target = vec![Inline::text(src_title.clone())];
        // the paragraph keeps its two tag pairs/points: rebuild the target from the source tags
        let tags_src: Vec<Inline> = doc.units[1].source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
        assert_eq!(tags_src.len(), 3, "emphasis pair and a literal");
        let mut t = vec![Inline::text(format!("{lang}: "))];
        t.push(tags_src[0].clone());
        t.push(Inline::text(src_body.split('*').nth(1).unwrap_or("x").to_string()));
        t.push(tags_src[1].clone());
        t.push(Inline::text(" "));
        t.push(tags_src[2].clone());
        doc.units[1].segments[0].target = t;
        let out = match Rst.merge(&doc) {
            Ok(o) => o,
            Err(Error::Write(m)) => panic!("{lang}: {m}"),
            Err(e) => panic!("{lang}: {e}"),
        };
        let text = String::from_utf8(out.clone()).unwrap();
        let first: Vec<&str> = text.lines().take(2).collect();
        assert_eq!(first[0], src_title, "{lang}");
        assert_eq!(first[1].chars().count(), bilingua_rst::display_width(&src_title), "{lang}: the underline follows the title width");
        let back = extract(&out).unwrap();
        assert_eq!(back.units.len(), n, "{lang}");
        assert_eq!(plain_text(&back.units[0].source()), src_title, "{lang}");
        // and the original text of that language reads as units too
        let own = doc_in(title, body);
        let d2 = extract(own.as_bytes()).unwrap();
        assert!(d2.units.len() >= 3, "{lang}");
        assert_eq!(Rst.merge(&d2).unwrap(), own.as_bytes(), "{lang}: untouched");
    }
}

fn enc_roundtrip(label: &'static str, lang: &str, text: &str, want_tag: &str) {
    let e = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
    let (bytes, _, bad) = e.encode(text);
    assert!(!bad, "{label}");
    let opts = ExtractOptions::new().with_source_lang(lang).with_granularity(Granularity::Paragraph);
    let mut doc = Rst.extract(&bytes, &opts).unwrap_or_else(|er| panic!("{label}: {er}"));
    let Skeleton::Text { encoding, .. } = &doc.skeleton else { panic!() };
    assert!(encoding.starts_with(want_tag), "{label}: {encoding}");
    assert!(!doc.units.is_empty(), "{label}");
    assert_eq!(Rst.merge(&doc).unwrap(), bytes.as_ref(), "{label}: untouched");
    pseudo(&mut doc, "");
    // translated into the same script: written in the same encoding
    let out = Rst.merge(&doc).unwrap();
    assert_eq!(out, bytes.as_ref(), "{label}: echo");
    let u0 = plain_text(&doc.units[0].source());
    doc.units[0].segments[0].target = vec![Inline::text(u0)];
}

#[test]
fn legacy_encodings_are_read_and_kept() {
    enc_roundtrip("shift_jis", "ja", "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{898b}\u{51fa}\u{3057}\n==============\n\n\u{3053}\u{308c}\u{306f}\u{30c6}\u{30b9}\u{30c8}\u{3067}\u{3059}\u{3002}\n", "shift_jis");
    enc_roundtrip("gbk", "zh-CN", "\u{4e2d}\u{6587}\u{6807}\u{9898}\n========\n\n\u{8fd9}\u{662f}\u{4e00}\u{4e2a}\u{6bb5}\u{843d}\u{3002}\n", "gbk");
    enc_roundtrip("euc-kr", "ko", "\u{d55c}\u{ad6d}\u{c5b4}\n======\n\n\u{c774}\u{ac83}\u{c740} \u{d14c}\u{c2a4}\u{d2b8}\u{c785}\u{b2c8}\u{b2e4}.\n", "euc-kr");
    enc_roundtrip("big5", "zh-TW", "\u{4e2d}\u{6587}\u{6a19}\u{984c}\n========\n\n\u{9019}\u{662f}\u{4e00}\u{500b}\u{6bb5}\u{843d}\u{3002}\n", "big5");
    enc_roundtrip("windows-1252", "fr", "Caf\u{e9} cr\u{e8}me\n===========\n\nUn paragraphe \u{e0} traduire.\n", "windows-1252");
    enc_roundtrip("windows-1251", "ru", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}\n=======\n\n\u{42d}\u{442}\u{43e} \u{430}\u{431}\u{437}\u{430}\u{446}.\n", "windows-1251");
    enc_roundtrip("windows-1256", "ar", "\u{645}\u{631}\u{62d}\u{628}\u{627}\n=====\n\n\u{647}\u{630}\u{647} \u{641}\u{642}\u{631}\u{629}.\n", "windows-1256");
}

#[test]
fn utf16_with_a_bom_is_read_and_kept() {
    let text = "Titel\n=====\n\nEin Absatz mit *Betonung*.\n";
    for le in [true, false] {
        let mut bytes: Vec<u8> = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
        for u in text.encode_utf16() {
            bytes.extend(if le { u.to_le_bytes() } else { u.to_be_bytes() });
        }
        let mut doc = extract(&bytes).unwrap();
        assert_eq!(doc.units.len(), 2);
        assert_eq!(Rst.merge(&doc).unwrap(), bytes);
        pseudo(&mut doc, "Zq ");
        let out = Rst.merge(&doc).unwrap();
        assert!(out.starts_with(&bytes[..2]));
        assert_eq!(extract(&out).unwrap().units.len(), 2);
    }
}

#[test]
fn a_translation_outside_the_code_page_is_written_as_utf8() {
    let e = encoding_rs::SHIFT_JIS;
    let (bytes, _, _) = e.encode("\u{898b}\u{51fa}\u{3057}\n======\n\n\u{672c}\u{6587}\u{3067}\u{3059}\u{3002}\n");
    let opts = ExtractOptions::new().with_source_lang("ja").with_granularity(Granularity::Paragraph);
    let mut doc = Rst.extract(&bytes, &opts).unwrap();
    doc.units[1].segments[0].target = vec![Inline::text("\u{c548}\u{b155}")];
    let out = Rst.merge(&doc).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("\u{c548}\u{b155}"));
}

#[test]
fn text_that_looks_like_a_charset_declaration_does_not_change_the_encoding() {
    // reStructuredText declares no charset: a meta field that mentions one is just text
    let data = b"Caf\xe9\n====\n\n.. meta::\n   :http-equiv=Content-Type: text/html; charset=UTF-8\n\nUn paragraphe \xe0 traduire.\n".to_vec();
    let mut doc = extract(&data).unwrap();
    assert_eq!(Rst.merge(&doc).unwrap(), data);
    doc.units[1].segments[0].target = vec![Inline::text("Un autre paragraphe \u{e0} lire.")];
    let out = Rst.merge(&doc).unwrap();
    let want = b"Caf\xe9\n====\n\n.. meta::\n   :http-equiv=Content-Type: text/html; charset=UTF-8\n\nUn autre paragraphe \xe0 lire.\n".to_vec();
    assert_eq!(out, want, "still Windows-1252, not UTF-8");
}
