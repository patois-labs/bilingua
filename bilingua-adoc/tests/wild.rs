//! Regressions found by running real files from the Internet through the pseudo round trip.
mod common;
use common::*;

#[test]
fn a_word_glued_before_a_link_macro_still_reads_as_a_link() {
    let src = b"See link:https://example.org/a[the page] now.\n";
    let out = build_str(src, &[(0, "Zq\u{27E6}g1\u{27E7}la pagina\u{27E6}/g1\u{27E7}qZ")]);
    assert!(out.contains("Zqlink:https://example.org/a[la pagina]qZ"), "{out}");
    let re = units(out.as_bytes());
    assert_eq!(re.len(), 1);
    assert!(re[0].starts_with("Zq\u{27E6}g1\u{27E7}la pagina"), "{re:?}");
}

#[test]
fn xref_glued_to_a_letter_is_one_pair() {
    let re = units(b"Zqxref:other.adoc[Configure]qZ\n");
    assert_eq!(re, vec!["Zq\u{27E6}g1\u{27E7}Configure\u{27E6}/g1\u{27E7}qZ".to_string()]);
}

#[test]
fn short_macro_names_still_need_a_boundary() {
    // `system:x[y]` is prose, not the `stem:` macro
    let re = units(b"The system:x[y] runs.\n");
    assert_eq!(re, vec!["The system:x[y] runs.".to_string()]);
}

#[test]
fn setext_underline_follows_a_longer_translation() {
    let src = b"Title\n=====\n\nBody text.\n";
    let out = build_str(src, &[(0, "A much longer translated title")]);
    assert!(out.starts_with("A much longer translated title\n==============================\n"), "{out}");
    assert_eq!(units(out.as_bytes()).len(), 2);
    let short = build_str(b"A long enough title\n===================\n\nBody.\n", &[(0, "Hi")]);
    assert!(short.starts_with("Hi\n==\n"), "{short}");
}

#[test]
fn block_title_before_a_delimiter_is_not_underlined() {
    let src = b".Output\n----\ncode\n----\n";
    let out = build_str(src, &[(0, "Una salida muy larga")]);
    assert_eq!(out, ".Una salida muy larga\n----\ncode\n----\n");
}
