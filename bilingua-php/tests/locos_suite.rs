//! Port of the Localization OS suite `tests/verify_php_array_filter.py`: all 12 of its cases. Same inputs and same
//! expected bytes (the two big fixtures are the suite's own, `tests/data/locos_laravel.php` and `locos_ci3.php`).
//! `test_registered` becomes the id and extension check; `test_end_to_end_upload_export` (the LocOS web app) becomes
//! the same flow on the document: extract, set one target, merge.
//! Differences from LocOS that these tests pin: a changed file keeps the file's own encoding (LocOS always wrote
//! UTF-8), and Bilingua tags replace the `unknown or paired tokens are dropped` rule (the model refuses them).
mod common;
use bilingua_model::*;
use bilingua_php::{records, Php};
use common::*;

const LARAVEL: &str = include_str!("data/locos_laravel.php");
const CI3: &str = include_str!("data/locos_ci3.php");

fn index_of(t: &[String], s: &str) -> usize {
    t.iter().position(|x| x == s).unwrap_or_else(|| panic!("{s:?} not in {t:?}"))
}

#[test]
fn registered() {
    assert_eq!(Php.id(), "php");
    assert!(Php.extensions().contains(&"php"));
    assert!(Php.sniff(LARAVEL.as_bytes()));
    assert!(Php.sniff(CI3.as_bytes()));
}

#[test]
fn parse_laravel() {
    let t = texts_str(LARAVEL);
    assert_eq!(t[0], "These credentials do not match our records.");
    assert_eq!(t[1], "The provided password is incorrect.\n");
    // :name is a free-order token (p), tags and plural pipes keep x
    assert_eq!(t[2], "Too many login attempts. Please try again in \u{27e6}p1\u{27e7} seconds.");
    assert_eq!(t[3], "Welcome, \u{27e6}p1\u{27e7}! You have \u{27e6}x2\u{27e7}\u{27e6}p3\u{27e7}\u{27e6}x4\u{27e7} messages.");
    assert_eq!(
        t[4],
        "\u{27e6}x1\u{27e7} There are none\u{27e6}x2\u{27e7}\u{27e6}x3\u{27e7} There are some\u{27e6}x4\u{27e7}\u{27e6}x5\u{27e7} There are many"
    );
    assert_eq!(t[5], "apple\u{27e6}x1\u{27e7}apples");
    assert_eq!(t[6], "It's a C:\\path value");
    assert!(t.contains(&"Deep title".to_string()) && t.contains(&"First half, second half".to_string()));
    let one = index_of(&t, "One");
    assert_eq!(&t[one..one + 3], ["One", "Two", "Three"]);
    for w in ["Fine", "Five", "Six"] {
        assert!(t.contains(&w.to_string()), "{w}");
    }
    assert!(!t.contains(&"...".to_string()) && !t.contains(&String::new()), "skipped, never a unit");
    assert!(t.iter().all(|x| x != "failed"), "keys are never units");

    let recs: std::collections::HashMap<String, String> =
        records(LARAVEL.as_bytes()).unwrap().into_iter().map(|r| (r.key, r.value)).collect();
    assert!(recs["failed"].starts_with("These credentials"));
    assert_eq!(recs["nested.deep.title"], "Deep title");
    assert_eq!(recs["nested.list.0"], "One");
    assert_eq!(recs["nested.list.3"], "Three");
    assert_eq!(recs["legacy.5"], "Five");
    assert_eq!(recs["legacy.6"], "Six");

    let doc = extract_str(LARAVEL).unwrap();
    let plural = &doc.units[4];
    let ph = |id: &str| plural.codes[id].standalone.clone().unwrap();
    assert_eq!((ph("1"), ph("3"), ph("2")), ("{0}".into(), "[1,19]".into(), "|".into()));
    // tag kinds: all standalone; :name and positional printf are free
    let welcome = &doc.units[3];
    let frees: Vec<bool> = welcome.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).map(Inline::is_free).collect();
    assert_eq!(frees, [true, false, true, false]);
    assert!(doc.units.iter().all(|u| u.codes.values().filter(|c| c.label.as_deref() == Some("placeholder")).count() == u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count()));
    // context
    assert_eq!(doc.units[0].context.key.as_deref(), Some("failed"));
    assert_eq!(doc.units[0].context.location.as_deref(), Some("line 10"));
}

#[test]
fn parse_ci3() {
    assert_eq!(
        texts_str(CI3),
        [
            "The email validation method must be passed an array.",
            "Invalid email address: \u{27e6}x1\u{27e7}",
            "Name: \u{27e6}p1\u{27e7} (\u{27e6}p2\u{27e7})",
            "Unable to connect to your database server.",
        ]
    );
    let recs = records(CI3.as_bytes()).unwrap();
    assert_eq!(recs[2].key, "form.label");
    assert_eq!(recs[2].value, "Name: %1$s (%2$d)");
}

#[test]
fn identity_round_trip() {
    for src in [LARAVEL, CI3] {
        let crlf = src.replace('\n', "\r\n");
        let bom = [&[0xEF, 0xBB, 0xBF][..], src.as_bytes()].concat();
        for data in [src.as_bytes().to_vec(), bom, crlf.into_bytes()] {
            assert_identity(&data);
            let doc = extract(&data).unwrap();
            assert_eq!(Php.merge(&doc).unwrap(), data);
        }
    }
}

#[test]
fn translated_round_trip() {
    let data = LARAVEL.as_bytes();
    let t = texts(data);
    let idx = index_of(&t, "First half, second half");
    let out = String::from_utf8(build(
        data,
        &[
            (0, "Diese Zugangsdaten stimmen nicht \u{fc}berein."),
            (1, "Das Passwort ist \"falsch\" $x.\n"),
            (2, "Zu viele Versuche. Bitte in \u{27e6}p1\u{27e7} Sekunden erneut versuchen."),
            (4, "\u{27e6}x1\u{27e7} Keine\u{27e6}x2\u{27e7}\u{27e6}x3\u{27e7} Einige\u{27e6}x4\u{27e7}\u{27e6}x5\u{27e7} Viele"),
            (6, "Es ist ein C:\\Pfad mit 'Zitat'"),
            (idx, "Erste und zweite H\u{e4}lfte"),
        ],
    ))
    .unwrap();
    assert!(out.contains("'failed' => 'Diese Zugangsdaten stimmen nicht \u{fc}berein.',"));
    assert!(out.contains("'password' => \"Das Passwort ist \\\"falsch\\\" \\$x.\\n\","), "{out}");
    assert!(out.contains("in :seconds Sekunden"));
    assert!(out.contains("'{0} Keine|[1,19] Einige|[20,*] Viele'"));
    assert!(out.contains(r"'Es ist ein C:\\Pfad mit \'Zitat\''"), "{out}");
    assert!(out.contains("'concat' => 'Erste und zweite H\u{e4}lfte',"));
    // everything else is byte-identical
    assert!(out.starts_with(&LARAVEL[..LARAVEL.find("'failed'").unwrap()]));
    assert!(out.contains("'legacy' => array('ok' => 'Fine', 5 => 'Five', 'Six'),"));
    let again = texts(out.as_bytes());
    assert!(again.contains(&"Diese Zugangsdaten stimmen nicht \u{fc}berein.".to_string()));
    assert!(again.contains(&"Das Passwort ist \"falsch\" $x.\n".to_string()));
    assert!(again.contains(&"Es ist ein C:\\Pfad mit 'Zitat'".to_string()));
}

#[test]
fn double_quote_escapes() {
    let src = "<?php return [\"a\" => \"Tab\\there \\x41\\101 \\u{e9} \\$5 \\q\", \"b\" => \"caf\\xc3\\xa9\"];";
    assert_eq!(texts_str(src), ["Tab\there AA \u{e9} $5 \\q", "caf\u{e9}"]);
}

#[test]
fn heredoc_nowdoc() {
    let src = "<?php\nreturn [\n    'h' => <<<EOT\n        Line one\n          Line two \\u{263A}\n        EOT,\n    'n' => <<<'TXT'\nRaw $not {$interp} \\n\nTXT,\n];\n";
    let t = texts_str(src);
    assert_eq!(t, ["Line one\n  Line two \u{263a}", "Raw $not {$interp} \\n"]);
    assert_identity(src.as_bytes());
    let out = build_str(src, &[(0, "Zeile eins"), (1, "Roh")]);
    assert!(out.contains("'h' => 'Zeile eins',") && out.contains("'n' => 'Roh',"), "{out}");
}

#[test]
fn placeholder_drop_and_unknown_tokens() {
    // LocOS drops a token that is not in the source or is paired; the adapter does the same before the model sees it.
    let out = build_str(LARAVEL, &[(2, "Warte \u{27e6}x1\u{27e7} \u{27e6}x9\u{27e7} \u{27e6}g1\u{27e7}Sekunden\u{27e6}/g1\u{27e7}")]);
    // the point is found by its number, x or p, as in LocOS; x9 and the pair are dropped
    assert!(out.contains("'throttle' => 'Warte :seconds  Sekunden',"), "{out}");
    let out = build_str(LARAVEL, &[(2, "Warte \u{27e6}p1\u{27e7} \u{27e6}x9\u{27e7} \u{27e6}g1\u{27e7}Sekunden\u{27e6}/g1\u{27e7}")]);
    assert!(out.contains("'throttle' => 'Warte :seconds  Sekunden',"), "{out}");
}

#[test]
fn refusals() {
    let cases: &[(&str, &str)] = &[
        ("<?php return ['a' => __('x')];", "line 1"),
        ("<?php return ['a' => env('APP_NAME')];", ""),
        ("<?php return ['a' => SOME_CONST];", ""),
        ("<?php return ['a' => \"Hello $name\"];", "interpolated"),
        ("<?php return ['a' => \"Hello {$user->name}\"];", "interpolated"),
        ("<?php return ['a' => \"Hello ${name}\"];", "interpolated"),
        ("<?php return ['a' => <<<EOT\nHi $name\nEOT];", "interpolated"),
        ("<?php return ['a' => 'x' . $y];", "concatenated"),
        ("<?php return ['a' => 'x' . 5];", "concatenated"),
        ("<?php\nnamespace App;\nreturn [];", ""),
        ("<?php system('rm -rf /'); return [];", ""),
        ("<?php return ['a' => 'b']; ?>\n<html>inline</html>", "after the closing"),
        ("<html><?php return ['a' => 'b'];", "start with <?php"),
        ("<?= 'x' ?>", "start with <?php"),
        ("<?php #[Attr]\nreturn [];", ""),
        ("<?php return ['a' => 'b'", "ends in the middle"),
        ("<?php return ['a' => 'b' /* open comment ];", ""),
        ("<?php $lang['a'] = $other['b'];", ""),
        ("<?php return ['a' => fn() => 'x'];", ""),
        ("<?php return ['a' => <<<EOT\nno end\n];", "heredoc"),
        ("<?php return ['a' => \"\\xff\"];", "UTF-8"),
    ];
    for (src, needle) in cases {
        let msg = refused(src, needle);
        assert!(msg.contains("never executed"), "{src:?}: {msg}");
        assert!(!Php.sniff(src.as_bytes()), "{src:?} must not sniff");
    }
}

#[test]
fn caps() {
    let deep = format!("<?php return {}'x'{};", "[".repeat(40), "]".repeat(40));
    refused(&deep, "nested");
    let small = |max_units| ExtractOptions::new().with_limits(Limits::new().with_max_units(max_units));
    let kind = |src: &str| match Php.extract(src.as_bytes(), &small(5)) {
        Err(Error::Limit(e)) => e.kind,
        other => panic!("{src:.40}: expected a limit error, got {:?}", other.map(|d| d.units.len())),
    };
    let many = format!("<?php return [{}];", (0..8).map(|i| format!("'k{i}' => 'Value {i}'")).collect::<Vec<_>>().join(","));
    assert_eq!(kind(&many), LimitKind::Units);
    assert_eq!(kind(&format!("<?php return [{}];", "'',".repeat(300))), LimitKind::Units);
    assert_eq!(kind(&format!("<?php {}", ";".repeat(400))), LimitKind::Units);
}

#[test]
fn encodings() {
    let src = "<?php return ['a' => 'Caf\u{e9} \u{fc}nd'];";
    let cp = encode_cp1252(src);
    assert_eq!(texts(&cp), ["Caf\u{e9} \u{fc}nd"]);
    assert_identity(&cp);
    // LocOS writes a changed file as UTF-8; Bilingua keeps Windows-1252 when the text fits (B6)
    assert_eq!(build(&cp, &[(0, "Kaffee")]), b"<?php return ['a' => 'Kaffee'];");
    assert_eq!(build(&cp, &[(0, "Kaffee \u{fc}")]), encode_cp1252("<?php return ['a' => 'Kaffee \u{fc}'];"));
    let doc = extract(&cp).unwrap();
    match &doc.skeleton {
        Skeleton::Text { encoding, .. } => assert_eq!(enc::base(encoding), "windows-1252"),
        other => panic!("{other:?}"),
    }
}

fn encode_cp1252(s: &str) -> Vec<u8> {
    let (b, _, bad) = encoding_rs::WINDOWS_1252.encode(s);
    assert!(!bad);
    b.into_owned()
}

#[test]
fn end_to_end_open_translate_merge() {
    let mut doc = extract_str(LARAVEL).unwrap();
    let id = doc.units.iter().find(|u| plain_text(&u.source()) == "Deep title").unwrap().segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Tiefer Titel")]).unwrap();
    let body = String::from_utf8(Php.merge(&doc).unwrap()).unwrap();
    assert!(body.contains("'title' => 'Tiefer Titel',"), "{body}");
    assert!(body.contains("'failed' => 'These credentials do not match our records.',"));
}
