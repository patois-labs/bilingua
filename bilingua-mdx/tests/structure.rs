//! LocOS tracker #790 (found by a round-trip structure fuzz): a translation must not open a code span that leaves
//! `{x}` or `<X>` live, a code fence that swallows the rest of the file, or an ESM block.
mod common;
use common::*;

fn one(src: &str, k: usize, t: &str) -> String {
    let out = build_str(src, &[(k, t)]);
    assert_eq!(texts_str(&out).len(), texts_str(src).len(), "the units stay: {out}");
    out
}

#[test]
fn an_escaped_backtick_opens_no_code_span() {
    let out = one("Hello world.\n", 0, "w \\`{x}`");
    assert_eq!(out, "w \\`\\{x\\}`\n");
    assert_eq!(texts_str(&out), vec!["w \\`\\{x\\}`"]);
}

#[test]
fn a_suffix_of_a_longer_backtick_run_opens_no_code_span() {
    let out = one("Hello world.\n", 0, "w ``<z`");
    assert_eq!(out, "w ``\\<z`\n");
}

#[test]
fn a_real_code_span_still_keeps_its_braces() {
    assert_eq!(one("Hello world.\n", 0, "w `{x}` and ``a`{y}``"), "w `{x}` and ``a`{y}``\n");
}

#[test]
fn a_code_span_does_not_cross_a_table_cell() {
    let src = "| a | b |\n|---|---|\n| cell one | cell two |\n";
    let k = texts_str(src).iter().position(|t| t.contains("cell one")).unwrap();
    let out = one(src, k, "| w `{x} | y` |");
    assert!(out.contains("| w `\\{x\\} | y` |"), "{out}");
    // a span inside one cell is still a span
    let out = one(src, k, "| w `{x}` | y |");
    assert!(out.contains("| w `{x}` | y |"), "{out}");
}

#[test]
fn backticks_around_a_code_do_not_pair_across_it() {
    let src = "Hello {count} world.\n";
    let t = texts_str(src)[0].clone();
    let tok = &t[t.find('\u{27E6}').unwrap()..t.find('\u{27E7}').unwrap() + 3];
    let out = one(src, 0, &format!("w ``{tok}`` z"));
    assert_eq!(out, "w \\`\\`{count}\\`\\` z\n");
    assert!(texts_str(&out)[0].contains('\u{27E6}'), "the expression stays a code: {out}");
    // a span inside one text run is kept
    assert_eq!(one(src, 0, &format!("w `a` {tok} z")), "w `a` {count} z\n");
}

#[test]
fn a_backslash_before_a_code_does_not_escape_it() {
    let src = "Hello <Badge>new</Badge> world.\n";
    let t = texts_str(src)[0].replace("Hello ", "w \\");
    let out = one(src, 0, &t);
    assert_eq!(out, "w \\\\<Badge>new</Badge> world.\n");
    assert!(texts_str(&out)[0].contains('\u{27E6}'), "the component stays a code: {out}");
    // an even run is already a literal backslash
    let t = texts_str(src)[0].replace("Hello ", "w \\\\");
    assert_eq!(one(src, 0, &t), "w \\\\<Badge>new</Badge> world.\n");
}

#[test]
fn a_translated_fence_is_escaped() {
    let src = "Hello world.\n\nSecond paragraph.\n";
    for (t, want) in [("```js w", "\\```js w"), ("~~~ w", "\\~~~ w"), ("  ```` w", "  \\```` w")] {
        let out = one(src, 0, t);
        assert!(out.starts_with(&format!("{want}\n")), "{t}: {out}");
        assert_eq!(texts_str(&out)[1], "Second paragraph.");
    }
}

#[test]
fn a_translated_esm_line_is_text() {
    let src = "Hello world.\n\nSecond paragraph.\n";
    let out = one(src, 0, "import x from 'y' w");
    assert!(out.starts_with("&#105;mport x from 'y' w\n"), "{out}");
    let out = one(src, 0, "export const a = 1 w");
    assert!(out.starts_with("&#101;xport const a = 1 w\n"), "{out}");
    // not at a line start, or no blank after the keyword: left alone
    assert_eq!(one(src, 0, "w import x"), "w import x\n\nSecond paragraph.\n");
    assert_eq!(one(src, 0, "importance w"), "importance w\n\nSecond paragraph.\n");
    // a list item's text is not at a line start
    assert_eq!(one("- item one\n", 0, "import x w"), "- import x w\n");
}
