//! Behaviour beyond the LocOS suites: tags, notes, line endings, encodings, languages, locked units, merges that
//! change nothing, and the cases that make a splice different from a re-serialisation.
mod common;
use bilingua_model::*;
use bilingua_xcstrings::{plural_categories, Xcstrings};
use common::*;

const TABS: &[u8] = include_bytes!("data/tabs_en_us.xcstrings");

fn merged(d: &Document) -> Vec<u8> {
    Xcstrings.merge(d).unwrap()
}

#[test]
fn units_tags_and_notes() {
    let d = ex(TABS, None);
    let t = texts(&d);
    // whitespace around a value stays outside the unit; numbered specifiers are free codes (p), others x
    assert!(t.contains(&"Zeta value \u{27E6}p1\u{27E7} of \u{27E6}p2\u{27E7}".to_string()), "{t:?}");
    assert!(t.contains(&"\u{27E6}x1\u{27E7} in 100%% of folders".to_string()), "{t:?}");
    assert!(t.contains(&"No files".to_string()) && t.contains(&"\u{27E6}x1\u{27E7} file".to_string()));
    assert!(t.contains(&"\u{27E6}x1\u{27E7} taps".to_string()));
    assert!(t.contains(&"Any other device \u{e9}".to_string()));
    assert!(t.contains(&"A/B \u{201c}quoted\u{201d} \u{2013} done".to_string()));
    // key as source text (no source localization), the key's own padding removed
    assert!(t.contains(&"padded".to_string()));
    // not units: shouldTranslate false, no letter, placeholder-only
    assert!(!t.iter().any(|x| x == "12345" || x == "---" || x == "\u{27E6}x1\u{27E7}"), "{t:?}");
    let u = d.units.iter().find(|u| text_of(u) == "padded").unwrap();
    assert_eq!(u.context.notes, ["key with spaces; also used as the source text"]);
    assert_eq!(u.context.key.as_deref(), Some("  padded  "));
    let tags: Vec<bool> = d.units.iter().find(|u| text_of(u).starts_with("Zeta")).unwrap().source().iter().filter(|i| matches!(i, Inline::Tag { .. })).map(Inline::is_free).collect();
    assert_eq!(tags, [true, true]);
    let v: Vec<&String> = d.units.iter().filter_map(|u| u.context.extra.get("variation")).collect();
    assert!(v.iter().any(|s| s.as_str() == "device:iphone plural:one"), "{v:?}");
}

fn text_of(u: &Unit) -> String {
    locos_text(u)
}

#[test]
fn ids_are_stable_and_unique() {
    let a = ex(TABS, None);
    let b = ex(TABS, Some("de"));
    assert_eq!(a.units.iter().map(|u| &u.id).collect::<Vec<_>>(), b.units.iter().map(|u| &u.id).collect::<Vec<_>>());
    let set: std::collections::HashSet<_> = a.units.iter().map(|u| &u.id).collect();
    assert_eq!(set.len(), a.units.len());
    assert_eq!(a.source_lang.as_deref(), Some("en"));
}

#[test]
fn crlf_catalog_stays_crlf() {
    let crlf = String::from_utf8(LOCOS_CATALOG.to_vec()).unwrap().replace('\n', "\r\n").into_bytes();
    assert_eq!(merged(&ex(&crlf, Some("ru"))), crlf);
    let mut d = ex(&crlf, Some("ru"));
    translate_all(&mut d, |t| format!("RU {t}"));
    let out = merged(&d);
    let mut lf = ex(LOCOS_CATALOG, Some("ru"));
    translate_all(&mut lf, |t| format!("RU {t}"));
    // the same file with every line ending CRLF, new lines included
    assert_eq!(out, String::from_utf8(merged(&lf)).unwrap().replace('\n', "\r\n").into_bytes());
}

#[test]
fn bom_is_kept_and_other_encodings_are_refused() {
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(LOCOS_CATALOG);
    assert_eq!(merged(&ex(&bom, None)), bom);
    let mut d = ex(&bom, Some("de"));
    translate_all(&mut d, |t| format!("DE {t}"));
    assert!(merged(&d).starts_with(&[0xEF, 0xBB, 0xBF]));
    // UTF-16 is not a catalog encoding Xcode writes; LocOS refuses it too
    let mut u16le = vec![0xFF, 0xFE];
    for c in String::from_utf8(LOCOS_CATALOG.to_vec()).unwrap().encode_utf16() {
        u16le.extend_from_slice(&c.to_le_bytes());
    }
    assert!(matches!(Xcstrings.extract(&u16le, &opts(None)), Err(Error::Encoding(_))));
}

#[test]
fn eleven_languages_round_trip_with_the_same_units() {
    for lang in ["de", "fr", "es", "ru", "pl", "ar", "ja", "zh-Hans", "ko", "pt_BR", "tr"] {
        for data in [LOCOS_CATALOG, TABS] {
            let d0 = ex(data, Some(lang));
            let mut d = d0.clone();
            translate_all(&mut d, |t| format!("\u{ab}{t}\u{bb}"));
            let out = merged(&d);
            // valid JSON; the source language is untouched; the new language is there
            let v = json(&out);
            assert!(v["strings"].is_object());
            // re-reading in the SOURCE language gives the same units
            let back = ex(&out, None);
            assert_eq!(texts(&back), texts(&d0), "{lang}");
            assert_eq!(back.units.len(), d0.units.len());
            // reading the target as if it were the source: units of the new language have our marks
            let tl = lang.replace('_', "-");
            let found = v["strings"].as_object().unwrap().values().any(|e| e["localizations"].get(&tl).is_some());
            assert!(found, "{lang} missing");
            // Unicode text is written as it is (or escaped like the file does), never damaged
            let s = String::from_utf8(out).unwrap();
            assert!(s.contains('\u{ab}') || s.contains("\\u00ab"), "{lang}");
        }
    }
}

#[test]
fn plural_categories_follow_cldr() {
    let cats = |l: &str| plural_categories(l).unwrap();
    assert_eq!(cats("en"), ["one", "other"]);
    assert_eq!(cats("ru"), ["one", "few", "many", "other"]);
    assert_eq!(cats("ar"), ["zero", "one", "two", "few", "many", "other"]);
    assert_eq!(cats("ja"), ["other"]);
    assert_eq!(cats("pl"), ["one", "few", "many", "other"]);
    assert_eq!(cats("fr"), ["one", "many", "other"]);
    assert_eq!(cats("pt_BR"), ["one", "many", "other"]);
    assert_eq!(cats("zh-Hans"), ["other"]);
    assert_eq!(cats("ko"), ["other"]);
    assert_eq!(plural_categories("xx-unknown"), None);
    // Arabic gets all six forms, filled from `other`; `zero` of the source is kept when the target lacks it
    let mut d = ex(TABS, Some("ar"));
    translate_all(&mut d, |t| format!("AR {t}"));
    let v = json(&merged(&d));
    let p = v["strings"]["files"]["localizations"]["ar"]["substitutions"]["count"]["variations"]["plural"].as_object().unwrap();
    let mut keys: Vec<&String> = p.keys().collect();
    keys.sort();
    assert_eq!(keys, ["few", "many", "one", "other", "two", "zero"]);
    assert_eq!(p["zero"]["stringUnit"]["value"], "AR No files");
    assert_eq!(p["two"]["stringUnit"]["value"], "AR %arg files");
    let mut d = ex(TABS, Some("ja"));
    translate_all(&mut d, |t| format!("JA {t}"));
    let v = json(&merged(&d));
    let p = v["strings"]["files"]["localizations"]["ja"]["substitutions"]["count"]["variations"]["plural"].as_object().unwrap();
    // Japanese uses only `other`, but the source `zero` form is kept (LocOS rule)
    let mut keys: Vec<&String> = p.keys().collect();
    keys.sort();
    assert_eq!(keys, ["other", "zero"]);
}

#[test]
fn unknown_language_keeps_the_source_forms() {
    let mut d = ex(LOCOS_CATALOG, Some("xx"));
    translate_all(&mut d, |t| format!("XX {t}"));
    let v = json(&merged(&d));
    let p = v["strings"]["%lld items"]["localizations"]["xx"]["variations"]["plural"].as_object().unwrap();
    let mut keys: Vec<&String> = p.keys().collect();
    keys.sort();
    assert_eq!(keys, ["one", "other"]);
}

#[test]
fn nothing_translated_changes_nothing() {
    for data in [LOCOS_CATALOG, TABS] {
        for lang in [None, Some("de"), Some("ar")] {
            assert_eq!(merged(&ex(data, lang)), data);
        }
    }
}

#[test]
fn untouched_text_is_byte_for_byte() {
    // Only the German leaf of one key is translated: every other byte of the hand-formatted file stays.
    let mut d = ex(TABS, Some("de"));
    let idx = d.units.iter().position(|u| text_of(u) == "Any other device \u{e9}").unwrap();
    d.units[idx].segments[0].target = vec![Inline::text("Anderes Ger\u{e4}t")];
    let out = String::from_utf8(merged(&d)).unwrap();
    let orig = String::from_utf8(TABS.to_vec()).unwrap();
    // everything before the edited entry is identical
    let cut = orig.find("\"device.plural\"").unwrap();
    assert_eq!(&out[..cut], &orig[..cut]);
    // and after it, up to the next entry
    let after = orig.find("\"12345\"").unwrap();
    let tail = &orig[after..];
    assert!(out.ends_with(tail), "tail changed");
    assert!(out.contains("Anderes Ger\\u00e4t"), "{out}");
}

#[test]
fn locked_units_are_listed_on_request_and_never_merged() {
    let o = ExtractOptions::new().with_include_locked(true);
    let d = Xcstrings.extract(LOCOS_CATALOG, &o).unwrap();
    let locked: Vec<&Unit> = d.units.iter().filter(|u| !u.translate).collect();
    assert_eq!(locked.len(), 2, "{:?}", locked.iter().map(|u| locos_text(u)).collect::<Vec<_>>());
    assert!(locked.iter().all(|u| u.lock_reason.is_some()));
    // translatable units keep their ids with or without the locked ones
    let plain = ex(LOCOS_CATALOG, None);
    let ids = |d: &Document| d.units.iter().filter(|u| u.translate).map(|u| u.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&d), ids(&plain));
    assert_eq!(Xcstrings.merge(&d).unwrap(), LOCOS_CATALOG);
    let mut d2 = d.clone();
    for u in d2.units.iter_mut().filter(|u| !u.translate) {
        u.segments[0].target = vec![Inline::text("LOCKED EDIT")];
    }
    let out = String::from_utf8(Xcstrings.merge(&d2).unwrap()).unwrap();
    assert!(!out.contains("LOCKED EDIT"), "a locked unit is never written");
}

#[test]
fn moved_numbered_specifiers_come_back_in_place() {
    let data = br#"{"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%1$@ owns %2$lld items"}}}}}}"#;
    let mut d = ex(data, Some("de"));
    // a translation may reorder numbered specifiers
    let u = &mut d.units[0];
    let (_, map) = locos::to_locos_text(&u.source());
    u.segments[0].target = locos::from_locos_text("\u{27E6}p2\u{27E7} Dinge geh\u{f6}ren \u{27E6}p1\u{27E7}", &map);
    let v = json(&merged(&d));
    assert_eq!(v["strings"]["k"]["localizations"]["de"]["stringUnit"]["value"], "%2$lld Dinge geh\u{f6}ren %1$@");
}

#[test]
fn key_as_source_in_a_no_locale_merge_adds_a_source_localization() {
    let data = br#"{"sourceLanguage":"en","strings":{"Hello":{}}}"#;
    let mut d = ex(data, None);
    d.units[0].segments[0].target = vec![Inline::text("Howdy")];
    let out = String::from_utf8(merged(&d)).unwrap();
    assert_eq!(out, r#"{"sourceLanguage":"en","strings":{"Hello":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Howdy"}}}}}}"#);
}

#[test]
fn repeated_keys_follow_python_dict_rules() {
    // the last value of a repeated key wins, in the first key's place
    let data = br#"{"sourceLanguage":"en","strings":{"a":{"localizations":{"en":{"stringUnit":{"value":"First"}}}},"b":{},"a":{"localizations":{"en":{"stringUnit":{"value":"Second"}}}}}}"#;
    let d = ex(data, None);
    assert_eq!(texts(&d), ["Second", "b"]);
}

#[test]
fn a_skeleton_from_another_crate_is_refused() {
    let mut d = ex(LOCOS_CATALOG, None);
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-json", "0.1.0", 1));
    }
    assert!(matches!(Xcstrings.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn surrogate_pairs_and_escapes_read_back() {
    let data = "{\"sourceLanguage\":\"en\",\"strings\":{\"k\":{\"localizations\":{\"en\":{\"stringUnit\":{\"state\":\"translated\",\"value\":\"Smile \\ud83d\\ude00 \\\"x\\\" \\u00e9\\n\"}}}}}}";
    let d = ex(data.as_bytes(), None);
    assert_eq!(texts(&d), ["Smile \u{1F600} \"x\" \u{e9}"]);
    assert_eq!(merged(&d), data.as_bytes());
}

#[test]
fn granularity_is_carried_and_sentences_merge_back() {
    let data = br#"{"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"One. Two."}}}}}}"#;
    let mut d = ex(data, Some("de"));
    // a unit split into two segments joins back with its separator
    let u = &mut d.units[0];
    let s0 = u.segments[0].clone();
    u.segments = vec![
        Segment::new("u1#1", vec![Inline::text("One.")]).with_target(vec![Inline::text("Eins.")]).with_state(s0.state).with_origin(s0.origin.clone()),
        Segment::new("u1#2", vec![Inline::text("Two.")]).with_target(vec![Inline::text("Zwei.")]).with_state(s0.state).with_origin(s0.origin),
    ];
    u.separators = vec![" ".into()];
    let v = json(&merged(&d));
    assert_eq!(v["strings"]["k"]["localizations"]["de"]["stringUnit"]["value"], "Eins. Zwei.");
}
