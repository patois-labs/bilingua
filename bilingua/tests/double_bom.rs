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
    ExtractOptions::new().with_source_lang("en").with_target_lang("es-ES")
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

/// Every text file of the oracle corpus with a second byte-order mark after its first (UTF-8, UTF-16LE or
/// UTF-16BE, whichever it has; a UTF-8 pair in front of a file without one): a filter that reads it must merge it
/// back to the exact bytes, untouched (found by the bilingua-adoc proptest on a mutated UTF-16LE fixture).
#[test]
fn doubled_bom_is_refused_or_round_trips() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus");
    let mut bad = Vec::new();
    let mut read = 0;
    let mut dirs: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().filter(|e| e.path().is_dir()).collect();
    dirs.sort_by_key(|e| e.path());
    for dir in dirs {
        let mut files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().filter(|e| e.path().is_file()).collect();
        files.sort_by_key(|e| e.path());
        for f in files {
            let data = std::fs::read(f.path()).unwrap();
            if data.starts_with(b"PK") || (data.contains(&0) && !data.starts_with(&[0xFF, 0xFE]) && !data.starts_with(&[0xFE, 0xFF])) {
                continue; // packages and binary files
            }
            let name = f.file_name().to_string_lossy().into_owned();
            let bom: &[u8] = [&[0xEFu8, 0xBB, 0xBF][..], &[0xFF, 0xFE], &[0xFE, 0xFF]].into_iter().find(|b| data.starts_with(b)).unwrap_or(&[]);
            let twice = if bom.is_empty() { [b"\xEF\xBB\xBF\xEF\xBB\xBF".as_slice(), &data].concat() } else { [bom, &data].concat() };
            let Ok(doc) = extract(&name, &twice, &opts()) else { continue };
            read += 1;
            match merge(&doc) {
                Ok(out) if out == twice => {}
                Ok(out) => bad.push(format!("{}/{name}: {} bytes in, {} out", dir.file_name().to_string_lossy(), twice.len(), out.len())),
                Err(e) => bad.push(format!("{}/{name}: merge failed: {e}", dir.file_name().to_string_lossy())),
            }
        }
    }
    assert!(read > 50, "only {read} corpus files read");
    assert!(bad.is_empty(), "{} of {read} doubled-BOM files did not merge back exactly:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn doubled_bom_is_refused_for_xaml() {
    let mut bytes = b"\xEF\xBB\xBF\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(SAMPLES[0].1.as_bytes());
    assert!(matches!(extract("a.xaml", &bytes, &opts()), Err(Error::Parse { .. })));
}
