use super::*;
use crate::scan::{escape, unescape};

#[test]
fn unescape_cases() {
    assert_eq!(unescape(r#"a\nb\tc\\d\"e"#), "a\nb\tc\\d\"e");
    assert_eq!(unescape(r"Caf\U00e9 é"), "Caf\u{e9} \u{e9}");
    assert_eq!(unescape(r"\UD83D\UDE00"), "\u{1F600}");
    assert_eq!(unescape(r"\101\0\7x"), "A\0\u{7}x");
    assert_eq!(unescape(r"\a\b\f\v\'\q"), "\u{7}\u{8}\u{c}\u{b}'q");
    assert_eq!(unescape(r"\Uzz"), "Uzz");
    assert_eq!(unescape(r"\UD800x"), "\u{fffd}x");
    assert_eq!(unescape("tail\\"), "tail\\");
    assert_eq!(unescape("h\u{e9}llo \\n \u{65e5}"), "h\u{e9}llo \n \u{65e5}");
}

#[test]
fn escape_cases() {
    assert_eq!(escape("a\"b\\c\nd\re\tf"), r#"a\"b\\c\nd\re\tf"#);
}

#[test]
fn split_ws_cases() {
    assert_eq!(split_ws("  hi \n"), ("  ", "hi", " \n"));
    assert_eq!(split_ws("   "), ("   ", "", ""));
    assert_eq!(split_ws("x"), ("", "x", ""));
}

#[test]
fn encodings_by_name() {
    for e in [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16LeBom, Enc::Utf16BeBom, Enc::Utf16Le, Enc::Utf16Be] {
        assert_eq!(Enc::from_name(e.name()), Some(e));
    }
}
