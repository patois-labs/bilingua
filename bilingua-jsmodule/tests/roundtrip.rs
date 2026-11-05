//! Round trips, languages, encodings, tags, ids and the other contract points of the filter.
mod common;
use bilingua_jsmodule::JsModule;
use bilingua_model::*;
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<_> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file() && p.file_name().is_some_and(|n| n != ".gitattributes"))
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().to_string(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect();
    v.sort();
    assert!(v.len() >= 15, "corpus went missing");
    v
}

/// "[" + source + "]" with the tags kept.
fn pseudo(v: &[Inline]) -> Vec<Inline> {
    let mut out = vec![Inline::text("[")];
    out.extend(v.iter().cloned());
    out.push(Inline::text("]"));
    out
}

fn tag_displays(u: &Unit) -> Vec<String> {
    u.source()
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { display, .. } => Some(display.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn every_corpus_file_round_trips_untouched_and_pseudo_translated() {
    for (name, bytes) in corpus() {
        assert_identity(&bytes);
        let mut doc = extract(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let before: Vec<(String, Vec<String>)> = doc
            .units
            .iter()
            .map(|u| (plain_text(&u.source()), tag_displays(u)))
            .collect();
        let ids: Vec<(String, Vec<Inline>)> = doc
            .units
            .iter()
            .filter(|u| u.translate)
            .map(|u| (u.id.clone(), u.source()))
            .collect();
        for (id, src) in &ids {
            doc.set_target(id, pseudo(src)).unwrap();
        }
        let merged = JsModule.merge(&doc).unwrap();
        let again = extract(&merged).unwrap_or_else(|e| panic!("{name}: reopen: {e}"));
        assert_eq!(again.units.len(), doc.units.len(), "{name}: unit count");
        for (u, (text, tags)) in again.units.iter().zip(&before) {
            if u.translate {
                assert_eq!(
                    plain_text(&u.source()),
                    format!("[{text}]"),
                    "{name}: {}",
                    u.id
                );
            }
            assert_eq!(&tag_displays(u), tags, "{name}: tags of {}", u.id);
        }
    }
}

#[test]
fn nothing_but_literals_changes_in_a_translated_file() {
    // Everything outside the changed literals (comments, code, types, other values) is untouched.
    let src = "// keep me\nexport default {\n  a: 'One', /* mid */ b: 2,\n  c: [\"Two\", `Three`],\n} // tail\n";
    let out = build_str(src, &[(0, "Uno"), (1, "Dos"), (2, "Tres")]);
    assert_eq!(out, "// keep me\nexport default {\n  a: 'Uno', /* mid */ b: 2,\n  c: [\"Dos\", `Tres`],\n} // tail\n");
}

#[test]
fn eleven_languages_round_trip() {
    let bytes = std::fs::read(corpus_dir().join("languages.js")).unwrap();
    let doc = extract(&bytes).unwrap();
    assert_eq!(doc.units.len(), 11);
    let langs = [
        ("en", "Welcome to Larchmoor"),
        ("ja", "ラーチムアへようこそ、{name}さん"),
        ("ar", "مرحبا بكم في لارتشمور، {name}"),
        ("ko", "라치모어에 오신 것을 환영합니다"),
        ("zh", "欢迎来到拉奇莫尔"),
    ];
    let shown: Vec<String> = doc
        .units
        .iter()
        .map(|u| locos_text(u).replace("\u{27E6}p1\u{27E7}", "{name}"))
        .collect();
    for (lang, expect) in langs {
        assert!(shown.iter().any(|t| t == expect), "{lang} missing");
    }
    // translate every unit into every language in turn: reading back gives the same text, in the same quotes
    for (_, target) in langs {
        let mut d = doc.clone();
        let ids: Vec<(String, Vec<Inline>)> =
            d.units.iter().map(|u| (u.id.clone(), u.source())).collect();
        for (id, src) in &ids {
            let has_tag = src.iter().any(|i| matches!(i, Inline::Tag { .. }));
            let mut t = vec![Inline::text(target.split("{name}").next().unwrap())];
            if has_tag {
                t.extend(
                    src.iter()
                        .filter(|i| matches!(i, Inline::Tag { .. }))
                        .cloned(),
                );
            }
            d.set_target(id, t).unwrap();
        }
        let out = JsModule.merge(&d).unwrap();
        let again = extract(&out).unwrap();
        assert_eq!(again.units.len(), 11);
        for u in &again.units {
            assert!(plain_text(&u.source()).starts_with(target.split("{name}").next().unwrap()));
        }
    }
    // Arabic keeps its quote style and the file stays valid
    let ar = build_str(
        std::str::from_utf8(&bytes).unwrap(),
        &[(
            10,
            "\u{627}\u{644}\u{633}\u{644}\u{627}\u{645} \u{27E6}p1\u{27E7}",
        )],
    );
    assert!(
        ar.contains("ar: '\u{627}\u{644}\u{633}\u{644}\u{627}\u{645} {name}',"),
        "{ar}"
    );
}

#[test]
fn legacy_and_wide_encodings_come_back_exact_when_untouched_and_utf8_when_changed() {
    for name in ["cp1252.js", "utf16le.js", "utf16be.mjs", "crlf_bom.js"] {
        let bytes = std::fs::read(corpus_dir().join(name)).unwrap();
        assert_identity(&bytes);
        let out = build(&bytes, &[(0, "Neu")]);
        if name == "crlf_bom.js" {
            assert!(out.starts_with(b"\xef\xbb\xbf"), "BOM kept");
            assert!(out.windows(2).any(|w| w == b"\r\n"), "CRLF kept");
        } else {
            assert!(
                !out.starts_with(&[0xFF]) && !out.starts_with(&[0xFE]),
                "{name}: UTF-8 out"
            );
            assert!(std::str::from_utf8(&out).is_ok());
        }
        assert_eq!(texts(&out)[0], "Neu", "{name}");
    }
    // undefined cp1252 byte and binary junk are typed errors
    assert!(extract(b"export default { a: '\x81' }").is_err());
    assert!(extract(&[0xff, 0xfe, 0x41]).is_err());
    assert!(extract(b"\x00\x01\x02\xff\xfe\x80").is_err());
}

#[test]
fn free_placeholders_move_and_ordered_ones_cannot_be_invented() {
    let mut doc = extract_str("export default { a: 'Hello {name}, you have %s items' }").unwrap();
    let src = doc.units[0].source();
    let tags: Vec<Inline> = src
        .iter()
        .filter(|i| matches!(i, Inline::Tag { .. }))
        .cloned()
        .collect();
    assert!(
        tags[0].is_free(),
        "{{name}} names its argument, so it may move"
    );
    assert!(!tags[1].is_free(), "%s keeps its order");
    let (t0, t1) = (tags[0].clone(), tags[1].clone());
    doc.set_target(
        "u1",
        vec![t1.clone(), Inline::text(" Artikel f\u{fc}r "), t0],
    )
    .unwrap();
    let out = String::from_utf8(JsModule.merge(&doc).unwrap()).unwrap();
    assert_eq!(out, "export default { a: '%s Artikel f\u{fc}r {name}' }");
    // a tag that is not in the source is refused by the model
    let bad = doc.set_target("u1", vec![Inline::tag("9", TagKind::Standalone, "%d")]);
    assert!(matches!(bad, Err(Error::InventedTag(_))));
}

#[test]
fn unit_ids_keys_and_locations_are_stable() {
    let a = extract(&std::fs::read(corpus_dir().join("vue_messages.js")).unwrap()).unwrap();
    let b = extract(&std::fs::read(corpus_dir().join("vue_messages.js")).unwrap()).unwrap();
    assert_eq!(
        a.units.iter().map(|u| &u.id).collect::<Vec<_>>(),
        b.units.iter().map(|u| &u.id).collect::<Vec<_>>()
    );
    assert_eq!(a.units[0].id, "u1");
    assert_eq!(a.units[0].context.key.as_deref(), Some("nav.home"));
    assert_eq!(a.units[0].context.kind.as_deref(), Some("string"));
    assert_eq!(a.units[0].context.location.as_deref(), Some("line 4"));
    assert_eq!(a.format, "js");
    for u in &a.units {
        assert_eq!(u.segments.len(), 1);
        assert_eq!(u.segments[0].id, u.id);
    }
}

#[test]
fn include_locked_lists_placeholder_only_values_as_locked() {
    let src = "export default { a: 'Text', b: '{{count}}', c: '---', d: '' , e: '  ' }";
    let doc = JsModule
        .extract(
            src.as_bytes(),
            &ExtractOptions {
                include_locked: true,
                ..Default::default()
            },
        )
        .unwrap();
    let flags: Vec<(String, bool)> = doc
        .units
        .iter()
        .map(|u| (plain_text(&u.source()), u.translate))
        .collect();
    assert_eq!(
        flags,
        vec![
            ("Text".to_string(), true),
            ("".to_string(), false),
            ("---".to_string(), false)
        ]
    );
    assert_eq!(JsModule.merge(&doc).unwrap(), src.as_bytes());
    // without the option they stay in the skeleton
    assert_eq!(extract_str(src).unwrap().units.len(), 1);
}

#[test]
fn sniff_claims_locale_modules_only() {
    for f in [
        "vue_messages.js",
        "typed_messages.ts",
        "i18next_resources.cjs",
        "icu_plurals.mjs",
        "arabic_rtl.mts",
        "japanese.cts",
    ] {
        assert!(
            JsModule.sniff(&std::fs::read(corpus_dir().join(f)).unwrap()),
            "{f}"
        );
    }
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    assert!(
        !JsModule.sniff(&std::fs::read(data.join("qt_linguist.ts")).unwrap()),
        "Qt Linguist XML"
    );
    assert!(!JsModule.sniff(&std::fs::read(data.join("not_a_module.js")).unwrap()));
    for no in [
        "",
        "   \n",
        "plain text, not code",
        "{\"json\": \"file\"}",
        "[1,2,3]",
        "<html></html>",
        "# Markdown\n\nBody",
    ] {
        assert!(!JsModule.sniff(no.as_bytes()), "{no:?}");
    }
    assert!(
        JsModule.extract(b"", &ExtractOptions::default()).is_ok(),
        "an empty file with a js extension opens with no units"
    );
}

#[test]
fn parser_stamp_is_checked_on_merge() {
    let mut doc = extract_str("export default { a: 'A' }").unwrap();
    match &mut doc.skeleton {
        Skeleton::Text { parser, .. } => parser.as_mut().unwrap().schema += 1,
        _ => unreachable!(),
    }
    assert!(matches!(
        JsModule.merge(&doc),
        Err(Error::ParserVersion { .. })
    ));
}

#[test]
fn strict_mode_and_edge_syntax() {
    // 'use strict', hashbang, ASI between statements, trailing commas, numeric keys, parenthesised object
    assert_eq!(
        texts_str("'use strict'\nconst a = { 1: 'One', 0x2: 'Two', }\nmodule.exports = a\n"),
        ["One", "Two"]
    );
    assert_eq!(texts_str("export default {\n  a: 'A'\n}\n;\n;"), ["A"]);
    // keys and numbers are never units; a quoted key with a dot joins the path as is
    let recs =
        bilingua_jsmodule::records(b"export default { 'a.b': { c: 'C' }, d: ['D'] }").unwrap();
    assert_eq!(
        recs.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
        ["a.b.c", "d.0"]
    );
    // a multi-line template stays multi-line when translated; a quoted one gets an escaped newline
    let out = build_str(
        "export default { a: `x\ny`, b: 'x\\ny' }",
        &[(0, "p\nq"), (1, "r\ns")],
    );
    assert_eq!(out, "export default { a: `p\nq`, b: 'r\\ns' }");
    // a translated concatenation becomes one literal in the first piece's quote style
    assert_eq!(
        build_str("export default { a: \"one \" + 'two' }", &[(0, "uno dos")]),
        "export default { a: \"uno dos\" }"
    );
}
