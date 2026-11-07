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
    // a drawing declares its code page and AutoCAD before 2007 has no UTF-8: LocOS keeps it (lead decision 11-05)
    for keep in ["strings", "dxf"] {
        assert_eq!(profile::locos().merge_options(keep).output_encoding, OutputEncoding::Keep, "{keep}");
    }
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

fn family(k: &str) -> &str {
    k.split('.').next().unwrap_or("")
}

/// The facade accepts a key when some filter declares it; the filter the file goes to then checks the keys of its
/// own families again. Both checks must agree, or a key the facade lets through would fail inside the filter.
#[test]
fn every_filter_declares_every_key_of_its_own_families() {
    let all = format_option_keys();
    for f in filters() {
        let mine: Vec<&str> = f.format_option_keys().iter().map(|k| k.key).collect();
        for k in &all {
            let own = family(k.key) == f.id() || mine.iter().any(|m| family(m) == family(k.key));
            if own {
                assert!(mine.contains(&k.key), "{} owns the {} family but does not declare {}", f.id(), family(k.key), k.key);
            }
        }
        // and every key it declares is accepted by its own check
        let opts: std::collections::BTreeMap<String, String> = f
            .format_option_keys()
            .iter()
            .map(|k| (k.key.to_string(), if k.kind == OptionKind::Int { "1".into() } else { "true".into() }))
            .collect();
        check_own_options(f.as_ref(), &opts).unwrap_or_else(|e| panic!("{}: {e}", f.id()));
    }
}

/// Called directly (not through the facade), a filter refuses a misspelt key of its own format and a bad value
/// for one of its keys, at extract and at merge; keys of other formats are left to the facade.
#[test]
fn a_filter_called_directly_refuses_its_own_unknown_keys() {
    use bilingua_csv::CsvFilter;
    use bilingua_ftl::FtlFilter;
    use bilingua_ini::IniFilter;
    let csv = b"id,English,German\nk1,Hello,\nk2,Bye,\n";
    let typo = ExtractOptions::new().with_format_option("csv.colums", "source=English");
    match CsvFilter::default().extract(csv, &typo) {
        Err(Error::FormatOption { key, msg }) => assert!(key == "csv.colums" && msg.contains("csv.columns"), "{key}: {msg}"),
        other => panic!("{other:?}"),
    }
    let ok = ExtractOptions::new().with_format_option("csv.columns", "key=id, source=English, target.de=German").with_target_lang("de");
    assert_eq!(CsvFilter::default().extract(csv, &ok).unwrap().units.len(), 2);
    // another format's keys are not this filter's business
    let foreign = ExtractOptions::new().with_format_option("docx.walk", "x").with_format_option("nosuch.key", "1");
    let doc = IniFilter.extract(b"[a]\nk=Hello\n", &foreign).unwrap();
    // a key in the filter's own family (its id), even though it declares no keys at all
    let own = ExtractOptions::new().with_format_option("ini.anything", "1");
    assert!(matches!(IniFilter.extract(b"[a]\nk=Hello\n", &own), Err(Error::FormatOption { .. })));
    assert!(matches!(IniFilter.merge_with(&doc, &MergeOptions::new().with_format_option("ini.anything", 1)), Err(Error::FormatOption { .. })));
    assert!(IniFilter.merge_with(&doc, &MergeOptions::new().with_format_option("docx.walk", 1)).is_ok());
    // a declared key with a value of the wrong kind
    let bad = ExtractOptions::new().with_format_option("ftl.translate_terms", "maybe");
    assert!(matches!(FtlFilter.extract(b"hello = Hello\n", &bad), Err(Error::FormatOption { .. })));
}

#[test]
fn the_locos_profile_reports_the_source_encoding_of_markdown_po_and_srt() {
    let o = profile::locos().extract_options();
    let md = extract("a.md", b"# Caf\xe9 menu\n\nCr\xe8me br\xfbl\xe9e.\n", &o).unwrap();
    assert_eq!(md.source_encoding.as_deref(), Some("windows-1252"));
    let po = extract("a.po", b"msgid \"Caf\xe9\"\nmsgstr \"\"\n", &o).unwrap();
    assert_eq!(po.source_encoding.as_deref(), Some("windows-1252"));
    let srt = extract("a.srt", b"1\n00:00:01,000 --> 00:00:02,000\nCaf\xe9 au lait\n", &o).unwrap();
    assert_eq!(srt.source_encoding.as_deref(), Some("windows-1252"));
    // UTF-8 (with or without a byte order mark) reports nothing
    for (name, bytes) in [("a.md", "# Caf\u{e9} menu\n".as_bytes()), ("b.md", "\u{feff}# Caf\u{e9} menu\n".as_bytes()), ("a.po", "msgid \"Caf\u{e9}\"\nmsgstr \"\"\n".as_bytes())] {
        assert_eq!(extract(name, bytes, &o).unwrap().source_encoding, None, "{name}");
    }
    // a changed Markdown file is written as UTF-8, whatever it was read as (LocOS)
    let mut doc = extract("a.md", b"# Caf\xe9 menu\n\nCr\xe8me br\xfbl\xe9e.\n", &o).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Men\u{fc}")]).unwrap();
    let out = merge_with(&doc, &profile::locos().merge_options("markdown")).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "# Men\u{fc}\n\nCr\u{e8}me br\u{fb}l\u{e9}e.\n");
    // the report survives a JSON round trip, and so does the merge
    let again: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(again.source_encoding.as_deref(), Some("windows-1252"));
    assert_eq!(merge_with(&again, &profile::locos().merge_options("markdown")).unwrap(), merge_with(&doc, &profile::locos().merge_options("markdown")).unwrap());
}
