//! Merge options through the facade: `stamp_target_always` and `output_encoding`, and the generic
//! `format_options` map. The defaults leave an untouched merge byte for byte.

use bilingua::*;

fn opts(target: &str) -> ExtractOptions {
    ExtractOptions { source_lang: Some("en".into()), target_lang: Some(target.into()), ..Default::default() }
}

fn always() -> MergeOptions {
    MergeOptions { stamp_target_always: true, ..MergeOptions::default() }
}

fn utf8() -> MergeOptions {
    MergeOptions { output_encoding: OutputEncoding::Utf8, ..MergeOptions::default() }
}

fn text(b: Vec<u8>) -> String {
    String::from_utf8(b).unwrap()
}

#[test]
fn xliff_12_target_language_is_written_when_translated_or_always() {
    let src = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2"><file original="a.txt" source-language="en" datatype="plaintext"><body><trans-unit id="1"><source>Hello</source></trans-unit></body></file><file original="b.txt" source-language="en" target-language="fr" datatype="plaintext"><body><trans-unit id="2"><source>World</source></trans-unit></body></file></xliff>"#;
    let mut doc = extract("a.xlf", src.as_bytes(), &opts("es-ES")).unwrap();
    // nothing translated: the original bytes, no language
    assert_eq!(merge(&doc).unwrap(), src.as_bytes());
    // always: every <file> declares the target language
    let all = text(merge_with(&doc, &always()).unwrap());
    assert_eq!(all.matches(r#"target-language="es-ES""#).count(), 2, "{all}");
    assert!(!all.contains(r#"target-language="fr""#));
    // translated: the same by default
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Hola")]).unwrap();
    let out = text(merge(&doc).unwrap());
    assert_eq!(out.matches(r#"target-language="es-ES""#).count(), 2, "{out}");
    assert!(out.contains("Hola"));
    // and it reads back with the language and the unit count
    let back = extract("a.xlf", out.as_bytes(), &opts("es-ES")).unwrap();
    assert_eq!(back.units.len(), 2);
    assert_eq!(back.target_lang.as_deref(), Some("es-ES"));
}

#[test]
fn xliff_2_trglang_is_written() {
    let src = r#"<?xml version="1.0"?>
<xliff version="2.0" xmlns="urn:oasis:names:tc:xliff:document:2.0" srcLang="en"><file id="f1"><unit id="1"><segment><source>Hello</source></segment></unit></file></xliff>"#;
    let mut doc = extract("a.xlf", src.as_bytes(), &opts("de")).unwrap();
    assert_eq!(merge(&doc).unwrap(), src.as_bytes());
    assert!(text(merge_with(&doc, &always()).unwrap()).contains(r#"trgLang="de""#));
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Hallo")]).unwrap();
    let out = text(merge(&doc).unwrap());
    assert!(out.contains(r#"trgLang="de""#) && out.contains("Hallo"), "{out}");
    assert_eq!(out.matches("trgLang").count(), 1);
}

#[test]
fn yaml_root_key_follows_the_target_only_when_asked_or_translated() {
    let src = "en:\n  hello: Hello\n";
    let doc = extract("a.yml", src.as_bytes(), &opts("es")).unwrap();
    assert_eq!(merge(&doc).unwrap(), src.as_bytes());
    let out = text(merge_with(&doc, &always()).unwrap());
    assert!(out.starts_with("es:"), "{out}");
}

#[test]
fn arb_locale_is_stamped_when_asked() {
    let src = "{\n  \"@@locale\": \"en\",\n  \"hello\": \"Hello\"\n}\n";
    let doc = extract("a.arb", src.as_bytes(), &opts("es-ES")).unwrap();
    assert_eq!(merge(&doc).unwrap(), src.as_bytes());
    let out = text(merge_with(&doc, &always()).unwrap());
    assert!(out.contains("\"@@locale\": \"es_ES\"") || out.contains("\"@@locale\": \"es-ES\""), "{out}");
}

#[test]
fn wxl_culture_is_stamped_when_asked() {
    let src = r#"<?xml version="1.0" encoding="utf-8"?>
<WixLocalization xmlns="http://wixtoolset.org/schemas/v4/wxl" Culture="en-US"><String Id="a" Value="Hello"/></WixLocalization>"#;
    let doc = extract("a.wxl", src.as_bytes(), &opts("es-ES")).unwrap();
    assert_eq!(merge(&doc).unwrap(), src.as_bytes());
    assert!(text(merge_with(&doc, &always()).unwrap()).to_lowercase().contains(r#"culture="es-es""#));
}

#[test]
fn ftl_plurals_are_filled_when_asked() {
    let src = "emails = { $n ->\n    [one] One email\n   *[other] { $n } emails\n}\n";
    let doc = extract("a.ftl", src.as_bytes(), &opts("ru")).unwrap();
    assert_eq!(merge(&doc).unwrap(), src.as_bytes());
    let out = text(merge_with(&doc, &always()).unwrap());
    assert!(out.contains("[few]") && out.contains("[many]"), "{out}");
}

#[test]
fn default_options_equal_plain_merge_everywhere() {
    for (name, src) in [("a.html", "<p>Hello</p>"), ("a.json", "{\"a\": \"Hello\"}"), ("a.po", "msgid \"Hello\"\nmsgstr \"\"\n"), ("a.srt", "1\n00:00:01,000 --> 00:00:02,000\nHello\n")] {
        let doc = extract(name, src.as_bytes(), &opts("es")).unwrap();
        assert_eq!(merge(&doc).unwrap(), merge_with(&doc, &MergeOptions::default()).unwrap(), "{name}");
    }
}

#[test]
fn output_utf8_rewrites_a_legacy_html_file() {
    let mut src = b"<html><head><meta charset=\"windows-1252\"></head><body><p>caf".to_vec();
    src.push(0xE9);
    src.extend_from_slice(b"</p></body></html>");
    let doc = extract("a.html", &src, &opts("es")).unwrap();
    // default: the file's own encoding, byte for byte
    assert_eq!(merge(&doc).unwrap(), src);
    let out = merge_with(&doc, &utf8()).unwrap();
    let s = text(out);
    assert!(s.contains("caf\u{e9}") && s.contains("charset=\"UTF-8\"") && !s.contains("windows-1252"), "{s}");
}

#[test]
fn output_utf8_for_a_json_utf16_file_and_srt_in_cp1252() {
    let src = "{\"a\": \"caf\u{e9}\"}\r\n";
    let mut b = vec![0xFF, 0xFE];
    for u in src.encode_utf16() {
        b.extend_from_slice(&u.to_le_bytes());
    }
    let doc = extract("a.json", &b, &opts("es")).unwrap();
    assert_eq!(merge(&doc).unwrap(), b);
    assert_eq!(merge_with(&doc, &utf8()).unwrap(), src.as_bytes());
    let srt = b"1\r\n00:00:01,000 --> 00:00:02,000\r\ncaf\xE9\r\n".to_vec();
    let doc = extract("a.srt", &srt, &opts("es")).unwrap();
    assert_eq!(merge(&doc).unwrap(), srt);
    assert_eq!(merge_with(&doc, &utf8()).unwrap(), "1\r\n00:00:01,000 --> 00:00:02,000\r\ncaf\u{e9}\r\n".as_bytes());
}

#[test]
fn ftl_translate_terms_format_option() {
    let src = "-brand = Firefox\nhello = Hello\n";
    let plain = extract("a.ftl", src.as_bytes(), &opts("es")).unwrap();
    let mut o = opts("es");
    o.format_options.insert("ftl.translate_terms".into(), "true".into());
    let terms = extract("a.ftl", src.as_bytes(), &o).unwrap();
    assert_eq!(plain.units.len(), 1);
    assert_eq!(terms.units.len(), 2);
}
