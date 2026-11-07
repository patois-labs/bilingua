//! #791: a short escape followed by a translated digit would read as a longer escape (`\x4` + `1` is `\x41`, `A`;
//! `\1` + `2` is `\12`). Such a merge is refused; a space between them, or a full-length escape, is fine.
mod common;
use bilingua_model::*;
use bilingua_rc::Rc;
use common::*;

fn merge(src: &str, target: &str) -> Result<Vec<u8>, Error> {
    let mut doc = extract(src.as_bytes()).unwrap();
    let u = &doc.units[0];
    let id = u.segments[0].id.clone();
    let tg = target_of(u, target);
    doc.set_target(&id, tg).unwrap();
    Rc.merge(&doc)
}

#[test]
fn short_escape_before_a_translated_digit_is_refused() {
    for (src, target) in [
        (r#"STRINGTABLE { 1 "\x4 a" }"#, "\u{27e6}x1\u{27e7}1 a"),
        (r#"STRINGTABLE { 1 "\x4 a" }"#, "\u{27e6}x1\u{27e7}f a"),
        (r#"STRINGTABLE { 1 "\1 a" }"#, "\u{27e6}x1\u{27e7}2 a"),
        (r#"STRINGTABLE { 1 "\12 a" }"#, "\u{27e6}x1\u{27e7}7 a"),
        (r#"STRINGTABLE { 1 L"\x004 a" }"#, "\u{27e6}x1\u{27e7}1 a"),
    ] {
        match merge(src, target) {
            Err(Error::Unwritable { reason, .. }) => assert!(reason.contains("would join it"), "{reason}"),
            other => panic!("expected a refusal for {src:?}, got {other:?}"),
        }
    }
}

#[test]
fn escapes_that_cannot_join_are_written_as_before() {
    for (src, target, out) in [
        (r#"STRINGTABLE { 1 "\x4 a" }"#, "\u{27e6}x1\u{27e7} 1 a", r#"STRINGTABLE { 1 "\x4 1 a" }"#),
        (r#"STRINGTABLE { 1 "\x4 a" }"#, "\u{27e6}x1\u{27e7}z a", r#"STRINGTABLE { 1 "\x4z a" }"#),
        (r#"STRINGTABLE { 1 "\x41 a" }"#, "\u{27e6}x1\u{27e7}1 a", r#"STRINGTABLE { 1 "\x411 a" }"#),
        (r#"STRINGTABLE { 1 "\1 a" }"#, "\u{27e6}x1\u{27e7}8 a", r#"STRINGTABLE { 1 "\18 a" }"#),
    ] {
        assert_eq!(merge(src, target).unwrap(), out.as_bytes(), "{src}");
    }
}
