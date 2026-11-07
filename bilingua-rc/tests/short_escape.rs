//! #791: a short escape followed by a translated digit would read as a longer escape (`\x4` + `1` is `\x41`, `A`;
//! `\1` + `2` is `\12`). rc.exe reads at most 2 hex / 3 octal digits in a narrow string and 4 hex / 7 octal digits
//! in a wide one, so the escape is padded with leading zeros to that width: the same character, and the same unit
//! text when the file is read again.
mod common;
use bilingua_model::*;
use bilingua_rc::Rc;
use common::*;

fn merge(src: &str, target: &str) -> Vec<u8> {
    let mut doc = extract(src.as_bytes()).unwrap();
    let u = &doc.units[0];
    let id = u.segments[0].id.clone();
    let tg = target_of(u, target);
    doc.set_target(&id, tg).unwrap();
    Rc.merge(&doc).unwrap()
}

const X: &str = "\u{27e6}x1\u{27e7}";

fn check(src: &str, target: &str, out: &str) {
    let t = target.replace("{x}", X);
    let got = merge(src, &t);
    assert_eq!(String::from_utf8_lossy(&got), out, "{src} <- {target}");
    // read back: the escape is still one code, the digit still text
    assert_eq!(units(&got), vec![t], "{out}");
}

#[test]
fn narrow_short_escape_before_a_digit_is_padded() {
    for (src, target, out) in [
        (r#"STRINGTABLE { 1 "\x4 a" }"#, "{x}1 a", r#"STRINGTABLE { 1 "\x041 a" }"#),
        (r#"STRINGTABLE { 1 "\x4 a" }"#, "{x}f a", r#"STRINGTABLE { 1 "\x04f a" }"#),
        (r#"STRINGTABLE { 1 "\X4 a" }"#, "{x}B a", r#"STRINGTABLE { 1 "\X04B a" }"#),
        (r#"STRINGTABLE { 1 "\1 a" }"#, "{x}2 a", r#"STRINGTABLE { 1 "\0012 a" }"#),
        (r#"STRINGTABLE { 1 "\12 a" }"#, "{x}7 a", r#"STRINGTABLE { 1 "\0127 a" }"#),
        (r#"STRINGTABLE { 1 "\0 a" }"#, "b{x}0", r#"STRINGTABLE { 1 "b\0000" }"#),
    ] {
        check(src, target, out);
    }
}

#[test]
fn wide_short_escape_before_a_digit_is_padded() {
    for (src, target, out) in [
        // `\x4` is not printable, so it stays a code in a wide string
        (r#"STRINGTABLE { 1 L"\x4 a" }"#, "{x}1 a", r#"STRINGTABLE { 1 L"\x00041 a" }"#),
        (r#"STRINGTABLE { 1 L"\x004 a" }"#, "{x}e a", r#"STRINGTABLE { 1 L"\x0004e a" }"#),
        // 3 octal digits are full width in a narrow string but not in a wide one
        (r#"STRINGTABLE { 1 L"\1 a" }"#, "{x}2 a", r#"STRINGTABLE { 1 L"\00000012 a" }"#),
        (r#"STRINGTABLE { 1 L"\101 a" }"#, "{x}7 a", r#"STRINGTABLE { 1 L"\00001017 a" }"#),
    ] {
        check(src, target, out);
    }
}

#[test]
fn escapes_that_cannot_join_are_written_as_before() {
    for (src, target, out) in [
        (r#"STRINGTABLE { 1 "\x4 a" }"#, "{x} 1 a", r#"STRINGTABLE { 1 "\x4 1 a" }"#),
        (r#"STRINGTABLE { 1 "\x4 a" }"#, "{x}z a", r#"STRINGTABLE { 1 "\x4z a" }"#),
        (r#"STRINGTABLE { 1 "\x41 a" }"#, "{x}1 a", r#"STRINGTABLE { 1 "\x411 a" }"#),
        (r#"STRINGTABLE { 1 "\1 a" }"#, "{x}8 a", r#"STRINGTABLE { 1 "\18 a" }"#),
        (r#"STRINGTABLE { 1 "\101 a" }"#, "{x}7 a", r#"STRINGTABLE { 1 "\1017 a" }"#),
        (r#"STRINGTABLE { 1 L"\x0004 a" }"#, "{x}1 a", r#"STRINGTABLE { 1 L"\x00041 a" }"#),
        (r#"STRINGTABLE { 1 "\n a" }"#, "{x}1 a", r#"STRINGTABLE { 1 "\n1 a" }"#),
    ] {
        check(src, target, out);
    }
}

#[test]
fn wide_octal_reads_up_to_seven_digits() {
    // rc.exe reads `\0000101` in a wide string as one escape (`A`); a narrow string stops after three digits
    assert_eq!(units(br#"STRINGTABLE { 1 L"\0000101 a" }"#), vec![format!("{X} a")]);
    assert_eq!(units(br#"STRINGTABLE { 1 "\0000101 a" }"#), vec![format!("{X}0101 a")]);
}
