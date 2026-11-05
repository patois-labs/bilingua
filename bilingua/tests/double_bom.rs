//! A second UTF-8 byte-order mark in front of an XML or markup file must give a typed error or be read, never a
//! panic (found by a proptest in bilingua-xaml: quick-xml skips a leading BOM and its offsets then no longer
//! match the text).

use bilingua::*;

const SAMPLES: &[(&str, &str)] = &[
    ("a.xaml", r#"<Page xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"><TextBlock Text="Hello"/></Page>"#),
    ("a.resx", r#"<root><resheader name="resmimetype"><value>text/microsoft-resx</value></resheader><data name="a"><value>Hello</value></data></root>"#),
    ("a.wxl", r#"<WixLocalization xmlns="http://wixtoolset.org/schemas/v4/wxl" Culture="en-US"><String Id="a" Value="Hello"/></WixLocalization>"#),
    ("a.xlf", r#"<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2"><file original="a" source-language="en" datatype="plaintext"><body><trans-unit id="1"><source>Hello</source></trans-unit></body></file></xliff>"#),
    ("a.html", "<p>Hello</p>"),
    ("a.xmb", "<messagebundle><msg id=\"1\">Hello</msg></messagebundle>"),
    ("a.xtb", "<translationbundle lang=\"es\"><translation id=\"1\">Hola</translation></translationbundle>"),
    ("a.ts", "<!DOCTYPE TS><TS version=\"2.1\" language=\"en_US\"><context><name>C</name><message><source>Hello</source><translation type=\"unfinished\"></translation></message></context></TS>"),
    ("a.php", "<?php
return ['a' => 'Hello'];
"),
    ("a.xcstrings", "{\"sourceLanguage\":\"en\",\"strings\":{\"a\":{\"localizations\":{\"en\":{\"stringUnit\":{\"state\":\"new\",\"value\":\"Hello\"}}}}},\"version\":\"1.0\"}"),
    ("a.arb", "{\"@@locale\":\"en\",\"a\":\"Hello\"}"),
    ("a.dtd", "<!ENTITY a \"Hello\">\n"),
    ("a.json", "{\"a\": \"Hello\"}"),
    ("a.po", "msgid \"Hello\"\nmsgstr \"\"\n"),
    ("a.ini", "[s]\na=Hello\n"),
    ("a.ftl", "a = Hello\n"),
    ("a.yaml", "a: Hello\n"),
    ("a.properties", "a=Hello\n"),
    ("a.strings", "\"a\" = \"Hello\";\n"),
    ("a.toml", "a = \"Hello\"\n"),
];

fn opts() -> ExtractOptions {
    ExtractOptions { source_lang: Some("en".into()), target_lang: Some("es-ES".into()), ..Default::default() }
}

#[test]
fn doubled_bom_never_panics() {
    for (name, text) in SAMPLES {
        for bom in 1..=3 {
            let mut bytes = b"\xEF\xBB\xBF".repeat(bom);
            bytes.extend_from_slice(text.as_bytes());
            let r = extract(name, &bytes, &opts());
            if let Ok(d) = &r {
                let _ = merge(d);
            }
            if bom == 1 {
                assert!(r.is_ok(), "{name} with one BOM: {:?}", r.err());
            }
        }
    }
}

#[test]
fn doubled_bom_is_refused_for_xaml() {
    let mut bytes = b"\xEF\xBB\xBF\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(SAMPLES[0].1.as_bytes());
    assert!(matches!(extract("a.xaml", &bytes, &opts()), Err(Error::Parse { .. })));
}
