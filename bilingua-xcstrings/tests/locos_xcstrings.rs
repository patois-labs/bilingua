//! The Localization OS suite `tests/verify_xcstrings_filter.py`, case by case (9 test functions there, 9 here;
//! the API and CLI case is ported as far as it concerns the file: upload, translate, download and keep the other
//! languages). The fixture is the suite's own `Localizable.xcstrings`.
mod common;
use bilingua_model::*;
use bilingua_xcstrings::{records, Xcstrings};
use common::*;

const PH: &str = "\u{27E6}x1\u{27E7}";

fn all_same(data: &[u8]) -> Vec<(usize, String)> {
    texts(&ex(data, None)).into_iter().enumerate().collect()
}

fn build_owned(data: &[u8], tr: &[(usize, String)], locale: Option<&str>) -> Vec<u8> {
    let refs: Vec<(usize, &str)> = tr.iter().map(|(i, s)| (*i, s.as_str())).collect();
    build(data, &refs, locale)
}

/// Positions of the keys of `obj_text`'s object `name` in file order (the JSON value type sorts keys).
fn order_in(text: &str, anchor: &str, keys: &[&str]) -> Vec<String> {
    let from = text.find(anchor).unwrap_or_else(|| panic!("{anchor} not in output"));
    let mut found: Vec<(usize, String)> = keys.iter().filter_map(|k| text[from..].find(&format!("\"{k}\"")).map(|p| (p, k.to_string()))).collect();
    found.sort();
    found.into_iter().map(|(_, k)| k).collect()
}

#[test]
fn test_registered_and_identity() {
    assert_eq!(Xcstrings.id(), "xcstrings");
    assert_eq!(Xcstrings.extensions(), &["xcstrings"]);
    assert!(Xcstrings.sniff(LOCOS_CATALOG));
    let data = LOCOS_CATALOG;
    assert_eq!(build(data, &[], None), data);
    let same: Vec<(usize, &str)> = Vec::new();
    let _ = same;
    let all = all_same(data);
    assert_eq!(build_owned(data, &all, None), data);
    assert_eq!(build(data, &[], Some("ru")), data);
}

#[test]
fn test_units() {
    let units = texts(&ex(LOCOS_CATALOG, None));
    let want = [
        format!("{PH} item"),
        format!("{PH} items"),
        format!("Hello, {PH}!"),
        "Tap to open Settings".to_string(),
        "Click to open Settings".to_string(),
        format!("Uploaded {PH} to the album"),
        format!("{PH} photo"),
        format!("{PH} photos"),
    ];
    assert_eq!(units, want);
    // "Legacy banner" (stale) and "SKU-%@" (shouldTranslate false) are not units
    assert!(!units.iter().any(|u| u.contains("Legacy") || u.contains("SKU")));
}

#[test]
fn test_localize_adds_only_target() {
    let data = LOCOS_CATALOG;
    let before = json(data);
    let tr: Vec<(usize, String)> = all_same(data).into_iter().map(|(i, v)| (i, format!("RU {v}"))).collect();
    let out = build_owned(data, &tr, Some("ru"));
    let obj = json(&out);
    for (key, entry) in before["strings"].as_object().unwrap() {
        if let Some(locs) = entry.get("localizations").and_then(|l| l.as_object()) {
            for (lang, loc) in locs {
                assert_eq!(obj["strings"][key]["localizations"][lang], *loc, "{key} {lang}");
            }
        }
    }
    let text = String::from_utf8(out.clone()).unwrap();
    let items = &obj["strings"]["%lld items"]["localizations"];
    assert_eq!(items.as_object().unwrap().len(), 3);
    let at = text.find("\"%lld items\"").unwrap();
    assert_eq!(order_in(&text[at..], "\"localizations\"", &["en", "fr", "ru"]), ["en", "fr", "ru"]);
    let plural = items["ru"]["variations"]["plural"].as_object().unwrap();
    let mut cats: Vec<&String> = plural.keys().collect();
    cats.sort();
    assert_eq!(cats, ["few", "many", "one", "other"]);
    assert_eq!(plural["one"]["stringUnit"], serde_json::json!({"state": "translated", "value": "RU %lld item"}));
    assert_eq!(plural["many"]["stringUnit"]["value"], "RU %lld items");
    let hello = obj["strings"]["Hello, %@!"]["localizations"].as_object().unwrap();
    assert_eq!(hello.keys().collect::<Vec<_>>(), ["ru"]);
    assert_eq!(hello["ru"]["stringUnit"]["value"], "RU Hello, %@!");
    let sub = &obj["strings"]["upload_status"]["localizations"]["ru"]["substitutions"]["photos"];
    assert_eq!((sub["argNum"].as_i64(), sub["formatSpecifier"].as_str()), (Some(1), Some("lld")));
    assert_eq!(sub["variations"]["plural"]["few"]["stringUnit"]["value"], "RU %arg photos");
    assert!(obj["strings"]["Legacy banner"].get("localizations").is_none());
    assert!(obj["strings"]["SKU-%@"].get("localizations").is_none());
    // Xcode style kept: " : " separators, two-space indent, no trailing newline
    assert!(text.contains("\"sourceLanguage\" : \"en\"") && text.contains("\n  \"strings\" : {"));
    assert!(!text.ends_with('\n') && !text.contains('\u{27E6}'));
}

#[test]
fn test_localize_replaces_existing_and_partial() {
    let data = LOCOS_CATALOG;
    let out = json(&build(data, &[(3, "Tippen"), (4, "Klicken")], Some("de")));
    let de = &out["strings"]["Open Settings"]["localizations"]["de"];
    assert_eq!(
        *de,
        serde_json::json!({"variations": {"device": {
            "iphone": {"stringUnit": {"state": "translated", "value": "Tippen"}},
            "mac": {"stringUnit": {"state": "translated", "value": "Klicken"}},
        }}})
    );
    // only one plural unit translated: the other category keeps the existing French text
    let fr = json(&build(data, &[(0, "%lld truc")], Some("fr")));
    let plural = &fr["strings"]["%lld items"]["localizations"]["fr"]["variations"]["plural"];
    assert_eq!(plural["one"]["stringUnit"]["value"], "%lld truc");
    assert_eq!(plural["other"]["stringUnit"]["value"], "%lld éléments");
    // locale spelling: pt_BR matches the catalog's pt-BR convention; ja keeps only other
    let ja = json(&build(data, &[(0, "%lld 件"), (1, "%lld 件")], Some("ja")));
    let cats: Vec<&String> = ja["strings"]["%lld items"]["localizations"]["ja"]["variations"]["plural"].as_object().unwrap().keys().collect();
    assert_eq!(cats, ["other"]);
    let pt = json(&build(data, &[(2, "Olá, %@!")], Some("pt_BR")));
    assert!(pt["strings"]["Hello, %@!"]["localizations"].get("pt-BR").is_some());
}

#[test]
fn test_build_without_locale_rewrites_source() {
    let out = json(&build(LOCOS_CATALOG, &[(2, &format!("Hi, {PH}!")), (3, "Tap for Settings")], None));
    assert_eq!(out["strings"]["Hello, %@!"]["localizations"]["en"]["stringUnit"]["value"], "Hi, %@!");
    let dev = &out["strings"]["Open Settings"]["localizations"]["en"]["variations"]["device"];
    assert_eq!(dev["iphone"]["stringUnit"]["value"], "Tap for Settings");
}

#[test]
fn test_style_variants() {
    let compact = "{\"sourceLanguage\":\"en\",\"strings\":{\"A\\/B test\":{\"comment\":\"x\"},\"Caf\\u00e9\":{}},\"version\":\"1.0\"}";
    let mut data = vec![0xEF, 0xBB, 0xBF];
    data.extend_from_slice(compact.as_bytes());
    data.push(b'\n');
    let all = all_same(&data);
    assert_eq!(all.len(), 2);
    assert_eq!(build_owned(&data, &all, None), data);
    let out = build(&data, &[(0, "A/B-Test")], Some("de"));
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]) && out.ends_with(b"}\n"));
    let text = std::str::from_utf8(&out[3..]).unwrap();
    assert!(text.is_ascii());
    assert!(!text.trim().contains('\n') && text.contains("\":") && !text.contains("\\\""));
    assert!(text.contains("A\\/B-Test") && text.contains("\\u00e9"), "{text}");
    assert_eq!(json(text.as_bytes())["strings"]["A/B test"]["localizations"]["de"]["stringUnit"]["value"], "A/B-Test");
}

#[test]
fn test_records() {
    let recs = records(LOCOS_CATALOG).unwrap();
    let by = |k: &str| recs.iter().find(|r| r.key == k).unwrap();
    let items = by("%lld items");
    assert_eq!(items.plural_forms.as_deref(), Some(&[("one".to_string(), "%lld item".to_string()), ("other".to_string(), "%lld items".to_string())][..]));
    assert_eq!(items.comment, "Item count in the list footer");
    assert_eq!(items.plural_units.as_deref(), Some(&[("one".to_string(), 0), ("other".to_string(), 1)][..]));
    assert_eq!((by("Hello, %@!").value.as_str(), by("Hello, %@!").unit_index), ("Hello, %@!", Some(2)));
    assert_eq!(by("SKU-%@").unit_index, None);
    assert_eq!(by("Legacy banner").unit_index, None);
    assert_eq!(
        by("Open Settings").plural_forms.as_deref(),
        Some(&[("iphone".to_string(), "Tap to open Settings".to_string()), ("mac".to_string(), "Click to open Settings".to_string())][..])
    );
}

fn refused(data: &[u8]) -> Error {
    Xcstrings.extract(data, &opts(None)).expect_err("must be refused")
}

fn limit_kind(e: &Error) -> LimitKind {
    match e {
        Error::Limit(l) => l.kind,
        other => panic!("expected a limit error, got {other}"),
    }
}

#[test]
fn test_hostile_inputs() {
    for (data, needle) in [(&b"{\"strings\": []}"[..], "String Catalog"), (b"[1, 2]", "String Catalog"), (b"not json", "JSON")] {
        let e = refused(data);
        assert!(matches!(e, Error::Parse { .. }), "{e}");
        assert!(e.to_string().contains(needle), "{e}");
    }
    let latin1 = b"{\"sourceLanguage\":\"en\",\"strings\":{\"caf\xe9\":{}}}";
    let e = refused(latin1);
    assert!(matches!(e, Error::Encoding(_)) && e.to_string().contains("UTF-8"), "{e}");
    let deep = format!(
        "{{\"sourceLanguage\":\"en\",\"strings\":{{\"k\":{{\"localizations\":{{\"en\":{}{{}}{}}}}}}}}}",
        "{\"variations\":{\"plural\":{\"one\":".repeat(30),
        "}}}".repeat(30)
    );
    let e = refused(deep.as_bytes());
    assert_eq!(limit_kind(&e), LimitKind::XmlDepth);
    assert!(e.to_string().contains("nested"));
    // caps (the suite lowers MAX_DOCUMENT_SEGMENTS to 3)
    let small = ExtractOptions::new().with_limits(Limits::new().with_max_units(3));
    let many = format!("{{\"sourceLanguage\":\"en\",\"strings\":{{{}}}}}", (0..5).map(|i| format!("\"String {i}\":{{}}")).collect::<Vec<_>>().join(","));
    let e = Xcstrings.extract(many.as_bytes(), &small).unwrap_err();
    assert_eq!(limit_kind(&e), LimitKind::Units);
    let quotes = format!("{{\"sourceLanguage\":\"en\",\"strings\":{{}},\"x\":[{}]}}", vec!["\"\""; 100].join(","));
    let e = Xcstrings.extract(quotes.as_bytes(), &small).unwrap_err();
    assert_eq!(limit_kind(&e), LimitKind::Units);
}

#[test]
fn test_api_and_cli_surfaces_file_level() {
    // upload -> translate -> download for ru keeps en/fr/de, ru plural has few/many/one/other with %lld kept
    let data = LOCOS_CATALOG;
    let before = json(data);
    let mut d = ex(data, Some("ru"));
    translate_all(&mut d, |t| format!("\u{440}\u{443} {t}"));
    let out = Xcstrings.merge(&d).unwrap();
    let obj = json(&out);
    for key in ["%lld items", "Open Settings"] {
        for (lang, loc) in before["strings"][key]["localizations"].as_object().unwrap() {
            assert_eq!(obj["strings"][key]["localizations"][lang], *loc, "{key} {lang}");
        }
    }
    let ru = obj["strings"]["%lld items"]["localizations"]["ru"]["variations"]["plural"].as_object().unwrap();
    let mut cats: Vec<&String> = ru.keys().collect();
    cats.sort();
    assert_eq!(cats, ["few", "many", "one", "other"]);
    assert!(ru["one"]["stringUnit"]["value"].as_str().unwrap().contains("%lld"));
    assert!(!String::from_utf8(out).unwrap().contains('\u{27E6}'));
    assert!(obj["strings"]["Hello, %@!"]["localizations"].get("ru").is_some());
    assert_eq!(obj["strings"]["Open Settings"]["localizations"]["de"], before["strings"]["Open Settings"]["localizations"]["de"]);
}

/// B-side 14 (5 wild files, e.g. a catalog that starts with `"" : {\n\n    }`): a translated build splices the
/// translated entries and keeps every other byte, empty entry objects included (LocOS splices values).
#[test]
fn empty_entry_objects_are_kept_byte_for_byte() {
    let src = "{\n  \"sourceLanguage\" : \"en\",\n  \"strings\" : {\n    \"\" : {\n\n    },\n    \"#\" : {\n\n    },\n    \"Hello\" : {\n\n    }\n  },\n  \"version\" : \"1.0\"\n}";
    let d = ex(src.as_bytes(), Some("de"));
    assert_eq!(texts(&d), ["Hello"]);
    let out = String::from_utf8(build(src.as_bytes(), &[(0, "Hallo")], Some("de"))).unwrap();
    assert!(out.starts_with("{\n  \"sourceLanguage\" : \"en\",\n  \"strings\" : {\n    \"\" : {\n\n    },\n    \"#\" : {\n\n    },\n    \"Hello\" : {"), "{out}");
    assert!(out.contains("\"value\" : \"Hallo\""), "{out}");
    assert!(out.ends_with("\n  },\n  \"version\" : \"1.0\"\n}"), "{out}");
    // under the LocOS profile a changed catalog is written as LocOS's `json.dumps` writes it: `{}` for an empty object
    let mut d = Xcstrings.extract(src.as_bytes(), &profile::locos().extract_options().with_target_lang("de")).unwrap();
    set_all(&mut d, &[(0, "Hallo")]);
    let out = String::from_utf8(Xcstrings.merge_with(&d, &profile::locos().merge_options("xcstrings")).unwrap()).unwrap();
    assert!(out.starts_with("{\n  \"sourceLanguage\" : \"en\",\n  \"strings\" : {\n    \"\" : {},\n    \"#\" : {},\n    \"Hello\" : {\n"), "{out}");
    assert!(out.contains("\"value\" : \"Hallo\""), "{out}");
    // nothing translated: the original bytes
    let d = Xcstrings.extract(src.as_bytes(), &profile::locos().extract_options().with_target_lang("de")).unwrap();
    assert_eq!(Xcstrings.merge_with(&d, &profile::locos().merge_options("xcstrings")).unwrap(), src.as_bytes());
}
