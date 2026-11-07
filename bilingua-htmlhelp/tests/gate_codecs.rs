//! Gate (11-05): Python-exact code pages, untouched bytes, and the model's merge options. Expected bytes are what the
//! Localization OS filter produces for the same input (its venv, read only).
mod common;
use bilingua_htmlhelp::{HhcFilter, HhpFilter, UTF8_FALLBACK};
use bilingua_model::*;
use common::*;

fn doc(cs: &[u8], v1: &[u8]) -> Vec<u8> {
    let mut d = b"<meta charset=".to_vec();
    d.extend_from_slice(cs);
    d.extend_from_slice(b"><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"");
    d.extend_from_slice(v1);
    d.extend_from_slice(b"\"><param name=\"Name\" value=\"Two\"></OBJECT>");
    d
}

fn first(data: &[u8]) -> Vec<u32> {
    texts(&HhcFilter, data)[0].chars().map(|c| c as u32).collect()
}

#[test]
fn cp932_nec_duplicates_keep_their_bytes_untouched_and_normalise_when_edited() {
    let d = doc(b"cp932", b"\x87\x90 A");
    assert_eq!(first(&d), [0x2252, 0x20, 0x41]);
    assert_eq!(build(&HhcFilter, &d, &[]), d);
    let out = build(&HhcFilter, &d, &[(1, "\u{65e5}")]);
    assert_eq!(out, b"<meta charset=cp932><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"\x81\xe0 A\"><param name=\"Name\" value=\"\x93\xfa\"></OBJECT>");
}

#[test]
fn euc_kr_label_reads_as_windows_949_so_a_localized_korean_export_reads_back() {
    // Browsers and HTML Help Workshop read euc-kr and ks_c_5601-1987 as Windows-949; LocOS's strict euc_kr could not
    // read the UHC syllables its own export writes (issue 796). The Annex 3 make-up sequence is no longer special.
    let d = doc(b"euc-kr", b"\x8c\x63 X");
    assert_eq!(first(&d), [0xb620, 0x20, 0x58]);
    assert_eq!(build(&HhcFilter, &d, &[]), d);
    let d = doc(b"ks_c_5601-1987", b"\x8c\x63 X");
    assert_eq!(first(&d), [0xb620, 0x20, 0x58]);
    let out = build(&HhcFilter, &d, &[(1, "\u{b620}")]);
    assert_eq!(out, b"<meta charset=ks_c_5601-1987><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"\x8cc X\"><param name=\"Name\" value=\"\x8cc\"></OBJECT>");
}

#[test]
fn gb2312_shift_jis_and_big5_labels_read_the_windows_supersets() {
    // a GBK-only hanzi, NEC row 13 U+2460, and the cp950 euro sign
    let d = doc(b"gb2312", b"\x84\x4a");
    assert!(first(&d)[0] > 0x3000, "{:?}", first(&d));
    let d = doc(b"shift_jis", b"\x87\x40");
    assert_eq!(first(&d), [0x2460]);
    let d = doc(b"big5", b"\xa3\xe1 X");
    assert_eq!(first(&d), [0x20ac, 0x20, 0x58]);
}

#[test]
fn iso_8859_9_is_not_windows_1254() {
    let d = doc(b"iso-8859-9", b"\x80\xdd X");
    assert_eq!(first(&d), [0x80, 0x130, 0x20, 0x58]);
    assert_eq!(build(&HhcFilter, &d, &[(1, "\u{11e}")]), b"<meta charset=iso-8859-9><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"\x80\xdd X\"><param name=\"Name\" value=\"\xd0\"></OBJECT>");
}

#[test]
fn gb2312_writes_python_bytes() {
    // U+00B7: the gb2312 label is read and written as GBK (A1A4)
    let d = doc(b"gb2312", b"\xb0\xa1");
    let out = build(&HhcFilter, &d, &[(1, "a\u{b7}")]);
    assert!(out.windows(3).any(|w| w == b"a\xa1\xa4"), "{out:?}");
}

#[test]
fn stamp_target_always_localizes_with_nothing_translated() {
    let hhp = cp1252(HHP);
    assert_eq!(merge_with(&HhpFilter, &hhp, &[], Some("fr-FR"), MergeOptions::default()).unwrap(), hhp);
    let mo = MergeOptions::new().with_stamp_target_always(true);
    let out = String::from_utf8(merge_with(&HhpFilter, &hhp, &[], Some("fr-FR"), mo.clone()).unwrap()).unwrap();
    assert!(out.contains("Language=0x40c French (France)"), "{out}");
    // a sitemap: the target code page and the Font charset
    let hhc = cp1252(HHC);
    let out = merge_with(&HhcFilter, &hhc, &[], Some("pl-PL"), mo.clone()).unwrap();
    assert!(out.windows(14).any(|w| w == b"Tahoma,8,238\">"), "{}", String::from_utf8_lossy(&out));
}

#[test]
fn output_encoding_utf8_writes_utf8_and_says_so() {
    let hhk = cp1252(HHK);
    let mo = MergeOptions::new().with_output_encoding(OutputEncoding::Utf8);
    let out = String::from_utf8(merge_with(&HhkFilter, &hhk, &[(0, "\u{43f}\u{435}\u{447}\u{430}\u{442}\u{44c}")], Some("ru-RU"), mo.clone()).unwrap()).unwrap();
    assert!(out.contains("charset=UTF-8\"") && out.contains("value=\"\u{43f}\u{435}\u{447}\u{430}\u{442}\u{44c}\""), "{out}");
    // a UTF-8 BOM stays, a UTF-16 one goes
    let mut bom = b"\xef\xbb\xbf".to_vec();
    bom.extend_from_slice(HHC.as_bytes());
    assert!(merge_with(&HhcFilter, &bom, &[(0, "x")], None, mo.clone()).unwrap().starts_with(b"\xef\xbb\xbf<!DOCTYPE"));
    let mut u16 = vec![0xFF, 0xFE];
    u16.extend(HHC.encode_utf16().flat_map(|u| u.to_le_bytes()));
    assert!(merge_with(&HhcFilter, &u16, &[(0, "x")], None, mo.clone()).unwrap().starts_with(b"<!DOCTYPE"));
}

#[test]
fn utf8_fallback_is_a_format_option() {
    let hhc = cp1252(HHC);
    let greek = "\u{3b5}\u{3bb}";
    assert!(merge_full(&HhcFilter, &hhc, &[(0, "\u{416}")], Some("el-GR"), false, MergeOptions::default()).is_err());
    let out = merge_full(&HhcFilter, &hhc, &[(0, "\u{416}"), (1, greek)], Some("el-GR"), true, MergeOptions::default()).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("\u{416}"));
    assert_eq!(UTF8_FALLBACK, "htmlhelp.utf8_fallback");
}

use bilingua_htmlhelp::HhkFilter;
