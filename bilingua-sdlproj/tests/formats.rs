//! Every fixture in `tests/data`: round trips, the 11 languages and each encoding the filter reads.

mod common;

use bilingua_model::*;
use bilingua_sdlproj::{project_info, Sdlproj};
use common::*;

include!("data_list.inc");

fn encoding(d: &Document) -> String {
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => enc::base(encoding).to_string(),
        _ => String::new(),
    }
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, data) in FILES {
        let d = extract(data);
        assert_eq!(Sdlproj.merge(&d).unwrap(), data, "{name}");
        assert!(d.units.is_empty(), "{name}");
    }
}

#[test]
fn a_target_locale_merge_changes_nothing_because_nothing_is_translated() {
    // lead decision 11-04: nothing translated means no language stamp; and a project file has no field to stamp
    for (name, data) in FILES {
        let d = Sdlproj.extract(data, &ExtractOptions { target_lang: Some("ja-JP".into()), ..opts() }).unwrap();
        let out = Sdlproj.merge(&d).unwrap();
        assert_eq!(out, data, "{name}");
        let again = extract(&out);
        assert_eq!(again.units.len(), d.units.len(), "{name}");
        assert_eq!(again.source_lang, d.source_lang, "{name}");
    }
}

#[test]
fn paragraph_and_sentence_mode_agree() {
    for (name, data) in FILES {
        let s = Sdlproj.extract(data, &ExtractOptions { granularity: Granularity::Sentence, ..opts() }).unwrap();
        let p = extract(data);
        assert_eq!(s.units.len(), p.units.len(), "{name}");
    }
}

#[test]
fn eleven_languages_and_their_encodings() {
    let langs = [("en", "en-US"), ("de", "de-DE"), ("fr", "fr-FR"), ("es", "es-ES"), ("pt", "pt-BR"), ("ru", "ru-RU"), ("tr", "tr-TR"), ("ar", "ar-SA"), ("ja", "ja-JP"), ("zh", "zh-CN"), ("ko", "ko-KR")];
    for (l, code) in langs {
        let i = project_info(fixture(&FILES, &format!("lang_{l}"))).unwrap();
        assert_eq!(i.source_lang, code, "{l}");
        assert!(i.name.chars().count() > 5, "{l}: {}", i.name);
    }
    // the same project read from every encoding equals the UTF-8 file
    for (enc_name, utf8, expected) in [
        ("lang_ar_utf16le", "lang_ar", "utf-16le"),
        ("lang_ar_cp1256", "lang_ar", "windows-1256"),
        ("lang_ko_utf16be", "lang_ko", "utf-16be"),
        ("lang_ko_euckr", "lang_ko", "euc-kr"),
        ("lang_fr_cp1252", "lang_fr", "windows-1252"),
        ("lang_de_latin1", "lang_de", "iso-8859-1"),
        ("lang_ru_cp1251", "lang_ru", "windows-1251"),
        ("lang_ru_koi8r", "lang_ru", "koi8-r"),
        ("lang_tr_cp1254", "lang_tr", "windows-1254"),
        ("lang_ja_shiftjis", "lang_ja", "shift_jis"),
        ("lang_ja_eucjp", "lang_ja", "euc-jp"),
        ("lang_zh_gbk", "lang_zh", "gbk"),
        ("lang_zh_gb2312", "lang_zh", "gbk"),
        ("lang_en_utf8bom_crlf", "lang_en", "utf-8-bom"),
    ] {
        let data = fixture(&FILES, enc_name);
        let d = extract(data);
        assert_eq!(encoding(&d), expected, "{enc_name}");
        assert_eq!(project_info(data).unwrap(), project_info(fixture(&FILES, utf8)).unwrap(), "{enc_name}");
        assert_eq!(d.name, extract(fixture(&FILES, utf8)).name, "{enc_name}");
    }
}

#[test]
fn studio_project_shape() {
    let i = project_info(fixture(&FILES, "studio_project")).unwrap();
    assert_eq!((i.root.as_str(), i.source_lang.as_str()), ("Project", "en-US"));
    assert_eq!(i.target_langs, ["de-DE", "ja-JP"]);
    assert_eq!(i.package_guid, "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d");
    assert_eq!(i.name, "", "Name lives on GeneralProjectInfo, which LocOS does not read");
    assert!(extract(fixture(&FILES, "studio_project")).name.is_none());
}

#[test]
fn sniff() {
    assert!(Sdlproj.sniff(fixture(&FILES, "locos_demo")));
    assert!(Sdlproj.sniff(fixture(&FILES, "studio_project")));
    assert!(Sdlproj.sniff(fixture(&FILES, "lang_ar_utf16le")));
    assert!(!Sdlproj.sniff(b"<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup/></Project>"));
    assert!(!Sdlproj.sniff(b"<html><body>hi</body></html>"));
    assert!(!Sdlproj.sniff(b"key=value\n"));
}

#[test]
fn undeclared_non_utf8_never_panics() {
    // bytes that are not UTF-8 and have no declaration are read as a code page; whatever happens, no panic and a
    // successful read merges back to the same bytes
    let mut data = b"<PackageProject Name=\"Caf".to_vec();
    data.push(0xE9);
    data.extend_from_slice(b"\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"fr\"/></PackageProject>");
    if let Ok(d) = Sdlproj.extract(&data, &opts()) {
        assert_eq!(Sdlproj.merge(&d).unwrap(), data);
    }
}
