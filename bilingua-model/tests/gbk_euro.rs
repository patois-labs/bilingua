//! GBK euro: encode_text writes 0x80 for U+20AC in a GBK / GB2312 file (what GBK readers expect), keeps
//! GB18030's A2 E3 when the file used it, and still writes everything else GBK cannot hold as GB18030 does.

use bilingua_model::enc::{decode_text, encode_gbk, encode_text, DecodeOptions};
use bilingua_model::Limits;

fn opts(lang: Option<&'static str>) -> DecodeOptions<'static> {
    DecodeOptions { declared: true, source_lang: lang, limits: Limits::default() }
}

#[test]
fn euro_is_one_byte_in_gbk() {
    // "price: " + euro + " " + Chinese, as a GBK file (euro = 0x80)
    let bytes: Vec<u8> = [b"price: ".as_slice(), &[0x80], b" ", &[0xD6, 0xD0, 0xCE, 0xC4], b"\r\n"].concat();
    let d = decode_text(&bytes, &opts(Some("zh-CN"))).unwrap();
    assert_eq!(d.text, "price: \u{20AC} \u{4E2D}\u{6587}\r\n");
    assert_eq!(d.encoding, "gbk;nl=crlf");
    // untouched merge is the same bytes
    assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), bytes);
    // new text with a euro sign: 0x80
    let out = encode_text("\u{20AC}100 \u{4E2D}\r\n", &d.encoding).unwrap();
    assert_eq!(out, [&[0x80][..], b"100 ", &[0xD6, 0xD0], b"\r\n"].concat());
}

#[test]
fn a_file_that_used_a2e3_keeps_it() {
    let bytes: Vec<u8> = [b"x ".as_slice(), &[0xA2, 0xE3], b" ", &[0xD6, 0xD0], b"\n"].concat();
    let d = decode_text(&bytes, &opts(Some("zh-CN"))).unwrap();
    assert!(d.text.contains('\u{20AC}'));
    assert!(d.encoding.contains("euro=a2e3"), "{}", d.encoding);
    assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), bytes);
}

#[test]
fn declared_gb2312_and_cp936_labels_get_the_single_byte_too() {
    for label in ["gb2312", "GBK", "x-gbk", "chinese", "gb_2312"] {
        let head = format!("<?xml version=\"1.0\" encoding=\"{label}\"?>\n<a>");
        let mut bytes = head.clone().into_bytes();
        bytes.extend_from_slice(&[0xD6, 0xD0]);
        bytes.extend_from_slice(b"</a>\n");
        let d = decode_text(&bytes, &opts(None)).unwrap();
        assert!(d.encoding.starts_with("gbk"), "{label}: {}", d.encoding);
        let out = encode_text(&d.text.replace("</a>", "\u{20AC}</a>"), &d.encoding).unwrap();
        let mut want = bytes.clone();
        let at = want.len() - 5;
        want.insert(at, 0x80);
        assert_eq!(out, want, "{label}");
    }
}

#[test]
fn characters_gbk_cannot_hold_still_use_gb18030_forms() {
    // U+00E9 has a two-byte GBK code, U+1F600 a four-byte GB18030 code; neither falls back to UTF-8
    let (b, bad) = encode_gbk("\u{20AC}\u{E9}\u{1F600}");
    assert!(!bad);
    assert_eq!(b[0], 0x80);
    let (back, _, errs) = bilingua_model::encoding_rs::GB18030.decode(&b[1..]);
    assert!(!errs);
    assert_eq!(back, "\u{E9}\u{1F600}");
    let tag = "gbk";
    let out = encode_text("\u{20AC}\u{1F600}", tag).unwrap();
    assert_eq!(out[0], 0x80);
    assert_eq!(out.len(), 5);
}

#[test]
fn other_code_pages_are_unchanged() {
    let out = encode_text("\u{20AC}", "windows-1252").unwrap();
    assert_eq!(out, vec![0x80]);
    let out = encode_text("\u{20AC}", "big5").unwrap();
    assert_ne!(out, vec![0x80]);
}
