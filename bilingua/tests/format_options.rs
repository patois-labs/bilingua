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
