//! Localization OS skips a value that has no letter or digit (Python `str.isalnum`). Rust's `char::is_alphanumeric`
//! also accepts some combining marks (U+0903 DEVANAGARI SIGN VISARGA, U+0345, ...), so every format that has this
//! rule uses `bilingua_model::py_isalnum_char`. A value made only of such a mark must give no unit.

use bilingua::*;

const MARK: &str = "\u{0903}";

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en")
}

fn units(name: &str, text: &str) -> usize {
    extract(name, text.as_bytes(), &opts()).unwrap_or_else(|e| panic!("{name}: {e}")).units.len()
}

type Make = fn(&str) -> String;

fn xcstrings(v: &str) -> String {
    let e = |s: &str| format!(r#""{s}":{{"localizations":{{"en":{{"stringUnit":{{"state":"new","value":"{v}"}}}}}}}}"#);
    format!(r#"{{"sourceLanguage":"en","strings":{{{},{}}},"version":"1.0"}}"#, e("a"), r#""b":{"localizations":{"en":{"stringUnit":{"state":"new","value":"hello"}}}}"#)
}

#[test]
fn mark_only_values_are_skipped_like_python() {
    let cases: Vec<(&str, Make)> = vec![
        ("a.ini", |v| format!("[s]\na={v}\nb=hello\n")),
        ("a.properties", |v| format!("a={v}\nb=hello\n")),
        ("a.strings", |v| format!("\"a\" = \"{v}\";\n\"b\" = \"hello\";\n")),
        ("a.toml", |v| format!("a = \"{v}\"\nb = \"hello\"\n")),
        ("a.yaml", |v| format!("a: \"{v}\"\nb: hello\n")),
        ("a.json5", |v| format!("{{a: \"{v}\", b: \"hello\"}}")),
        ("a.resjson", |v| format!("{{\"a\": \"{v}\", \"b\": \"hello\"}}")),
        ("a.arb", |v| format!("{{\"@@locale\":\"en\",\"a\":\"{v}\",\"b\":\"hello\"}}")),
        ("a.dtd", |v| format!("<!ENTITY a \"{v}\">\n<!ENTITY b \"hello\">\n")),
        ("a.wxl", |v| format!(r#"<WixLocalization xmlns="http://wixtoolset.org/schemas/v4/wxl" Culture="en-US"><String Id="a" Value="{v}"/><String Id="b" Value="hello"/></WixLocalization>"#)),
        ("a.ftl", |v| format!("a = {v}\nb = hello\n")),
        ("a.js", |v| format!("export default {{ a: \"{v}\", b: \"hello\" }};\n")),
        ("a.resx", |v| format!(r#"<root><resheader name="resmimetype"><value>text/microsoft-resx</value></resheader><data name="a"><value>{v}</value></data><data name="b"><value>hello</value></data></root>"#)),
        ("a.xcstrings", xcstrings),
        ("a.php", |v| format!("<?php\nreturn ['a' => '{v}', 'b' => 'hello'];\n")),
        ("a.ts", |v| format!(r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="en_US"><context><name>C</name><message><source>{v}</source><translation type="unfinished"></translation></message><message><source>hello</source><translation type="unfinished"></translation></message></context></TS>
"#)),
    ];
    for (name, make) in cases {
        let plain = units(name, &make("\u{e9}!"));
        let with_mark = units(name, &make(MARK));
        let only_hello = units(name, &make("hello"));
        assert_eq!(plain, 2, "{name}: a letter value is a unit");
        assert_eq!(only_hello, 2, "{name}");
        assert_eq!(with_mark, 1, "{name}: a combining-mark-only value must be skipped");
    }
}

#[test]
fn mark_only_html_text_is_skipped() {
    assert_eq!(units("a.html", &format!("<p>{MARK}</p><p>hello</p>")), 1);
}
