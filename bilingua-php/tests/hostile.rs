//! Hostile input: random bytes, PHP-shaped noise, mutated and truncated fixtures, deep and huge files.
//! Nothing panics; a file that opens merges back to its exact bytes and survives translation.
mod common;
use bilingua_model::*;
use bilingua_php::{records, Php};
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = Php.sniff(data);
    let _ = records(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(Php.merge(&doc).unwrap(), data, "untouched merge must be exact");
            let ids: Vec<(String, String)> = doc
                .units
                .iter()
                .filter(|u| u.translate)
                .map(|u| (u.segments[0].id.clone(), plain_text(&u.source())))
                .collect();
            let n = doc.units.len();
            for (id, t) in ids {
                doc.set_target(&id, vec![Inline::text(format!("~{t}~Zz\"'`\\${{\n"))]).unwrap();
            }
            let out = Php.merge(&doc).unwrap();
            let again = extract(&out).unwrap_or_else(|e| {
                panic!("reopen failed ({e}) for {:?} -> {:?}", String::from_utf8_lossy(data), String::from_utf8_lossy(&out))
            });
            assert_eq!(again.units.len(), n, "unit count after translation");
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| std::fs::read(p).unwrap())
        .collect();
    v.push(include_bytes!("data/locos_laravel.php").to_vec());
    v.push(include_bytes!("data/locos_ci3.php").to_vec());
    v.sort();
    v
}

const PIECES: &[&str] = &[
    "<?php ", "<?php\n", "<?PHP ", "?>", "return", "array", "(", ")", "[", "]", ",", ";", "=>", "=", ".", "->", "::", "$lang", "$x",
    "$lang['a']", "'a'", "\"b\"", "'it\\'s'", "\"q\\\"q\"", "'", "\"", "<<<EOT\n", "<<<'N'\nx\nN", "\nEOT", "EOT;", "<<<", "<<<\"A\"\nb\nA",
    "\"\\u{1F600}\"", "\"\\u{", "\"\\x4", "\"\\101\"", "\"$x\"", "\"{$x}\"", "\"${x}\"", "'\\\\'", "// c\n", "# c\n", "/* c */", "/*", "#[A]", "//?>",
    "declare(strict_types=1);", "defined('B') OR exit('x');", "defined('B') || die;", "true", "FALSE", "null", "42", "-1.5e3", "0x1F", "1_0",
    "'x' . 'y'", "' . '", "|", "||", "\n", " ", "\r\n", "\u{2028}", "\u{feff}", "{0} a|[1,*] b", "':n %1$s <b>x</b> {x}'", "'%", "'\u{27e6}x1\u{27e7}'",
    "'{n, plural, one {#} other {# {a}}}'", "é", "日", "function", "fn", "new", "static", "namespace", "use", "echo",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2500))]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..600)) {
        check(&data);
    }

    #[test]
    fn random_text_never_panics(s in "[ -~\\t\\r\\n\\u{a0}\\u{e9}\\u{65e5}\\u{2028}\\u{feff}]{0,400}") {
        check(s.as_bytes());
        check(format!("<?php return [{s}];").as_bytes());
        check(format!("<?php return ['a' => '{s}'];").as_bytes());
        check(format!("<?php return [\"a\" => \"{s}\"];").as_bytes());
    }

    #[test]
    fn php_shaped_noise_never_panics(parts in proptest::collection::vec(proptest::sample::select(PIECES), 0..60)) {
        check(parts.concat().as_bytes());
        check(format!("<?php return [ {} ];", parts.join(" ")).as_bytes());
        check(format!("<?php return ['a' => {}];", parts.join("")).as_bytes());
        check(format!("<?php\n$lang['a'] = {};", parts.join(" ")).as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..60, edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0u8..4), 1..8)) {
        let fx = fixtures();
        let mut d = fx[idx % fx.len()].clone();
        for (pos, byte, op) in edits {
            if d.is_empty() { break; }
            let p = pos as usize % d.len();
            match op {
                0 => d[p] = byte,
                1 => d.insert(p, byte),
                2 => { d.remove(p); }
                _ => d.truncate(p),
            }
        }
        check(&d);
    }

    #[test]
    fn every_prefix_of_a_fixture_never_panics(idx in 0usize..60, cut in any::<u16>()) {
        let fx = fixtures();
        let d = &fx[idx % fx.len()];
        check(&d[..cut as usize % (d.len() + 1)]);
    }
}

#[test]
fn every_byte_prefix_of_the_main_fixtures_is_safe() {
    for name in ["data/locos_laravel.php", "data/locos_ci3.php"] {
        let d = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(name)).unwrap();
        for cut in 0..=d.len() {
            check(&d[..cut]);
        }
    }
}

#[test]
fn deep_nesting_is_a_typed_error() {
    for depth in [33usize, 100, 5_000, 200_000] {
        let arr = format!("<?php return {}'x'{};", "[".repeat(depth), "]".repeat(depth));
        assert!(matches!(extract_str(&arr), Err(Error::Unsupported(m)) if m.contains("nested")), "depth {depth}");
        let old = format!("<?php return {}'x'{};", "array(".repeat(depth), ")".repeat(depth));
        assert!(extract_str(&old).is_err(), "array() depth {depth}");
        let keyed = format!("<?php return {}'x'{};", "['a' => ".repeat(depth), "]".repeat(depth));
        assert!(extract_str(&keyed).is_err(), "keyed depth {depth}");
    }
    // 32 levels is fine
    let ok = format!("<?php return {}'x'{};", "[".repeat(32), "]".repeat(32));
    assert_eq!(texts_str(&ok), ["x"]);
}

#[test]
fn huge_and_degenerate_files_are_typed_errors_or_fine() {
    let small = |f: &dyn Fn(&mut Limits)| {
        let mut l = Limits::default();
        f(&mut l);
        ExtractOptions { limits: Some(l), ..Default::default() }
    };
    let big = vec![b' '; 1001];
    match Php.extract(&big, &small(&|l| l.max_input_bytes = 1000)) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::InputBytes),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    // a million line breaks
    let lines = format!("<?php{}", "\n".repeat(1_000_001));
    match extract_str(&lines) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Lines),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    // a million quotes and values
    let quotes = format!("<?php return [{}];", "'',".repeat(1_100_000));
    assert!(matches!(extract_str(&quotes), Err(Error::Limit(_))));
    // too many units
    let many = format!("<?php return [{}];", (0..60_000).map(|i| format!("'v{i}'")).collect::<Vec<_>>().join(","));
    match extract_str(&many) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Units),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    // one value over the character limit
    let long = format!("<?php return ['a' => '{}'];", "word ".repeat(30_000));
    match extract_str(&long) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::UnitChars),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    // quadratic traps: long runs of placeholder starts, backslashes, unterminated heredocs
    let t = std::time::Instant::now();
    for body in [
        format!("x %{}z", "0".repeat(100_000)),
        "{ ".repeat(40_000),
        "<a ".repeat(40_000),
        "[1,".repeat(30_000),
        ":a".repeat(50_000),
        "{n, ".repeat(20_000),
        "\u{27e6}".repeat(50_000),
        "|{0}".repeat(20_000),
        " ".repeat(90_000) + "{x",
    ] {
        let _ = extract_str(&format!("<?php return ['a' => '{body}'];"));
    }
    let _ = extract_str(&format!("<?php return ['a' => \"{}\"];", "\\\\".repeat(40_000)));
    let _ = extract_str(&format!("<?php return [{}", "<<<EOT\nx\n".repeat(20_000)));
    let _ = extract_str(&format!("<?php return [{}", "/* ".repeat(50_000)));
    let _ = extract_str(&format!("<?php return [{}", "'a\\".repeat(50_000)));
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
    for s in [
        "<?php", "<?php ", "<?php return", "<?php return [", "<?php return ['a' =>", "<?php return ['a' => '", "<?php /*", "<?php //", "<?php #",
        "\u{feff}", "<?php return ['a' => <<<", "<?php return ['a' => <<<EOT", "<?php return ['a' => <<<EOT\n", "<?php return ['a' => \"\\u{", "<?php $",
        "<?php $lang[", "<?php $lang['a'] = ", "<?php defined(", "<?php declare(", "<?php return ['a' => 'b'] ?",
    ] {
        check(s.as_bytes());
    }
}

#[test]
fn binary_and_wrong_encodings_are_refused() {
    assert!(matches!(extract(b"<?php return ['a' => '\x00\x01\x02\x03\x04\x05\x06\x07\x08\x0e\xff'];"), Err(Error::Encoding(_))));
    assert!(extract(&[0xFF, 0xFE, 0x3C]).is_err(), "odd UTF-16");
    assert!(extract(&[0xFE, 0xFF, 0xD8, 0x00, 0x00, 0x3C]).is_err(), "bad surrogate");
    assert!(extract(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").is_err());
}

#[test]
fn code_is_never_run_or_translated() {
    // each of these is code, not a language array: refused whole, nothing half-translated
    for src in [
        "<?php echo 'Hello world'; return ['a' => 'Text'];",
        "<?php return ['a' => sprintf('Text %s', 'x')];",
        "<?php if ($x) { return ['a' => 'Text']; }",
        "<?php return ['a' => 'Text'] + $base;",
        "<?php return array_merge($a, ['a' => 'Text']);",
        "<?php include 'other.php'; return ['a' => 'Text'];",
        "<?php eval('x'); return [];",
        "<?php return [...$base, 'a' => 'Text'];",
        "<?php return ['a' => 'Text' . PHP_EOL];",
        "<?php return ['a' => `ls`];",
        "<?php $lang['a'] = trans('x');",
    ] {
        assert!(extract_str(src).is_err(), "{src}");
        assert!(!Php.sniff(src.as_bytes()), "{src}");
    }
}
