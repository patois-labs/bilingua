//! Typed `format_options`: each filter declares its keys; the facade refuses unknown keys and wrong values.

use bilingua::*;

fn keys() -> Vec<&'static str> {
    format_option_keys().iter().map(|k| k.key).collect()
}

#[test]
fn the_known_keys_are_declared() {
    let k = keys();
    for want in [
        "ftl.translate_terms",
        "csv.columns",
        "csv.target_locales",
        "odf.translate_comments",
        "docx.translate_comments",
        "xlsx.translate_comments",
        "pptx.translate_comments",
        "vsdx.translate_comments",
        "mdx.attrs",
        "mdx.front_matter_keys",
        // LocOS export keywords the adapter sends: accepted, ignored for now
        "docx.walk",
        "xlsx.walk",
        "pptx.walk",
        "xlsx.table",
        "csv.table",
        "markdown.walk",
        "markdown.md_walk",
        "markdown.fm_keys",
        "json.dialect",
    ] {
        assert!(k.contains(&want), "{want} missing from {k:?}");
    }
    let mut sorted = k.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), k.len(), "a key is declared twice");
}

#[test]
fn unknown_keys_and_bad_values_are_typed_errors() {
    let src = b"Hello world.\n";
    let bad = ExtractOptions::new().with_format_option("ftl.translate_term", true);
    match extract("a.txt", src, &bad) {
        Err(Error::FormatOption { key, msg }) => {
            assert_eq!(key, "ftl.translate_term");
            assert!(msg.contains("ftl.translate_terms"), "{msg}");
        }
        other => panic!("{other:?}"),
    }
    let bad = ExtractOptions::new().with_format_option("nosuch.thing", "1");
    assert!(matches!(extract("a.txt", src, &bad), Err(Error::FormatOption { .. })));
    let bad = ExtractOptions::new().with_format_option("odf.translate_comments", "maybe");
    assert!(matches!(extract("a.txt", src, &bad), Err(Error::FormatOption { .. })));
    // a key for another format is fine: one set of options goes with every file
    let ok = ExtractOptions::new().with_format_option("docx.translate_comments", false).with_format_option("csv.columns", "source=1");
    assert!(extract("a.txt", src, &ok).is_ok());
}

#[test]
fn csv_columns_and_target_locales_through_format_options() {
    let csv = b"id,English,German\nk1,Hello,\nk2,Bye,\n";
    let o = ExtractOptions::new()
        .with_source_lang("en")
        .with_target_lang("de")
        .with_format_option("csv.columns", "key=id, source=English, target.de=German");
    let mut doc = extract("a.csv", csv, &o).unwrap();
    assert_eq!(doc.units.len(), 2);
    assert_eq!(doc.units[0].context.key.as_deref(), Some("k1"));
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Hallo")]).unwrap();
    let out = String::from_utf8(merge(&doc).unwrap()).unwrap();
    assert!(out.contains("k1,Hello,Hallo"), "{out}");
    // the header names a language the project has: it becomes the target column
    let csv = b"English,fr\nHello,\n";
    let o = ExtractOptions::new().with_source_lang("en").with_target_lang("fr").with_format_option("csv.target_locales", "de, fr");
    let doc = extract("b.csv", csv, &o).unwrap();
    assert_eq!(doc.units.len(), 1, "{:?}", doc.units);
}

#[test]
fn pptx_comments_option_warns_that_it_is_not_supported() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/pptx");
    let Some(file) = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "pptx")) else {
        return;
    };
    let bytes = std::fs::read(&file).unwrap();
    let o = ExtractOptions::new().with_format_option("pptx.translate_comments", true);
    let doc = extract("deck.pptx", &bytes, &o).unwrap();
    assert!(doc.warnings.iter().any(|w| w.kind == "format_option"), "{:?}", doc.warnings);
}

#[test]
fn the_locos_profile() {
    assert_eq!(profile::profiles(), vec![profile::locos()]);
    assert!(matches!(extract("a.txt", b"Hi.\n", &ExtractOptions::new().with_profile("nope")), Err(Error::UnknownProfile(_))));
    // units carry LocOS's paragraph index where the format knows it, and only under the profile
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/odf");
    let file = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "odt")).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    let on = extract("a.odt", &bytes, &profile::locos().extract_options()).unwrap();
    assert!(!on.units.is_empty() && on.units.iter().all(|u| u.context.extra.contains_key(profile::P_INDEX)));
    let off = extract("a.odt", &bytes, &ExtractOptions::new()).unwrap();
    assert!(off.units.iter().all(|u| !u.context.extra.contains_key(profile::P_INDEX)));
    // the profile's merge options: UTF-8 out, except where LocOS keeps the encoding
    let m = profile::locos().merge_options("ini");
    assert_eq!((m.output_encoding, m.stamp_target_always, m.profile.as_deref()), (OutputEncoding::Keep, true, Some("locos")));
    assert_eq!(profile::locos().merge_options("html").output_encoding, OutputEncoding::Utf8);
    let doc = extract("a.txt", b"Hi.\n", &ExtractOptions::new()).unwrap();
    assert!(matches!(merge_with(&doc, &MergeOptions::new().with_profile("nope")), Err(Error::UnknownProfile(_))));
    assert!(matches!(merge_with(&doc, &MergeOptions::new().with_format_option("nope.x", 1)), Err(Error::FormatOption { .. })));
}

#[test]
fn the_source_encoding_is_reported() {
    let utf8 = extract("a.txt", "caf\u{e9}\n".as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(utf8.source_encoding, None);
    let legacy = extract("a.txt", b"caf\xe9\n", &ExtractOptions::new()).unwrap();
    assert_eq!(legacy.source_encoding.as_deref(), Some("windows-1252"));
    let mut b = vec![0xFF, 0xFE];
    for u in "[a]\r\nk=Hello\r\n".encode_utf16() {
        b.extend_from_slice(&u.to_le_bytes());
    }
    let ini = extract("a.ini", &b, &ExtractOptions::new()).unwrap();
    assert_eq!(ini.source_encoding.as_deref(), Some("utf-16le"));
}
