//! Hostile input: random bytes, JavaScript-shaped noise, mutated and truncated fixtures, deep and huge files.
//! Nothing panics; a file that opens merges back to its exact bytes and survives translation.
mod common;
use bilingua_jsmodule::{records, JsModule, Limits};
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = JsModule.sniff(data);
    let _ = records(data);
    match extract(data) {
        Ok(mut doc) => {
            assert_eq!(
                JsModule.merge(&doc).unwrap(),
                data,
                "untouched merge must be exact"
            );
            let ids: Vec<(String, String)> = doc
                .units
                .iter()
                .filter(|u| u.translate)
                .map(|u| (u.segments[0].id.clone(), plain_text(&u.source())))
                .collect();
            let n = doc.units.len();
            for (id, t) in ids {
                doc.set_target(&id, vec![Inline::text(format!("~{t}~\"'`\\${{\n"))])
                    .unwrap();
            }
            let out = JsModule.merge(&doc).unwrap();
            let again = extract(&out).unwrap_or_else(|e| {
                panic!(
                    "reopen failed ({e}) for {:?} -> {:?}",
                    String::from_utf8_lossy(data),
                    String::from_utf8_lossy(&out)
                )
            });
            assert_eq!(again.units.len(), n, "unit count after translation");
        }
        Err(Error::Parse { .. } | Error::Unsupported(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file() && p.file_name().is_some_and(|n| n != ".gitattributes"))
        .map(|p| std::fs::read(p).unwrap())
        .collect();
    v.sort();
    v
}

const PIECES: &[&str] = &[
    "export",
    "default",
    "const",
    "let",
    "module",
    ".",
    "exports",
    "=",
    "{",
    "}",
    "[",
    "]",
    "(",
    ")",
    ",",
    ":",
    ";",
    "'a'",
    "\"b\"",
    "`c`",
    "`${x}`",
    "'\\u{1F600}'",
    "'\\uD83D'",
    "'\\x4",
    "'\\101'",
    "...",
    "=>",
    "+",
    "as",
    "const",
    "satisfies",
    "type",
    "interface",
    "import",
    "from",
    "<",
    ">",
    "?.",
    "x",
    "1",
    "-2.5e3",
    "0x",
    "//c\n",
    "/*c*/",
    "/*",
    "\n",
    " ",
    "\r\n",
    "\u{2028}",
    "\u{feff}",
    "#!",
    "{n, plural, one {x} other {y}}",
    "'{n, plural, one {#} other {# {a}}}'",
    "'{{a}} %s <b>'",
    "true",
    "null",
    "get",
    "async",
    "*",
    "'{'",
    "'}'",
    "'%",
    "'@:a'",
    "é",
    "日",
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
    }

    #[test]
    fn js_shaped_noise_never_panics(parts in proptest::collection::vec(proptest::sample::select(PIECES), 0..60)) {
        check(parts.concat().as_bytes());
        check(format!("export default {{ {} }}", parts.join(" ")).as_bytes());
        check(format!("export default {{ a: {} }}", parts.join("")).as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..40, edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0u8..4), 1..8)) {
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
    fn every_prefix_of_a_fixture_never_panics(idx in 0usize..40, cut in any::<u16>()) {
        let fx = fixtures();
        let d = &fx[idx % fx.len()];
        check(&d[..cut as usize % (d.len() + 1)]);
    }
}

#[test]
fn deep_nesting_is_a_typed_error() {
    for depth in [33usize, 100, 5_000, 200_000] {
        let src = format!(
            "export default {}'x'{}",
            "{a:".repeat(depth),
            "}".repeat(depth)
        );
        assert!(
            matches!(extract_str(&src), Err(Error::Unsupported(m)) if m.contains("nested")),
            "depth {depth}"
        );
        let arr = format!(
            "export default {}'x'{}",
            "[".repeat(depth),
            "]".repeat(depth)
        );
        assert!(extract_str(&arr).is_err(), "array depth {depth}");
        let paren = format!("export default {}1{}", "(".repeat(depth), ")".repeat(depth));
        assert!(extract_str(&paren).is_err(), "paren depth {depth}");
    }
    // a type with deep brackets is skipped iteratively, never recursively
    let ty = format!(
        "type T = {}number{}\nexport default {{ a: 'A' }}",
        "Array<".repeat(100_000),
        ">".repeat(100_000)
    );
    let _ = extract_str(&ty);
    // ICU nesting beyond the cap falls back to plain text
    let icu = format!(
        "export default {{ a: '{}x{}' }}",
        "{n, select, o {".repeat(2_000),
        "}}".repeat(2_000)
    );
    let _ = extract_str(&icu);
}

#[test]
fn huge_and_degenerate_files_are_typed_errors_or_fine() {
    let lim = { let mut built = Limits::default(); built.max_bytes = 1000; built };
    let big = vec![b' '; 1001];
    assert!(matches!(
        JsModule.extract_with(&big, &ExtractOptions::default(), &lim),
        Err(Error::Unsupported(_))
    ));
    // a million line breaks and a million quotes
    let lines = "\n".repeat(1_000_001);
    assert!(matches!(extract_str(&lines), Err(Error::Unsupported(m)) if m.contains("lines")));
    let quotes = format!("export default [{}]", "'',".repeat(700_000));
    assert!(matches!(extract_str(&quotes), Err(Error::Unsupported(_))));
    // one value over the character limit
    let long = format!("export default {{ a: '{}' }}", "word ".repeat(30_000));
    assert!(matches!(extract_str(&long), Err(Error::Unsupported(m)) if m.contains("longer than")));
    // quadratic traps
    let work = |make: &dyn Fn(usize) -> String, n: usize| {
        let (small, big) = (bilingua_model::work::count(|| extract_str(&make(n))).1, bilingua_model::work::count(|| extract_str(&make(2 * n))).1);
        assert!(small > 0 && big * 10 <= small * 25 + 2_000, "{small} steps for n={n}, {big} for n={}: more than linear", 2 * n);
    };
    work(&|k| format!("export default {{ a: 'x %{}z' }}", "0".repeat(k)), 40_000);
    work(&|k| format!("export default {{ a: '{}' }}", "{ ".repeat(k)), 20_000);
    work(&|k| format!("export default {{ a: '{{n, plural, {}' }}", "one {x} ".repeat(k)), 2_500);
    for s in [
        "'\\",
        "`\\",
        "'\\u{",
        "'\\u{110000}'",
        "export",
        "export default",
        "export default {",
        "export default { a:",
        "/*",
        "//",
        "#!",
        "\u{feff}",
        "`${",
    ] {
        check(s.as_bytes());
    }
}

/// The lexer does not read regular expressions. A `/` in code that a TypeScript type skip would swallow could hide a
/// template literal from it (`x = /`/ + `...`), and a string inside that real template would then be translated.
#[test]
fn a_slash_in_a_skipped_type_refuses_the_file() {
    for (src, needle) in [
        (
            "type T = string\nx = /`/ + `\nexport default { a: \"SELECT 1 FROM users\" } //`\n",
            "regular expression",
        ),
        (
            "interface I { a: string }\nfoo = /`/ + `\nexport default { a: \"rm -rf home\" } //`\n",
            "regular expression",
        ),
        (
            "interface I extends A / B { a: string }\nexport default { a: \"Hi\" }\n",
            "in an interface",
        ),
        (
            "export default { a: \"Hi\" as T / 2 }\n",
            "regular expression",
        ),
    ] {
        refused(src, needle);
        assert!(!JsModule.sniff(src.as_bytes()), "{src:?}");
    }
    // a type that only has strings with a slash in them is fine
    assert_eq!(
        texts_str("type P = 'a/b' | \"c/d\"\nexport default { a: \"Hi\" }\n"),
        vec!["Hi".to_string()]
    );
}
