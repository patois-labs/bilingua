//! `MergeOptions` and `enc::encode_text_with`: output_encoding "utf-8" writes UTF-8 and keeps declarations truthful.

use bilingua_model::enc::{decode_text, encode_text, encode_text_with, DecodeOptions};
use bilingua_model::{Limits, MergeOptions, OutputEncoding};

fn dec(bytes: &[u8]) -> bilingua_model::enc::Decoded {
    decode_text(bytes, &DecodeOptions::with_limits(Limits::default()).declared()).unwrap()
}

fn utf8() -> MergeOptions {
    MergeOptions::new().with_output_encoding(OutputEncoding::Utf8)
}

fn legacy(e: &'static bilingua_model::encoding_rs::Encoding, s: &str) -> Vec<u8> {
    let (b, _, bad) = e.encode(s);
    assert!(!bad);
    b.into_owned()
}

#[test]
fn legacy_xml_becomes_utf8_with_a_truthful_declaration() {
    let src = "<?xml version=\"1.0\" encoding=\"windows-1253\"?>\r\n<a>\u{3b1}\u{3b2}</a>\r\n";
    let bytes = legacy(bilingua_model::encoding_rs::WINDOWS_1253, src);
    let d = dec(&bytes);
    assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), bytes);
    let out = encode_text_with(&d.text, &d.encoding, &utf8()).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), src.replace("windows-1253", "UTF-8"));
    // the default options are plain encode_text
    assert_eq!(encode_text_with(&d.text, &d.encoding, &MergeOptions::default()).unwrap(), bytes);
}

#[test]
fn utf16_html_drops_the_bom_and_says_utf8() {
    let src = "<meta charset=\"utf-16\"><p>caf\u{e9}</p>\r\n";
    let mut b = vec![0xFF, 0xFE];
    for u in src.encode_utf16() {
        b.extend_from_slice(&u.to_le_bytes());
    }
    let d = dec(&b);
    assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
    let out = String::from_utf8(encode_text_with(&d.text, &d.encoding, &utf8()).unwrap()).unwrap();
    assert_eq!(out, "<meta charset=\"UTF-8\"><p>caf\u{e9}</p>\r\n");
}

#[test]
fn po_charset_is_rewritten() {
    let po = legacy(
        bilingua_model::encoding_rs::ISO_8859_2,
        "msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=ISO-8859-2\\n\"\n\nmsgid \"a\"\nmsgstr \"\u{15b}\"\n",
    );
    let d = dec(&po);
    let out = String::from_utf8(encode_text_with(&d.text, &d.encoding, &utf8()).unwrap()).unwrap();
    assert!(out.contains("charset=UTF-8") && out.contains('\u{15b}'), "{out}");
}

#[test]
fn a_utf8_file_with_a_false_declaration_is_fixed_and_keeps_its_bom() {
    let src = "<?xml version=\"1.0\" encoding=\"iso-8859-2\"?><a>\u{15b}</a>";
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend_from_slice(src.as_bytes());
    let d = dec(&b);
    assert_eq!(d.encoding, "utf-8-bom;label=iso-8859-2");
    // an untouched merge still returns the original bytes
    assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
    let out = encode_text_with(&d.text, &d.encoding, &utf8()).unwrap();
    assert_eq!(&out[..3], &[0xEF, 0xBB, 0xBF]);
    assert_eq!(std::str::from_utf8(&out[3..]).unwrap(), src.replace("iso-8859-2", "UTF-8"));
}

#[test]
fn plain_utf8_and_json_text_are_left_alone() {
    let src = "{\"charset\": \"x\"} \u{e9}\n";
    let d = dec(src.as_bytes());
    assert_eq!(encode_text_with(&d.text, &d.encoding, &utf8()).unwrap(), src.as_bytes());
}

#[test]
fn options_serde_and_helpers() {
    let o: MergeOptions = serde_json::from_str(r#"{"output_encoding":"utf-8","stamp_target_always":true}"#).unwrap();
    assert_eq!(o, MergeOptions::locos());
    assert_eq!(serde_json::from_str::<MergeOptions>("{}").unwrap(), MergeOptions::default());
    assert_eq!(serde_json::from_str::<MergeOptions>(r#"{"output_encoding":"keep"}"#).unwrap().output_encoding, OutputEncoding::Keep);
    assert!(serde_json::from_str::<MergeOptions>(r#"{"bogus":1}"#).is_err());
}

#[test]
fn format_options_flags() {
    let mut o = bilingua_model::ExtractOptions::default();
    o.format_options.insert("ftl.translate_terms".into(), "True".into());
    o.format_options.insert("x.y".into(), "maybe".into());
    assert_eq!(o.format_flag("ftl.translate_terms"), Some(true));
    assert_eq!(o.format_flag("x.y"), None);
    assert_eq!(o.format_flag("nope"), None);
    assert_eq!(o.format_option("x.y"), Some("maybe"));
}
