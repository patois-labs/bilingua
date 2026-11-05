//! Every fixture in `tests/data`: round trips, the 11 languages and each encoding the filter reads.

mod common;

use bilingua_model::*;
use bilingua_xaml::Xaml;
use common::*;

include!("data_list.inc");

fn tag_count(d: &Document) -> usize {
    d.units.iter().map(|u| u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

fn encoding(d: &Document) -> String {
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => enc::base(encoding).to_string(),
        _ => String::new(),
    }
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, data) in FILES {
        for f in [&OFF, &ON] {
            let d = extract_with(f, data);
            assert_eq!(f.merge(&d).unwrap(), data, "{name}");
            assert_identity_with(f, data);
        }
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (name, data) in FILES {
        for f in [&OFF, &ON] {
            let before = extract_with(f, data);
            let mapping: Vec<(usize, String)> = before.units.iter().enumerate().map(|(i, u)| (i, format!("[{}]", show(u, &u.source())))).collect();
            let m: Vec<(usize, &str)> = mapping.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let out = translate_with(f, data, &m);
            let after = extract_with(f, &out);
            assert_eq!(after.units.len(), before.units.len(), "{name}");
            assert_eq!(tag_count(&after), tag_count(&before), "{name}");
            assert_eq!(encoding(&after), encoding(&before), "{name}: encoding kept");
            for (i, (a, b)) in after.units.iter().zip(&before.units).enumerate() {
                assert_eq!(show(a, &a.source()), format!("[{}]", show(b, &b.source())), "{name} unit {i}");
                assert_eq!(a.context.key, b.context.key, "{name}: keys never change");
            }
        }
    }
}

#[test]
fn eleven_languages_and_their_encodings() {
    let langs = ["en", "de", "fr", "es", "pt", "ru", "tr", "ar", "ja", "zh", "ko"];
    for l in langs {
        let data = FILES.iter().find(|(n, _)| *n == format!("lang_{l}")).unwrap().1;
        let u = units(data);
        assert_eq!(u.len(), 5, "{l}");
        assert!(u[1].contains("{0}") && u[1].contains("{1}"), "{l}: {}", u[1]);
        assert_eq!(u[4], "42");
    }
    // the same text read from every encoding equals the UTF-8 file
    for (enc_name, utf8, expected) in [
        ("lang_ar_utf16le", "lang_ar", "utf-16le"),
        ("lang_ar_cp1256", "", "windows-1256"),
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
        let data = FILES.iter().find(|(n, _)| *n == enc_name).unwrap().1;
        let d = extract(data);
        assert_eq!(encoding(&d), expected, "{enc_name}");
        if !utf8.is_empty() {
            let other = FILES.iter().find(|(n, _)| *n == utf8).unwrap().1;
            assert_eq!(units(data), units(other), "{enc_name}");
        }
    }
}

#[test]
fn legacy_files_translated_into_another_script_stay_readable() {
    // a Latin-1 file given Japanese text: the characters become references, the file stays Latin-1 and valid
    let data = FILES.iter().find(|(n, _)| *n == "lang_de_latin1").unwrap().1;
    let out = translate(data, &[(0, "ようこそ"), (1, "{0} と {1}")]);
    assert_eq!(units(&out)[0], "ようこそ");
    assert_eq!(units(&out)[1], "{0} と {1}");
    assert_eq!(encoding(&extract(&out)), "iso-8859-1");
    // Arabic into a Shift_JIS file
    let data = FILES.iter().find(|(n, _)| *n == "lang_ja_shiftjis").unwrap().1;
    let out = translate(data, &[(2, "حفظ وإغلاق")]);
    assert_eq!(units(&out)[2], "حفظ وإغلاق");
}

#[test]
fn euro_in_a_gbk_file_is_a_character_reference() {
    // GB18030 spells the euro A2 E3 and GBK 0x80; GBK / GB2312 readers take neither
    for name in ["lang_zh_gbk", "lang_zh_gb2312"] {
        let data = FILES.iter().find(|(n, _)| *n == name).unwrap().1;
        let out = translate(data, &[(0, "价格 €5")]);
        assert!(out.windows(7).any(|w| w == b"&#8364;"), "{name}");
        assert!(!out.windows(2).any(|w| w == [0xA2, 0xE3]) && !out.contains(&0x80), "{name}");
        assert_eq!(units(&out)[0], "价格 €5");
    }
}

#[test]
fn us_ascii_declaration_gets_character_references() {
    let data = b"<?xml version=\"1.0\" encoding=\"us-ascii\"?>\n<ResourceDictionary xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\" xmlns:sys=\"clr-namespace:System;assembly=mscorlib\">\n  <sys:String x:Key=\"a\">Save</sys:String>\n</ResourceDictionary>\n";
    assert_identity(data);
    let out = translate(data, &[(0, "Café 日本")]);
    assert!(out.is_ascii(), "{}", String::from_utf8_lossy(&out));
    assert!(String::from_utf8_lossy(&out).contains(">Caf&#233; &#26085;&#26412;<"));
    assert_eq!(units(&out), ["Café 日本"]);
}

#[test]
fn edge_file() {
    let data = FILES.iter().find(|(n, _)| *n == "edge").unwrap().1;
    let d = extract(data);
    assert_eq!(
        units(data),
        ["padded \u{263A} <ok>", "a\nb", "line1\nline2", "{ } brace", "{0}", "Use {StaticResource Key} here %s and %1$s."]
    );
    // the placeholder-looking parts of the last string are tags; the markup extension in it too (it is not at the start)
    assert!(tag_count(&d) >= 4);
    // padding, CDATA, the XML declaration with single quotes, the processing instruction and the tail comment survive
    let out = String::from_utf8(translate(data, &[(0, "x"), (2, "p\nq")])).unwrap();
    assert!(out.starts_with("<?xml version='1.0' encoding='utf-8' standalone=\"yes\"?>\n<?pi data?>\n"));
    assert!(out.contains("<x:String   x:Key = \"a\" >  x  </x:String>"), "{out}");
    assert!(out.contains("<![CDATA[cdata skipped]]>") && out.ends_with("<!-- tail -->\n"));
    assert!(out.contains(">p\r\nq<"), "CRLF source convention kept: {out:?}");
    // locked units list what is left alone
    let locked = Xaml::default().extract(data, &ExtractOptions { include_locked: true, ..opts() }).unwrap();
    assert!(locked.units.iter().any(|u| !u.translate && show(u, &u.source()) == "{Binding}"));
}

#[test]
fn view_attributes_carry_context() {
    let data = FILES.iter().find(|(n, _)| *n == "view_attributes").unwrap().1;
    let d = extract_with(&ON, data);
    let t: Vec<String> = d.units.iter().map(|u| show(u, &u.source())).collect();
    assert_eq!(t, ["Thank you", "Lumen Bakery orders", "Place order", "Sends the order & closes", "{0} loaves waiting", "_File", "Fresh bread"]);
    assert_eq!(d.units[2].context.kind.as_deref(), Some("content"));
    assert_eq!(d.units[2].context.key.as_deref(), Some("Go"));
    assert_identity_with(&ON, data);
    let ja = FILES.iter().find(|(n, _)| *n == "view_attributes_ja").unwrap().1;
    assert_eq!(units_with(&ON, ja)[2], "注文する");
}

#[test]
fn comment_before_a_string_is_a_note() {
    let data = b"<ResourceDictionary xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\"><!-- shown on the home page -->\n<x:String x:Key=\"A\">Hello</x:String></ResourceDictionary>";
    let d = extract(data);
    assert_eq!(d.units[0].context.notes, ["shown on the home page"]);
    assert_eq!(d.units[0].context.key.as_deref(), Some("A"));
}

#[test]
fn sniff() {
    use bilingua_model::Filter;
    assert!(OFF.sniff(FILES.iter().find(|(n, _)| *n == "locos_suite_wpf").unwrap().1));
    assert!(OFF.sniff(FILES.iter().find(|(n, _)| *n == "lang_ar_utf16le").unwrap().1));
    assert!(!OFF.sniff(b"<html><body>hi</body></html>"));
    assert!(!OFF.sniff(b"key=value\n"));
    assert!(!OFF.sniff(b"{\"a\":1}"));
}
