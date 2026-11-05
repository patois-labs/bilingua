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
fn euc_kr_make_up_sequences_read_and_write_as_python() {
    let d = doc(b"euc-kr", b"\xa4\xd4\xa4\xa8\xa4\xc7\xa4\xb1 X");
    assert_eq!(first(&d), [0xb620, 0x20, 0x58]);
    assert_eq!(build(&HhcFilter, &d, &[]), d);
    let out = build(&HhcFilter, &d, &[(1, "\u{b620}")]);
    assert_eq!(out, b"<meta charset=euc-kr><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"\xa4\xd4\xa4\xa8\xa4\xc7\xa4\xb1 X\"><param name=\"Name\" value=\"\xa4\xd4\xa4\xa8\xa4\xc7\xa4\xb1\"></OBJECT>");
    // a broken make-up sequence is not EUC-KR (Python refuses it), so the file reads as Windows-1252
    let bad = doc(b"euc-kr", b"\xa4\xd4\xa4\xa8 X");
    assert_eq!(tag_of(&bad), "windows-1252");
}

fn tag_of(data: &[u8]) -> String {
    match extract(&HhcFilter, data).unwrap().skeleton {
        Skeleton::Text { encoding, .. } => encoding,
        _ => unreachable!(),
    }
}

#[test]
fn a_uhc_byte_under_a_ks_c_5601_label_reads_as_windows_1252_as_in_locos() {
    // Python maps ks_c_5601-1987 to its strict euc_kr codec, which has no UHC extension (LocOS bug, see the report)
    let d = doc(b"ks_c_5601-1987", b"\x8c\x63 X");
    assert_eq!(first(&d), [0x152, 0x63, 0x20, 0x58]);
    assert_eq!(build(&HhcFilter, &d, &[(1, "x")]), b"<meta charset=ks_c_5601-1987><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"\x8cc X\"><param name=\"Name\" value=\"x\"></OBJECT>");
}

#[test]
fn iso_8859_9_is_not_windows_1254() {
    let d = doc(b"iso-8859-9", b"\x80\xdd X");
    assert_eq!(first(&d), [0x80, 0x130, 0x20, 0x58]);
    assert_eq!(build(&HhcFilter, &d, &[(1, "\u{11e}")]), b"<meta charset=iso-8859-9><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"\x80\xdd X\"><param name=\"Name\" value=\"\xd0\"></OBJECT>");
}

#[test]
fn gb2312_writes_python_bytes() {
    // U+30FB and U+2015: Python's gb2312 writes A1A4 and A1AA where encoding_rs's GBK differs or cannot
    let d = doc(b"gb2312", b"\xb0\xa1");
    let out = build(&HhcFilter, &d, &[(1, "a\u{30fb}\u{2015}")]);
    assert!(out.windows(5).any(|w| w == b"a\xa1\xa4\xa1\xaa"), "{out:?}");
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
