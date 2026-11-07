//! Round trips, languages, metadata, fallbacks and segmentation modes.

use bilingua_arb::Arb;
use bilingua_model::*;
use serde_json::Value;

const LANGS: &str = include_str!("data/languages.arb");

fn extract(text: &str) -> Document {
    Arb.extract(text.as_bytes(), &ExtractOptions::default()).unwrap()
}

/// Sets every unit's target to prefix + source, by position (no per-call search).
fn translate_all(doc: &mut Document, prefix: &str) {
    for u in doc.units.iter_mut() {
        for s in u.segments.iter_mut() {
            let mut t = vec![Inline::text(prefix)];
            t.extend(s.source.clone());
            s.target = merge_text(t);
            s.state = SegState::Draft;
        }
    }
}

fn tag_shape(d: &Document) -> Vec<Vec<(String, TagKind)>> {
    d.units
        .iter()
        .map(|u| {
            u.source()
                .iter()
                .filter_map(|i| match i {
                    Inline::Tag { id, kind, .. } => Some((id.clone(), *kind)),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

#[test]
fn eleven_languages_round_trip_and_survive_pseudo_translation() {
    let d = extract(LANGS);
    assert_eq!(Arb.merge(&d).unwrap(), LANGS.as_bytes());
    let texts: Vec<String> = d.units.iter().map(|u| plain_text(&u.source())).collect();
    for needle in ["ラーチムアへようこそ", "欢迎来到拉奇莫尔", "라치모어에", "مرحبا بكم", "Bienvenue \u{e0} Larchmoor", "\u{1F600}"] {
        assert!(texts.iter().any(|t| t.contains(needle)), "{needle}");
    }
    let mut t = d.clone();
    translate_all(&mut t, "[x]");
    let out = Arb.merge(&t).unwrap();
    let back = Arb.extract(&out, &ExtractOptions::default()).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    assert_eq!(tag_shape(&back), tag_shape(&d));
    for (a, b) in d.units.iter().zip(&back.units) {
        let mut want = vec![Inline::text("[x]")];
        want.extend(a.source());
        assert_eq!(b.source(), merge_text(want), "{}", a.id);
    }
    // the output is valid JSON with the same keys
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["fr"], "[x]Bienvenue \u{e0} Larchmoor, {name}");
}

#[test]
fn every_target_language_gets_its_locale_and_categories() {
    // (target, categories of a cardinal plural)
    let cases: [(&str, &[&str]); 11] = [
        ("en", &["one", "other"]),
        ("es-ES", &["one", "many", "other"]),
        ("fr-FR", &["one", "many", "other"]),
        ("de-DE", &["one", "other"]),
        ("it-IT", &["one", "many", "other"]),
        ("nl-NL", &["one", "other"]),
        ("pt-BR", &["one", "many", "other"]),
        ("ja-JP", &["other"]),
        ("zh-Hans-CN", &["other"]),
        ("ko-KR", &["other"]),
        ("ar-EG", &["zero", "one", "two", "few", "many", "other"]),
    ];
    for (target, cats) in cases {
        assert_eq!(bilingua_arb::plural_categories(target, false).unwrap(), cats, "{target}");
        let mut d = extract(LANGS);
        d.target_lang = Some(target.into());
        translate_all(&mut d, "T:");
        let out = Arb.merge(&d).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["@@locale"], target.replace('-', "_"));
        let plural = v["loaves"].as_str().unwrap();
        for c in cats {
            assert!(plural.contains(&format!("{c}{{")), "{target}: {plural}");
        }
        assert_eq!(plural.matches("{T:").count(), cats.len() + 1, "{target}: =0 and each category: {plural}");
        // a merged file reads back
        assert!(Arb.extract(&out, &ExtractOptions::default()).is_ok());
    }
    assert!(bilingua_arb::plural_categories("xx-YY", false).is_none());
    assert!(bilingua_arb::plural_categories("", false).is_none());
    assert_eq!(bilingua_arb::plural_categories("ru_RU", true).unwrap(), ["other"]);
}

#[test]
fn untouched_text_keeps_its_escapes_and_a_changed_message_is_requoted() {
    let mut d = extract(LANGS);
    let i = d.units.iter().position(|u| u.context.key.as_deref() == Some("fr")).unwrap();
    d.units[i].segments[0].target = vec![Inline::text("Salut \"toi\"\n")];
    d.units[i].segments[0].state = SegState::Draft;
    let out = String::from_utf8(Arb.merge(&d).unwrap()).unwrap();
    assert!(out.contains(r#""fr": "Salut \"toi\"\n","#), "{out}");
    // the other messages keep their original spelling: \/ and 😀 stay escaped
    assert!(out.contains(r#"slash \/ slash 😀"#));
}

#[test]
fn metadata_becomes_notes_and_placeholders_stay_flattenable() {
    let d = extract(LANGS);
    let u = &d.units[0];
    assert_eq!(u.context.key.as_deref(), Some("welcome"));
    assert_eq!(u.context.notes, ["Home page greeting", "{name}: String, e.g. Ana"]);
    // the tag keeps the label `placeholder` (the LocOS adapter flattens it to its display, the source text)
    let tag = u.codes.get("1").unwrap();
    assert_eq!(tag.label.as_deref(), Some("placeholder"));
    assert!(matches!(&u.source()[1], Inline::Tag { free: true, display, .. } if display == "{name}"));
    // plural branch units carry the argument and selector
    let loaves: Vec<&Unit> = d.units.iter().filter(|u| u.context.key.as_deref() == Some("loaves")).collect();
    assert_eq!(loaves.len(), 4); // text around the argument, =0, one, other
    assert_eq!(loaves[0].context.extra.get("icu"), None);
    assert_eq!(loaves[2].context.extra.get("icu").map(String::as_str), Some("count: plural one"));
    // # is a standalone tag that does not move freely
    assert!(matches!(&loaves[2].source()[0], Inline::Tag { kind: TagKind::Standalone, free: false, display, .. } if display == "#"));
}

#[test]
fn damaged_translations_fall_back_to_the_source_level() {
    let src = r#"{"m": "You have {count, plural, one{# file} other{# files}} left", "n": "Hello {name}"}"#;
    let d = extract(src);
    assert_eq!(d.units.len(), 4);
    // the outer unit drops the plural argument: the source outer text stays, the branches still translate
    let mut a = d.clone();
    a.units[0].segments[0].target = vec![Inline::text("Tienes archivos")];
    a.units[1].segments[0].target = vec![Inline::text("# archivo")];
    let out = String::from_utf8(Arb.merge(&a).unwrap()).unwrap();
    assert!(out.contains("You have {count, plural, one{# archivo} other{# files}} left"), "{out}");
    // a translation that breaks ICU braces falls back to the source
    let mut b = d.clone();
    b.units[3].segments[0].target = vec![Inline::text("Hallo {name")];
    assert_eq!(Arb.merge(&b).unwrap(), src.as_bytes());
    // a hand-made tag the source never had falls back
    let mut c = d.clone();
    c.units[0].segments[0].target = vec![Inline::tag("9", TagKind::Standalone, "{x}")];
    assert_eq!(Arb.merge(&c).unwrap(), src.as_bytes());
    // a moved free placeholder is fine
    let mut e = d.clone();
    let tag = d.units[3].source()[1].clone();
    e.units[3].segments[0].target = vec![tag, Inline::text(", hallo")];
    let out = String::from_utf8(Arb.merge(&e).unwrap()).unwrap();
    assert!(out.contains(r#""n": "{name}, hallo""#), "{out}");
}

#[test]
fn bom_and_crlf_are_kept_and_locale_insertion_follows_the_line_ending() {
    let crlf = "\u{feff}{\r\n  \"hello\": \"Hello\",\r\n  \"bye\": \"Goodbye\"\r\n}\r\n";
    let mut d = Arb.extract(crlf.as_bytes(), &ExtractOptions::default()).unwrap();
    assert_eq!(Arb.merge(&d).unwrap(), crlf.as_bytes());
    d.target_lang = Some("fr".into());
    d.units[0].segments[0].target = vec![Inline::text("Bonjour")];
    let out = String::from_utf8(Arb.merge(&d).unwrap()).unwrap();
    assert_eq!(out, "\u{feff}{\r\n  \"@@locale\": \"fr\",\r\n  \"hello\": \"Bonjour\",\r\n  \"bye\": \"Goodbye\"\r\n}\r\n");
    // nothing translated: untouched, even with a target language
    let mut e = extract("{ }");
    assert!(e.units.is_empty());
    e.target_lang = Some("fr".into());
    assert_eq!(Arb.merge(&e).unwrap(), b"{ }");
}

#[test]
fn only_utf8_is_read() {
    let latin1 = b"{\"a\": \"caf\xe9\"}";
    assert!(matches!(Arb.extract(latin1, &ExtractOptions::default()), Err(Error::Parse { .. })));
    let mut utf16 = vec![0xFF, 0xFE];
    for c in "{\"a\": \"b\"}".encode_utf16() {
        utf16.extend(c.to_le_bytes());
    }
    assert!(matches!(Arb.extract(&utf16, &ExtractOptions::default()), Err(Error::Unsupported(_))));
}

#[test]
fn sentence_segments_merge_back_with_their_separators() {
    let src = r#"{"m": "First sentence here. Second one for {name}. Third.", "@m": {"description": "d"}}"#;
    let mut d = extract(src);
    bilingua_seg::segment(&mut d, "en", Granularity::Sentence, bilingua_seg::Rules::bundled());
    assert!(d.units[0].segments.len() >= 2);
    assert_eq!(Arb.merge(&d).unwrap(), src.as_bytes());
    for s in d.units[0].segments.iter_mut() {
        let mut t = vec![Inline::text("<")];
        t.extend(s.source.clone());
        s.target = merge_text(t);
    }
    let out = String::from_utf8(Arb.merge(&d).unwrap()).unwrap();
    assert!(out.contains("<First sentence here. <Second one for {name}. <Third."), "{out}");
    // and back to paragraph mode
    bilingua_seg::segment(&mut d, "en", Granularity::Paragraph, bilingua_seg::Rules::bundled());
    assert_eq!(d.units[0].segments.len(), 1);
}

#[test]
fn duplicate_keys_and_odd_values() {
    let src = r#"{"a": "One", "a": "Two", "n": 5, "o": {"x": "ignored"}, "l": ["ignored"], "@@x": "meta", "e": "", "p": "---"}"#;
    let d = extract(src);
    assert_eq!(bilingua_arb::keys(&d), ["a", "a"]);
    assert_eq!(Arb.merge(&d).unwrap(), src.as_bytes());
}

#[test]
fn document_json_round_trips() {
    let d = extract(LANGS);
    let j = serde_json::to_string(&d).unwrap();
    let back: Document = serde_json::from_str(&j).unwrap();
    assert_eq!(back, d);
    assert_eq!(Arb.merge(&back).unwrap(), LANGS.as_bytes());
}

#[test]
fn the_locos_profile_adds_locale_and_plural_keys_with_the_files_crlf() {
    let crlf = "{\r\n  \"hello\": \"Hello\",\r\n  \"itemOne\": \"one\",\r\n  \"itemOther\": \"many\"\r\n}\r\n";
    let o = ExtractOptions::new().with_source_lang("en").with_profile(Some(profile::LOCOS));
    let mut d = Arb.extract(crlf.as_bytes(), &o).unwrap();
    d.target_lang = Some("ru".into());
    for u in d.units.iter_mut() {
        u.segments[0].target = vec![Inline::text("x")];
    }
    let m = MergeOptions::new().with_profile(Some(profile::LOCOS));
    let out = String::from_utf8(Arb.merge_with(&d, &m).unwrap()).unwrap();
    assert!(out.contains("\"@@locale\""), "{out}");
    assert!(!out.replace("\r\n", "").contains('\n'), "a bare LF in a CRLF file: {out:?}");
}
