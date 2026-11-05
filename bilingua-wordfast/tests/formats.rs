//! Every fixture in `tests/data`: round trips, the 11 languages, each encoding, the target-language stamp.

mod common;

use bilingua_model::*;
use common::*;

include!("data_list.inc");

fn filter_for(name: &str, targets: bool) -> Box<dyn Filter> {
    match (name.ends_with(".txlf"), targets) {
        (true, true) => Box::new(TXLF_T),
        (true, false) => Box::new(TXLF),
        (false, true) => Box::new(TXML_T),
        (false, false) => Box::new(TXML),
    }
}

fn tag_count(d: &Document) -> usize {
    d.units.iter().map(|u| u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

fn encoding(d: &Document) -> String {
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => enc::base(encoding).to_string(),
        _ => String::new(),
    }
}

fn show(v: &[Inline]) -> String {
    locos::to_locos_text(v).0
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, data) in FILES {
        for targets in [false, true] {
            let f = filter_for(name, targets);
            for locked in [false, true] {
                let o = ExtractOptions { include_locked: locked, ..opts() };
                let d = f.extract(data, &o).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(f.merge(&d).unwrap(), *data, "{name}");
                // every unit's own target written back unchanged is still the original bytes
                let mut e = d.clone();
                for u in e.units.iter_mut().filter(|u| u.translate) {
                    let t = u.segments[0].target.clone();
                    u.segments[0].target = t;
                }
                assert_eq!(f.merge(&e).unwrap(), *data, "{name}: echo");
            }
        }
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (name, data) in FILES {
        let f = filter_for(name, true);
        let before = extract(f.as_ref(), data);
        let mut d = before.clone();
        for u in d.units.iter_mut().filter(|u| u.translate) {
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            t.push(Inline::text("]"));
            u.segments[0].target = t;
        }
        let out = f.merge(&d).unwrap_or_else(|e| panic!("{name}: {e}"));
        let after = extract(f.as_ref(), &out);
        assert_eq!(after.units.len(), before.units.len(), "{name}");
        assert_eq!(tag_count(&after), tag_count(&before), "{name}");
        assert_eq!(encoding(&after), encoding(&before), "{name}: encoding kept");
        for (a, b) in after.units.iter().zip(&before.units) {
            assert_eq!(a.id, b.id, "{name}: ids are stable");
            assert_eq!(show(&a.source()), show(&b.source()), "{name}: sources do not change");
            if b.translate {
                assert_eq!(show(&a.output()), format!("[{}]", show(&b.source())), "{name} unit {}", b.id);
            } else {
                assert_eq!(show(&a.output()), show(&b.output()), "{name}: protected unit {} untouched", b.id);
            }
        }
        // and the writer's own output is stable: merging what was read back changes nothing
        let again = f.merge(&after).unwrap();
        assert_eq!(again, out, "{name}: second merge");
    }
}

#[test]
fn eleven_languages() {
    let langs = ["en", "de", "fr", "es", "pt", "ru", "tr", "ar", "ja", "zh", "ko"];
    for l in langs {
        for ext in ["txml", "txlf"] {
            let name = format!("lang_{l}.{ext}");
            let data = FILES.iter().find(|(n, _)| *n == name).unwrap().1;
            let f = filter_for(&name, false);
            let d = extract(f.as_ref(), data);
            let want = if ext == "txml" { 4 } else { 3 };
            assert_eq!(d.units.len(), want, "{name}");
            let text = show(&d.units[0].source());
            assert!(!text.is_empty() && text.chars().any(|c| c.is_alphabetic()), "{name}: {text}");
            assert!(d.source_lang.is_some());
            // translate every unit with text in the same language and read it back
            let mut e = d.clone();
            for u in e.units.iter_mut() {
                let mut t = vec![Inline::text(format!("{l}: "))];
                t.extend(u.source());
                u.segments[0].target = t;
            }
            let out = f.merge(&e).unwrap();
            let back = extract(filter_for(&name, true).as_ref(), &out);
            for b in d.units.iter().filter(|u| u.translate) {
                let a = back.units.iter().find(|u| u.id == b.id).expect("same id");
                assert!(show(&a.output()).starts_with(&format!("{l}: ")), "{name}");
            }
        }
    }
}

#[test]
fn encodings_are_kept() {
    for (name, data) in FILES {
        let want = if name.contains("utf16le") {
            "utf-16le"
        } else if name.contains("utf16be") {
            "utf-16be"
        } else if name.contains("utf8bom") {
            "utf-8-bom"
        } else {
            "utf-8"
        };
        let d = extract(filter_for(name, false).as_ref(), data);
        assert_eq!(encoding(&d), want, "{name}");
    }
    // a translation with characters of every script goes back in the same encoding
    for name in ["lang_ar_utf16le.txml", "lang_ko_utf16be.txml", "lang_ja_utf8bom_crlf.txml", "lang_ar_utf16le.txlf", "lang_ko_utf16be.txlf", "lang_ja_utf8bom_crlf.txlf"] {
        let data = FILES.iter().find(|(n, _)| *n == name).unwrap().1;
        let f = filter_for(name, false);
        let mut d = extract(f.as_ref(), data);
        d.units[0].segments[0].target = vec![Inline::text("\u{65e5}\u{672c}\u{8a9e} \u{645}\u{631}\u{62d}\u{628}\u{627} \u{d55c}\u{ad6d}\u{c5b4} \u{1f600}")];
        let out = f.merge(&d).unwrap();
        let first = &out[..3];
        match encoding(&d).as_str() {
            "utf-16le" => assert_eq!(&out[..2], &[0xFF, 0xFE]),
            "utf-16be" => assert_eq!(&out[..2], &[0xFE, 0xFF]),
            _ => assert_eq!(first, &[0xEF, 0xBB, 0xBF]),
        }
        let back = extract(filter_for(name, true).as_ref(), &out);
        assert!(back.units.iter().any(|u| show(&u.output()).contains("\u{1f600}")), "{name}");
        if name.contains("crlf") {
            let n = |v: &[u8]| v.windows(2).filter(|w| *w == b"\r\n").count();
            let bare = out.iter().enumerate().filter(|(i, b)| **b == b'\n' && (*i == 0 || out[i - 1] != b'\r')).count();
            assert_eq!(bare, 0, "{name}: every newline stays CRLF");
            assert!(n(&out) >= n(data));
        }
    }
}

const LANGS: [(&str, &str, &str); 11] = [
    ("en-US", "en_US", "en-US"),
    ("de-DE", "de_DE", "de-DE"),
    ("fr-FR", "fr_FR", "fr-FR"),
    ("es-ES", "es_ES", "es-ES"),
    ("pt-BR", "pt_BR", "pt-BR"),
    ("ru-RU", "ru_RU", "ru-RU"),
    ("tr-TR", "tr_TR", "tr-TR"),
    ("ar-SA", "ar_SA", "ar-SA"),
    ("ja-JP", "ja_JP", "ja-JP"),
    ("zh-CN", "zh_CN", "zh-CN"),
    ("ko-KR", "ko_KR", "ko-KR"),
];

#[test]
fn target_language_is_declared_for_each_of_the_11_languages() {
    let txml = String::from_utf8_lossy(FILES.iter().find(|(n, _)| *n == "lang_en.txml").unwrap().1).replace(" targetlocale=\"xx_XX\"", "");
    let txlf = String::from_utf8_lossy(FILES.iter().find(|(n, _)| *n == "lang_en.txlf").unwrap().1).replace(" target-language=\"xx-XX\"", "");
    for (tag, under, hyph) in LANGS {
        let o = ExtractOptions { target_lang: Some(tag.replace('-', "_")), ..opts() };
        let mut d = TXML.extract(txml.as_bytes(), &o).unwrap();
        set(&mut d, 5, "x");
        let out = merged(&TXML, &d);
        assert!(contains(&out, &format!("targetlocale=\"{under}\"")), "{tag}");
        let mut d = TXLF.extract(txlf.as_bytes(), &o).unwrap();
        set(&mut d, 0, "x");
        let out = merged(&TXLF, &d);
        assert!(contains(&out, &format!("target-language=\"{hyph}\"")), "{tag}");
        // the stamped file reads back with that language
        let back = TXLF_T.extract(&out, &opts()).unwrap();
        assert_eq!(back.target_lang.as_deref(), Some(hyph));
    }
}
