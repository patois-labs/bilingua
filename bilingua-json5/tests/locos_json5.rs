//! Port of the Localization OS suite `tests/verify_json5_filter.py` (identity, walk, translate, tokens, records,
//! encodings, hostile) plus its two fixtures. The suite's last two cases (`surfaces`, `end_to_end_jsonc`) test the
//! LocOS web app, API and CLI; their Bilingua equivalents are in `facade.rs`.
//!
//! LocOS counts: 9 test functions. Ported here: 7 directly (`identity`, `walk`, `translated_round_trip`, `tokens`,
//! `records`, `encodings`, `hostile`), each split into the finer tests below.

use bilingua_json5::{Json5Filter, Limits};
use bilingua_model::*;

const APP: &[u8] = include_bytes!("data/app.json5");
const SETTINGS: &[u8] = include_bytes!("data/settings.jsonc");

fn opts() -> ExtractOptions {
    ExtractOptions { source_lang: Some("en".into()), target_lang: Some("de-DE".into()), granularity: Granularity::Paragraph, include_locked: false, ..Default::default() }
}

fn open(data: &[u8]) -> Result<Document, Error> {
    Json5Filter::default().extract(data, &opts())
}

fn show(inl: &[Inline]) -> String {
    inl.iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

/// Unit texts as LocOS lists them (tags written out as the markup they stand for).
fn texts(data: &[u8]) -> Vec<String> {
    open(data).unwrap_or_else(|e| panic!("{e}")).units.iter().map(|u| show(&u.source())).collect()
}

/// LocOS `build(original, {index: text})`: sets the plain-text target of unit `index` and merges.
fn build(src: &[u8], tr: &[(usize, &str)]) -> Vec<u8> {
    let mut doc = open(src).unwrap();
    for (i, t) in tr {
        let id = doc.units[*i].segments[0].id.clone();
        doc.set_target(&id, vec![Inline::text(*t)]).unwrap();
    }
    Json5Filter::default().merge(&doc).unwrap()
}

fn refuses(data: &[u8], word: &str) {
    match open(data) {
        Ok(_) => panic!("must refuse {:?}", String::from_utf8_lossy(&data[..data.len().min(40)])),
        Err(e) => assert!(e.to_string().contains(word), "{e}"),
    }
}

fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

// ---------------------------------------------------------------- identity

#[test]
fn identity_fixtures_untouched() {
    for src in [APP, SETTINGS] {
        assert_eq!(build(src, &[]), src);
    }
}

#[test]
fn identity_every_unit_set_to_its_own_text() {
    for src in [APP, SETTINGS] {
        let same: Vec<String> = texts(src);
        let tr: Vec<(usize, &str)> = same.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
        assert_eq!(build(src, &tr), src);
    }
}

#[test]
fn identity_bom_kept() {
    let bom = b"\xef\xbb\xbf{\"a\": \"Hello\"}\n";
    assert_eq!(build(bom, &[]), bom);
    assert_eq!(build(bom, &[(0, "Hallo")]), b"\xef\xbb\xbf{\"a\": \"Hallo\"}\n");
}

#[test]
fn identity_crlf_kept() {
    let crlf = b"{\r\n  // c\r\n  a: 'Hello',\r\n  b: \"World\",\r\n}\r\n";
    let want = s(crlf).replace("'Hello'", "'Hallo'");
    assert_eq!(s(&build(crlf, &[(0, "Hallo")])), want);
}

#[test]
fn identity_original_escapes_kept() {
    let esc = br#"{"a": "caf\u00e9 \"q\"", "b": "x"}"#;
    assert_eq!(texts(esc), vec!["caf\u{e9} \"q\"", "x"]);
    let out = build(esc, &[(0, "caf\u{e9} \"q\""), (1, "y")]);
    assert_eq!(s(&out), r#"{"a": "caf\u00e9 \"q\"", "b": "y"}"#);
}

// ---------------------------------------------------------------- walk

#[test]
fn walk_app_fixture() {
    assert_eq!(
        texts(APP),
        vec![
            "Hello, {name}!",
            "Goodbye",
            "It's \"fine\"",
            "first half second half",
            "Tab\there \u{e9} A \u{b}",
            "Dollar",
            "#00ff00",
            "%1$s has %d items",
            "One",
            "Two",
            "<b>Bold</b> text",
        ]
    );
}

#[test]
fn walk_settings_fixture() {
    assert_eq!(texts(SETTINGS), vec!["My Workspace", "Welcome back, {{user}}", "Saved", "Not saved"]);
}

#[test]
fn walk_plain_json_gives_the_json_units() {
    let plain = br#"{"a": "One", "b": {"c": ["Two", 3, "---", {"d": "Three"}], "e": null}, "f": "{count}", "g": "", "h": "Four 4", "i": true}"#;
    // Same units, same order as LocOS JsonFilter (any letter or digit makes a unit).
    assert_eq!(texts(plain), vec!["One", "Two", "Three", "{count}", "Four 4"]);
}

#[test]
fn walk_top_level_string_and_number_spellings() {
    assert_eq!(texts(b"'Top level string'"), vec!["Top level string"]);
    assert_eq!(texts(b"[1, 0xFF, -Infinity, +NaN, .5, 5., 1e3, 'x1']"), vec!["x1"]);
}

// ---------------------------------------------------------------- translate

#[test]
fn translated_round_trip_keeps_everything_but_the_strings() {
    let doc = open(APP).unwrap();
    let originals = texts(APP);
    let tr: Vec<String> = originals.iter().map(|t| format!("DE {t}")).collect();
    let pairs: Vec<(usize, &str)> = tr.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
    assert_eq!(doc.units.len(), originals.len());
    let out = build(APP, &pairs);
    assert_eq!(texts(&out), tr);
    let text = s(&out).replace("\r\n", "\n");
    for kept in [
        "// App strings",
        "/* Greeting shown on the home page */",
        "// same-line note belongs to farewell",
        "$dollar_key: 'DE Dollar'",
        "count: 0x1F,",
        "ratio: .5,",
        "big: +Infinity,",
        "nothing: NaN,",
        "      '---',\n    ],",
        "greeting: 'DE Hello, {name}!'",
        "\"farewell\": \"DE Goodbye\"",
        "'quoted': 'DE It\\'s \"fine\"'",
    ] {
        assert!(text.contains(kept), "{kept}");
    }
    // the line continuation became a plain value (same decoded text)
    assert!(text.contains("multi: 'DE first half second half'"));
}

#[test]
fn translated_escaping_per_quote_style() {
    let tricky = "a'b\"c\\d\ne\rf\tg\u{1}h\u{2028}i\u{e9}";
    let src = b"{x: 'Hello', y: \"World\"}";
    let out = build(src, &[(0, tricky), (1, tricky)]);
    let want = "{x: 'a\\'b\"c\\\\d\\ne\\rf\\tg\\u0001h\\u2028i\u{e9}', y: \"a'b\\\"c\\\\d\\ne\\rf\\tg\\u0001h\\u2028i\u{e9}\"}";
    assert_eq!(s(&out), want);
    assert_eq!(texts(&out), vec![tricky, tricky]);
}

#[test]
fn translated_jsonc_keeps_comments_and_trailing_commas() {
    let out = build(SETTINGS, &[(1, "Willkommen \"zur\u{fc}ck\", {{user}}"), (3, "Nicht gespeichert")]);
    let o = s(&out);
    assert!(o.contains("\"window.welcome\": \"Willkommen \\\"zur\u{fc}ck\\\", {{user}}\","));
    assert!(o.contains("\"messages\": [\"Saved\", \"Nicht gespeichert\", \"\"],"));
    assert!(o.contains("\"**/.git\": true,\n    },"));
    assert!(o.contains("/* block\n       comment */"));
}

// ---------------------------------------------------------------- tokens

#[test]
fn tokens_are_protected_inline_tags() {
    let mut tags: Vec<String> = Vec::new();
    for src in [APP, SETTINGS] {
        for u in open(src).unwrap().units {
            for i in u.source() {
                if let Inline::Tag { display, .. } = i {
                    tags.push(display);
                }
            }
        }
    }
    for tok in ["{name}", "%1$s", "%d", "<b>", "</b>", "{{user}}"] {
        assert!(tags.iter().any(|t| t == tok), "{tok} in {tags:?}");
    }
}

#[test]
fn tokens_are_labelled_placeholder_so_the_locos_adapter_flattens_them() {
    // LocOS keeps placeholders and HTML as plain text in the unit; every tag must be a `placeholder` whose display
    // is the exact source text, so flattening gives back the LocOS unit text.
    for src in [APP, SETTINGS] {
        for u in open(src).unwrap().units {
            let mut flat = String::new();
            for i in u.source() {
                match i {
                    Inline::Text(t) => flat.push_str(&t),
                    Inline::Tag { id, display, .. } => {
                        assert_eq!(u.codes[&id].label.as_deref(), Some("placeholder"), "{display}");
                        flat.push_str(&display);
                    }
                }
            }
            assert_eq!(flat, show(&u.source()));
        }
    }
}

// ---------------------------------------------------------------- records

fn doc_keys(data: &[u8]) -> Vec<(Option<String>, String, Vec<String>)> {
    let o = ExtractOptions { include_locked: true, ..opts() };
    Json5Filter::default()
        .extract(data, &o)
        .unwrap()
        .units
        .iter()
        .map(|u| (u.context.key.clone(), show(&u.source()), u.context.notes.clone()))
        .collect()
}

#[test]
fn records_dotted_keys_comments_and_unit_order() {
    let recs = doc_keys(APP);
    let by = |k: &str| recs.iter().find(|r| r.0.as_deref() == Some(k)).unwrap_or_else(|| panic!("{k}")).clone();
    assert_eq!(by("greeting").2, vec!["Greeting shown on the home page".to_string()]);
    assert!(by("farewell").2.is_empty()); // the same-line comment follows the value
    assert_eq!(by("nested.list[1]").1, "Two");
    // '---' is counted as a string but is not a translatable unit (a locked unit here)
    let doc = Json5Filter::default().extract(APP, &ExtractOptions { include_locked: true, ..opts() }).unwrap();
    let dash = doc.units.iter().find(|u| u.context.key.as_deref() == Some("nested.list[3]")).unwrap();
    assert!(!dash.translate);
    // the unit_index of LocOS records(): position among translatable units
    let idx: Vec<usize> = doc.units.iter().filter(|u| u.translate).map(|u| u.id[1..].parse::<usize>().unwrap()).collect();
    assert_eq!(idx.len(), 11);
    let plain = open(APP).unwrap();
    assert_eq!(plain.units.len(), 11);
    assert_eq!(plain.units[0].context.key.as_deref(), Some("greeting"));
    assert_eq!(plain.units[9].context.key.as_deref(), Some("nested.list[1]"));
}

#[test]
fn records_settings_keys_and_leading_comment() {
    let doc = Json5Filter::default().extract(SETTINGS, &ExtractOptions { include_locked: true, ..opts() }).unwrap();
    let first = &doc.units[0];
    assert_eq!(first.context.key.as_deref(), Some("editor.title"));
    assert!(first.context.notes[0].contains("Editor settings"));
    assert_eq!(doc.units.last().unwrap().context.key.as_deref(), Some("messages[1]"));
    // the empty string at messages[2] has no letter: not a unit at all (blank values never are)
    let keys = bilingua_json5::keys(&doc);
    assert!(!keys.contains(&"messages[2]".to_string()));
}

#[test]
fn records_duplicate_names_are_each_a_unit() {
    assert_eq!(texts(br#"{a: "One", a: "Two"}"#), vec!["One", "Two"]);
}

// ---------------------------------------------------------------- encodings

#[test]
fn encodings_utf16_is_read_and_written_back_as_utf16() {
    let mut u16: Vec<u8> = vec![0xFF, 0xFE];
    for u in "{\"greeting\": \"Gr\u{fc}\u{df}e\"}".encode_utf16() {
        u16.extend_from_slice(&u.to_le_bytes());
    }
    assert_eq!(texts(&u16), vec!["Gr\u{fc}\u{df}e"]);
    assert_eq!(build(&u16, &[]), u16);
    let out = build(&u16, &[(0, "Hi")]);
    let mut want: Vec<u8> = vec![0xFF, 0xFE];
    for u in "{\"greeting\": \"Hi\"}".encode_utf16() {
        want.extend_from_slice(&u.to_le_bytes());
    }
    assert_eq!(out, want); // LocOS converts to UTF-8 here; Bilingua keeps the file's own encoding
}

#[test]
fn encodings_binary_is_refused() {
    let bin = b"\xff\xd8\xff\x00\x81binary\x00".repeat(10);
    assert!(matches!(open(&bin), Err(Error::Unsupported(_))));
}

// ---------------------------------------------------------------- hostile

#[test]
fn hostile_depth_cap() {
    let deep = [vec![b'['; 300], vec![b']'; 300]].concat();
    refuses(&deep, "limit");
    let ok = [vec![b'['; 256], b"'a'".to_vec(), vec![b']'; 256]].concat();
    assert_eq!(texts(&ok), vec!["a"]);
}

#[test]
fn hostile_syntax_errors_are_refused() {
    let bad: Vec<&[u8]> = vec![
        b"",
        b"// only a comment\n",
        b"{a: 'unterminated}",
        b"{a: 'x'} /* unterminated",
        b"{a: 'x\\1'}",
        b"{a: 'x\\x4'}",
        b"{a: 'x\\u12'}",
        b"{a: 'x\\01'}",
        b"{a: \"\\ud800 lone\"}",
        b"[1,,2]",
        b"{,}",
        b"{a: 1} extra",
        b"{a 1}",
        b"{1a: 'x'}",
        b"[truex]",
        b"[0x]",
        b"{a: 'line\nbreak'}",
        b"{a: 'x',",
    ];
    for b in bad {
        refuses(b, "JSON5");
    }
}

#[test]
fn hostile_surrogate_pair_joins() {
    assert_eq!(texts(br#"{a: "\ud83d\ude00 ok"}"#), vec!["\u{1f600} ok"]);
}

#[test]
fn hostile_unterminated_runs_do_not_backtrack() {
    let t0 = std::time::Instant::now();
    let a = [b"['".to_vec(), b"a\\'".repeat(300_000), b"]".to_vec()].concat();
    assert!(open(&a).is_err());
    let b = [b"[".to_vec(), b" ".repeat(300_000), b"/*".to_vec(), b"x ".repeat(300_000)].concat();
    assert!(open(&b).is_err());
    assert!(t0.elapsed().as_secs() < 5, "{:?}", t0.elapsed());
}

#[test]
fn hostile_large_real_file_parses_quickly() {
    let mut big = String::from("{\n");
    for i in 0..20000 {
        big.push_str(&format!("  // note {i}\n  key{i}: 'Value number {i}',\n"));
    }
    big.push_str("}\n");
    let t0 = std::time::Instant::now();
    let doc = open(big.as_bytes()).unwrap();
    assert_eq!(doc.units.len(), 20000);
    let out = build(big.as_bytes(), &[(19999, "Last")]);
    assert!(s(&out).ends_with("key19999: 'Last',\n}\n"));
    assert!(t0.elapsed().as_secs() < 15, "{:?}", t0.elapsed());
}

#[test]
fn hostile_budgets_and_caps() {
    let five = Json5Filter::with_limits(Limits { max_units: 5, ..Limits::default() });
    let many = format!("{{{}}}", (0..10).map(|i| format!("k{i}: 'Value {i}'")).collect::<Vec<_>>().join(","));
    let quotes = format!("[{}]", vec!["''"; 200].join(","));
    let commas = format!("[{}]", vec!["1"; 200].join(","));
    for bad in [many, quotes, commas] {
        let e = five.extract(bad.as_bytes(), &opts()).unwrap_err();
        assert!(matches!(e, Error::Parse { .. }), "{e}");
    }
    let short = Json5Filter::with_limits(Limits { max_unit_chars: 50, ..Limits::default() });
    let long = format!("{{a: '{}'}}", "word ".repeat(40));
    assert!(short.extract(long.as_bytes(), &opts()).is_err());
    let tiny = Json5Filter::with_limits(Limits { max_bytes: 10, ..Limits::default() });
    assert!(tiny.extract(b"{a: 'hello world'}", &opts()).is_err());
}
