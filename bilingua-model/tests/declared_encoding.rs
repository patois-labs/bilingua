//! A declared encoding (XML declaration, meta charset, or a byte order mark in a format that declares its charset)
//! is binding: bytes that are not valid in it are refused, never read with a fallback code page. An undeclared
//! format is still sniffed, as LocOS's `read_text_upload` does.

use bilingua_model::enc::{decode_text, DecodeOptions};
use bilingua_model::{Error, Limits};

fn declared() -> DecodeOptions<'static> {
    DecodeOptions::with_limits(Limits::default()).declared()
}

fn plain() -> DecodeOptions<'static> {
    DecodeOptions::with_limits(Limits::default())
}

#[test]
fn a_file_that_says_utf8_but_is_not_is_refused() {
    let b = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><a>caf\xe9</a>";
    assert!(matches!(decode_text(b, &declared()), Err(Error::Encoding(m)) if m.contains("UTF-8")));
}

#[test]
fn bad_bytes_in_a_declared_code_page_are_refused() {
    // 0x81 0x20: a Shift_JIS lead byte followed by a byte that is no trail byte
    let b = b"<?xml version=\"1.0\" encoding=\"Shift_JIS\"?><a>\x81\x20</a>";
    assert!(matches!(decode_text(b, &declared()), Err(Error::Encoding(_))));
    // the same file in valid Shift_JIS reads
    let ok = b"<?xml version=\"1.0\" encoding=\"Shift_JIS\"?><a>\x82\xa0</a>";
    let d = decode_text(ok, &declared()).unwrap();
    assert!(d.text.ends_with("<a>\u{3042}</a>") && d.encoding.starts_with("shift_jis"), "{d:?}");
}

#[test]
fn a_bom_is_a_declaration_in_a_declared_format() {
    let b = b"\xef\xbb\xbfcaf\xe9\n";
    assert!(matches!(decode_text(b, &declared()), Err(Error::Encoding(_))));
    assert!(matches!(decode_text(b"\xff\xfe\x00\xd8", &declared()), Err(Error::Encoding(_))));
    // an undeclared format falls back as LocOS's read_text_upload does (Windows-1252, the BOM bytes included)
    assert_eq!(decode_text(b, &plain()).unwrap().text, "\u{ef}\u{bb}\u{bf}caf\u{e9}\n");
    // LocOS's plain text and subtitle readers: UTF-8 with replacement characters
    let d = decode_text(b, &plain().lossy_after_utf8_bom()).unwrap();
    assert_eq!((d.text.as_str(), d.encoding.as_str()), ("caf\u{fffd}\n", "utf-8-bom"));
}

#[test]
fn an_undeclared_file_is_still_sniffed() {
    let d = decode_text(b"caf\xe9\n", &plain()).unwrap();
    assert_eq!(d.text, "caf\u{e9}\n");
    assert_eq!(d.encoding, "windows-1252");
}
