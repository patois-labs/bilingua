//! BILI-57: real drawings that declare `ANSI_950` (cp950) carry Hong Kong (HKSCS) characters, which Big5-HKSCS
//! adds to Big5 at byte pairs cp950 leaves undefined. Such a drawing reads with those characters, merges back
//! byte for byte, and a translation may use them; a cp950-only drawing keeps the cp950 tag.
mod common;
use bilingua_dxf::Dxf;
use bilingua_model::*;
use common::*;

/// A drawing with `$DWGCODEPAGE` ANSI_950 whose TEXT value is `value` (raw bytes).
fn drawing(value: &[u8]) -> Vec<u8> {
    let mut pairs = header("AC1015", "ANSI_950");
    pairs.extend(p(&[(0, "SECTION"), (2, "ENTITIES"), (0, "TEXT"), (8, "0"), (1, "@@VALUE@@"), (0, "ENDSEC"), (0, "EOF")]));
    let s = dxf(&pairs, "\r\n", utf8);
    let at = s.windows(9).position(|w| w == b"@@VALUE@@").unwrap();
    let mut out = s[..at].to_vec();
    out.extend_from_slice(value);
    out.extend_from_slice(&s[at + 9..]);
    out
}

fn tag(doc: &Document) -> String {
    match &doc.skeleton {
        Skeleton::Text { encoding, .. } => encoding.clone(),
        _ => unreachable!(),
    }
}

// 0x87 0x40 = U+43F0 and 0x88 0x40 = U+31C0 (HKSCS lead bytes below 0xA1), 0xFA 0x40 = U+20547 (HKSCS in the
// cp950 user-defined area); 0xA4 0xA4 = U+4E2D in cp950 itself. Python's big5hkscs reads them so too.
const HK: &[u8] = &[b'R', b'o', b'o', b'm', b' ', 0x87, 0x40, 0x88, 0x40, 0xA4, 0xA4, 0xFA, 0x40];

#[test]
fn hkscs_bytes_in_a_cp950_drawing_are_read() {
    let data = drawing(HK);
    let doc = extract(&data).expect("HKSCS bytes in a cp950 drawing");
    assert_eq!(doc.units.len(), 1);
    assert_eq!(plain_text(&doc.units[0].source()), "Room \u{43F0}\u{31C0}\u{4E2D}\u{20547}");
    assert!(tag(&doc).ends_with("cp950-hkscs"), "{}", tag(&doc));
    assert_eq!(Dxf.merge(&doc).unwrap(), data, "untouched merge is byte for byte");
}

#[test]
fn a_translation_may_use_hkscs_characters() {
    let data = drawing(HK);
    let mut doc = extract(&data).unwrap();
    doc.units[0].segments[0].target = vec![Inline::text("\u{43F0}\u{4E2D}\u{20547} x")];
    let out = Dxf.merge(&doc).unwrap();
    let want: &[u8] = &[0x87, 0x40, 0xA4, 0xA4, 0xFA, 0x40, b' ', b'x'];
    assert!(out.windows(want.len()).any(|w| w == want), "HKSCS characters written as their bytes");
    let back = extract(&out).unwrap();
    assert_eq!(plain_text(&back.units[0].source()), "\u{43F0}\u{4E2D}\u{20547} x");
}

#[test]
fn a_cp950_only_drawing_keeps_the_cp950_tag_and_bad_bytes_are_still_refused() {
    let data = drawing(&[b'A', 0xA4, 0xA4]);
    let doc = extract(&data).unwrap();
    assert!(tag(&doc).ends_with("cp950") && !tag(&doc).contains("hkscs"), "{}", tag(&doc));
    // a lead byte with no trail is no character in either table
    match extract(&drawing(&[b'A', 0xA4])) {
        Err(Error::Encoding(m)) => assert!(m.contains("cp950"), "{m}"),
        other => panic!("{other:?}"),
    }
    // a pair neither cp950 nor HKSCS defines
    match extract(&drawing(&[b'A', 0xA4, 0x30, b'B'])) {
        Err(Error::Encoding(_)) => {}
        other => panic!("{other:?}"),
    }
}
