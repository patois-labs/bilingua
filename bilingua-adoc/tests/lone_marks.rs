//! Structure fuzz: a lone mark character in a translation, away from a code, must not pair with a code's mark
//! further on (or before it), so the written line is formatted as the translator typed it.
mod common;
use common::*;

const DOC: &str = "A paragraph with *strong* words.\n";

fn write(t: &str) -> String {
    let src = units(DOC.as_bytes())[0].clone();
    let out = build_str(DOC.as_bytes(), &[(0, t)]);
    assert_eq!(units(out.as_bytes()).len(), 1, "{src:?} -> {out}");
    out
}

#[test]
fn a_mark_before_a_code_does_not_pair_with_it() {
    // `*}` + `**p**` read as `*}*` and `*p**`
    let out = write("*}\u{27E6}g1\u{27E7}p\u{27E6}/g1\u{27E7} w");
    assert_eq!(out, "&#42;}**p** w\n");
    let read = units(out.as_bytes())[0].clone();
    assert!(read.starts_with("*}\u{27E6}g1\u{27E7}p\u{27E6}/g1\u{27E7}"), "{read}");
}

#[test]
fn marks_inside_a_pair_do_not_close_it() {
    let out = write("w \u{27E6}g1\u{27E7}p** |\u{27E6}/g1\u{27E7} w");
    assert_eq!(out, "w *p&#42;&#42; |* w\n");
}

#[test]
fn a_mark_after_a_code_does_not_pair_with_it() {
    let out = write("w \u{27E6}g1\u{27E7}p\u{27E6}/g1\u{27E7} x *y");
    assert_eq!(out, "w *p* x &#42;y\n");
}

#[test]
fn marks_that_cannot_be_a_delimiter_stay() {
    assert_eq!(write("5 * 3 \u{27E6}g1\u{27E7}p\u{27E6}/g1\u{27E7} w"), "5 * 3 *p* w\n");
    assert_eq!(write("a*b \u{27E6}g1\u{27E7}p\u{27E6}/g1\u{27E7} w"), "a*b *p* w\n");
    // `_` is no mark of any code of the unit: typed markup stays markup
    assert_eq!(write("a _b_ \u{27E6}g1\u{27E7}p\u{27E6}/g1\u{27E7} w"), "a _b_ *p* w\n");
}
