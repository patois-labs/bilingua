//! Regressions found by running real files from the Internet through the pseudo round trip.
mod common;
use common::*;

#[test]
fn literal_block_under_a_bullet_stays_literal() {
    let src = b"* Run it::\n\n      $ tool\n      <doc a>\n          <item>\n\n* Next item.\n";
    assert_eq!(units(src), vec!["Run it".to_string(), "Next item.".to_string()]);
    assert_identity(src);
}

#[test]
fn literal_ending_in_a_backtick_is_one_literal() {
    let src = b"They are written as ``:role:`content```.\n";
    assert_eq!(units(src).len(), 1);
    let u = units(src);
    assert_eq!(u[0].matches("\u{27E6}").count(), 1, "one code for the whole literal: {u:?}");
    assert_identity(src);
}

#[test]
fn table_cell_does_not_wrap_inside_an_escaped_blank() {
    // the closing `*` is followed by a letter, so the writer adds `\ `; the wrap must not cut it in two
    let src = b"+-------------------------------+-------+\n| when processing match *m* now | other |\n+-------------------------------+-------+\n";
    let out = build_str(src, &[(0, "when processing match *m*qZ")]);
    assert!(!out.contains("\\n") && !out.contains("\ \n"), "{out}");
    let re = units(out.as_bytes());
    assert!(re[0].ends_with("qZ"), "{re:?}");
}

#[test]
fn indented_simple_table_can_be_written() {
    let src = b"Intro:\n\n  =========  ==========\n  name       value\n  =========  ==========\n  Abstract   one\n  Address    two\n  =========  ==========\n\nAfter.\n";
    let n = units(src).len();
    let out = build_str(src, &[(n - 1, "After it.")]);
    assert!(out.contains("After it."));
    let table = build_str(src, &[(2, "Un")]);
    assert!(table.contains("  Un        "), "{table}");
}
